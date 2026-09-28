//! 指静脈 (vein) の Worker (Refs #680 / #683)。crates/alc-vein の 4 本の口を
//! workers-rs + Hyperdrive で提供する。
//!
//! - `PUT /vein/templates/{employee_id}` `{"charas": [hex, ...]}` → 登録し直す
//! - `POST /vein/identify` `{"chara": hex}` → 1:N 照合 + 学習の書き戻し
//! - `GET /vein/templates` → オフライン照合用の全件 (`logic_version` つき)
//! - `DELETE /vein/templates/{employee_id}` → 登録の削除
//!
//! monolith と同じく `/api` 付きでも受ける。
//!
//! **この Worker は JWT を検証せず、auth-worker が付け直した `X-Tenant-ID` を信頼する。**
//! 到達経路は auth-worker からの Service Binding だけで、`workers_dev` / `preview_urls` /
//! `routes` を持たない (wrangler.toml と scripts/check-exposure.sh が保証する。#556 と同じ穴を
//! 開けないため)。
//!
//! 段階 A: DB (`db`) と X-Tenant-ID の解析 (`tenant_id`) は worker 内の最小実装、
//! 照合 (`matcher`) は alc-vein の写し。段階 B で alc-vein / alc-core-wasm に置き換える。

mod db;
mod matcher;

use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
use worker::{
    console_log, event, Context, Date, Env, Request, Response, Result, RouteContext, Router,
};

use crate::matcher::{CharaError, MAX_TEMPLATES};

fn json_response(status: u16, body: &Value) -> Result<Response> {
    Ok(Response::from_json(body)?.with_status(status))
}

fn unprocessable(error: &str, message: &str) -> Result<Response> {
    json_response(422, &json!({ "error": error, "message": message }))
}

fn not_found(error: &str) -> Result<Response> {
    json_response(404, &json!({ "error": error }))
}

fn internal_error(detail: &str) -> Result<Response> {
    worker::console_error!("internal error: {detail}");
    json_response(500, &json!({ "error": "internal_error" }))
}

fn chara_error(e: CharaError) -> Result<Response> {
    unprocessable(e.code(), e.message())
}

/// auth-worker が付け直した `X-Tenant-ID` (欠落・不正は 401。monolith の
/// `require_tenant_header` と同じ規約)。
fn tenant_id(req: &Request) -> Option<Uuid> {
    req.headers()
        .get("X-Tenant-ID")
        .ok()
        .flatten()
        .and_then(|v| Uuid::parse_str(&v).ok())
}

fn employee_id(ctx: &RouteContext<()>) -> Option<Uuid> {
    ctx.param("employee_id")
        .and_then(|v| Uuid::parse_str(v).ok())
}

/// handler の `Result<Response, Result<Response>>` を畳む (`?` で早期に応答を返すため)。
macro_rules! try_resp {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(resp) => return resp,
        }
    };
}

fn require_tenant(req: &Request) -> std::result::Result<Uuid, Result<Response>> {
    tenant_id(req).ok_or_else(|| Response::error("Unauthorized", 401))
}

#[derive(Deserialize)]
struct UpsertBody {
    charas: Vec<String>,
}

#[derive(Deserialize)]
struct IdentifyBody {
    chara: String,
}

async fn upsert_template(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let tenant_id = try_resp!(require_tenant(&req));
    let Some(employee_id) = employee_id(&ctx) else {
        return Response::error("Bad Request", 400);
    };
    let Ok(body) = req.json::<UpsertBody>().await else {
        return Response::error("Bad Request", 400);
    };
    let charas = match body
        .charas
        .iter()
        .map(|c| matcher::decode_chara(c))
        .collect::<std::result::Result<Vec<_>, _>>()
    {
        Ok(c) => c,
        Err(e) => return chara_error(e),
    };
    let Ok(template) = matcher::enroll_template(&charas) else {
        return unprocessable(
            "invalid_chara_count",
            "登録の特徴量は 1〜6 件で送ってください",
        );
    };
    let mut client = match db::connect(&ctx.env).await {
        Ok(c) => c,
        Err(e) => return internal_error(&e),
    };
    let tx = match db::tenant_tx(&mut client, tenant_id).await {
        Ok(tx) => tx,
        Err(e) => return internal_error(&e),
    };
    // 新規登録で上限を超えさせない (上書きは人数が増えないので通す)
    let (registered, enrolled) = match db::registration_count(&tx, tenant_id, employee_id).await {
        Ok(v) => v,
        Err(e) => return internal_error(&e),
    };
    if matcher::check_capacity(registered as usize + usize::from(!enrolled)).is_err() {
        let message = format!("登録が上限の {MAX_TEMPLATES} 人に達しています");
        return unprocessable("too_many_templates", &message);
    }
    let updated_at = match db::upsert(&tx, tenant_id, employee_id, &template).await {
        Ok(Some(t)) => t,
        Ok(None) => return not_found("employee_not_found"),
        Err(e) => return internal_error(&e),
    };
    if let Err(e) = tx.commit().await {
        return internal_error(&format!("commit: {e}"));
    }
    json_response(
        200,
        &json!({ "employee_id": employee_id, "updated_at": updated_at }),
    )
}

async fn identify(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let tenant_id = try_resp!(require_tenant(&req));
    let Ok(body) = req.json::<IdentifyBody>().await else {
        return Response::error("Bad Request", 400);
    };
    let chara = match matcher::decode_chara(&body.chara) {
        Ok(c) => c,
        Err(e) => return chara_error(e),
    };
    let mut client = match db::connect(&ctx.env).await {
        Ok(c) => c,
        Err(e) => return internal_error(&e),
    };
    let tx = match db::tenant_tx(&mut client, tenant_id).await {
        Ok(tx) => tx,
        Err(e) => return internal_error(&e),
    };
    let list_started = Date::now().as_millis();
    let rows = match db::list(&tx, tenant_id).await {
        Ok(r) => r,
        Err(e) => return internal_error(&e),
    };
    let list_ms = Date::now().as_millis() - list_started;
    let templates: Vec<&str> = rows.iter().map(|r| r.template.as_str()).collect();
    let t8 = chrono::Utc::now().timestamp() as u8;
    // 照合の CPU 時間の測定 (#683)。I/O を挟まない区間なので Date.now の差分で測る。
    // ローカル (wrangler dev) の Date.now は CPU 実行中も進むが、Cloudflare 上では
    // Spectre 対策で I/O まで止まるので 0 になる (本番の CPU 時間は dashboard / tail で見る)
    let started = Date::now().as_millis();
    let found = matcher::identify(&templates, &chara, t8);
    let identify_ms = Date::now().as_millis() - started;
    let n = templates.len();
    console_log!("vein identify: templates={n} list_ms={list_ms} identify_ms={identify_ms}");
    let found = match found {
        Ok(f) => f,
        Err(e) => {
            let message = format!(
                "登録が {} 人あり、1:N 照合の上限 {MAX_TEMPLATES} 人を超えています",
                e.0
            );
            let resp = unprocessable("too_many_templates", &message)?;
            resp.headers()
                .set("Server-Timing", &format!("list;dur={list_ms}"))?;
            return Ok(resp);
        }
    };
    for &i in &found.unreadable {
        let employee_id = rows[i].employee_id;
        console_log!("vein identify: {employee_id} のテンプレートを読めず照合から外した");
    }
    let timing = format!("list;dur={list_ms}, identify;dur={identify_ms}");
    let Some(hit) = found.hit else {
        let resp = json_response(200, &json!({ "employee_id": null }))?;
        resp.headers().set("Server-Timing", &timing)?;
        return Ok(resp);
    };
    let row = &rows[hit.index];
    // 学習は次の照合でまた起きるので、書き戻しの競合 (0 行) や失敗は捨ててよい
    let written = db::update_learned(&tx, tenant_id, row.id, &hit.learned, row.updated_at).await;
    let committed = tx.commit().await.map_err(|e| e.to_string());
    let employee_id = row.employee_id;
    console_log!(
        "vein identify: {employee_id} learned write-back {written:?} commit {committed:?}"
    );
    let resp = json_response(
        200,
        &json!({ "employee_id": row.employee_id, "name": row.name }),
    )?;
    resp.headers().set("Server-Timing", &timing)?;
    Ok(resp)
}

async fn list_templates(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let tenant_id = try_resp!(require_tenant(&req));
    let mut client = match db::connect(&ctx.env).await {
        Ok(c) => c,
        Err(e) => return internal_error(&e),
    };
    let tx = match db::tenant_tx(&mut client, tenant_id).await {
        Ok(tx) => tx,
        Err(e) => return internal_error(&e),
    };
    let rows = match db::list(&tx, tenant_id).await {
        Ok(r) => r,
        Err(e) => return internal_error(&e),
    };
    // 読むだけなので COMMIT せず drop (= ROLLBACK) でよいが、明示して閉じる
    if let Err(e) = tx.commit().await {
        return internal_error(&format!("commit: {e}"));
    }
    let templates: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "employee_id": r.employee_id,
                "template": r.template,
                "updated_at": r.updated_at,
            })
        })
        .collect();
    json_response(
        200,
        &json!({
            "logic_version": vein_match_search::VERSION,
            "templates": templates,
        }),
    )
}

async fn delete_template(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let tenant_id = try_resp!(require_tenant(&req));
    let Some(employee_id) = employee_id(&ctx) else {
        return Response::error("Bad Request", 400);
    };
    let mut client = match db::connect(&ctx.env).await {
        Ok(c) => c,
        Err(e) => return internal_error(&e),
    };
    let tx = match db::tenant_tx(&mut client, tenant_id).await {
        Ok(tx) => tx,
        Err(e) => return internal_error(&e),
    };
    let deleted = match db::delete(&tx, tenant_id, employee_id).await {
        Ok(d) => d,
        Err(e) => return internal_error(&e),
    };
    if let Err(e) = tx.commit().await {
        return internal_error(&format!("commit: {e}"));
    }
    if deleted {
        Ok(Response::empty()?.with_status(204))
    } else {
        not_found("vein_template_not_found")
    }
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let mut router = Router::new();
    for prefix in ["", "/api"] {
        router = router
            .get_async(&format!("{prefix}/vein/templates"), list_templates)
            .put_async(
                &format!("{prefix}/vein/templates/:employee_id"),
                upsert_template,
            )
            .delete_async(
                &format!("{prefix}/vein/templates/:employee_id"),
                delete_template,
            )
            .post_async(&format!("{prefix}/vein/identify"), identify);
    }
    router.run(req, env).await
}
