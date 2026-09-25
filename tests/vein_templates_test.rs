//! 指静脈テンプレート (`vein_templates`、migration 151) の実 DB 統合テスト
//! (Refs ippoan/vein-match#20)。
//!
//! 口の分岐 (422 / 0〜2 人 / 501 人 / 書き戻しの競合) は alc-vein の単体テスト
//! (`crates/alc-vein/src/routes_tests.rs`) が fake の repo で見る。ここは SQL の側 —
//! `ON CONFLICT (tenant_id, employee_id)` の upsert・`updated_at` を条件にした書き戻し・
//! 削除済みの乗務員の除外・RLS のテナント分離 — と、登録 → 照合で当たる → 学習後の
//! テンプレートが書き戻される、の一連を固定する。
//!
//! 特徴量は `alc_vein::matcher::synth` の合成 (0xBDBD 構造体)。vein-match の固定データ
//! (実機の特徴量) は private repo のものなので、public のこの repo には置かない。

#[macro_use]
mod common;

use alc_vein::matcher::synth;
use alc_vein::repo::{PgVeinTemplatesRepository, VeinTemplatesRepository};
use serde_json::{json, Value};
use uuid::Uuid;

fn hex(seed: u64) -> String {
    synth::hex(&synth::chara(seed))
}

struct Ctx {
    base_url: String,
    auth: String,
    client: reqwest::Client,
    tenant_id: Uuid,
    pool: sqlx::PgPool,
}

async fn setup(name: &str) -> Ctx {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(state.pool(), name).await;
    let auth = format!("Bearer {}", common::create_test_jwt(tenant_id, "admin"));
    Ctx {
        base_url,
        auth,
        client: reqwest::Client::new(),
        tenant_id,
        pool: state.pool().clone(),
    }
}

impl Ctx {
    async fn employee(&self, name: &str) -> Uuid {
        let code = format!("V{}", &Uuid::new_v4().simple().to_string()[..8]);
        let e = common::create_test_employee(&self.client, &self.base_url, &self.auth, name, &code)
            .await;
        e["id"].as_str().unwrap().parse().unwrap()
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut req = self
            .client
            .request(method, format!("{}/api{path}", self.base_url))
            .header("Authorization", &self.auth);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().await.unwrap();
        let status = res.status().as_u16();
        (status, res.json().await.unwrap_or(Value::Null))
    }

    async fn put_template(&self, employee_id: Uuid, seed: u64) -> (u16, Value) {
        let body = json!({ "charas": [hex(seed), hex(seed)] });
        let path = format!("/vein/templates/{employee_id}");
        self.send(reqwest::Method::PUT, &path, Some(body)).await
    }

    async fn identify(&self, seed: u64) -> Value {
        let body = json!({ "chara": hex(seed) });
        let (status, v) = self
            .send(reqwest::Method::POST, "/vein/identify", Some(body))
            .await;
        assert_eq!(status, 200, "{v}");
        v
    }

    async fn list(&self) -> Value {
        let (status, v) = self
            .send(reqwest::Method::GET, "/vein/templates", None)
            .await;
        assert_eq!(status, 200, "{v}");
        v
    }
}

#[tokio::test]
async fn test_vein_enroll_identify_writes_back_learned_template() {
    test_group!("指静脈: 登録 → 照合 → 学習の書き戻し");
    test_case!(
        "当たった乗務員の名前を返し、学習後のテンプレートを DB に書き戻す",
        {
            let ctx = setup("Vein Roundtrip Tenant").await;
            let yamada = ctx.employee("山田 太郎").await;
            let suzuki = ctx.employee("鈴木 花子").await;

            let (status, put) = ctx.put_template(yamada, 11).await;
            assert_eq!(status, 200, "{put}");
            assert_eq!(put["employee_id"], yamada.to_string());
            assert!(put["updated_at"].is_string());
            let (status, _) = ctx.put_template(suzuki, 12).await;
            assert_eq!(status, 200);

            let before = ctx.list().await;
            assert_eq!(before["logic_version"], "0.1.1");
            assert_eq!(before["templates"].as_array().unwrap().len(), 2);
            let row = |v: &Value, id: Uuid| {
                v["templates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|t| t["employee_id"] == id.to_string())
                    .unwrap()
                    .clone()
            };
            let yamada_before = row(&before, yamada);

            let hit = ctx.identify(11).await;
            assert_eq!(hit, json!({ "employee_id": yamada, "name": "山田 太郎" }));

            // 学習後のテンプレートが書き戻され、updated_at が進む (鈴木さんの行は触らない)
            let after = ctx.list().await;
            let yamada_after = row(&after, yamada);
            assert_ne!(yamada_after["template"], yamada_before["template"]);
            assert_ne!(yamada_after["updated_at"], yamada_before["updated_at"]);
            assert_eq!(row(&after, suzuki), row(&before, suzuki));

            // 書き戻したテンプレートでも同じ指で当たる (import_temp_b64 で読み戻せる)
            let learned = yamada_after["template"].as_str().unwrap();
            let again = alc_vein::matcher::identify(&[learned], &synth::chara(11), 0).unwrap();
            assert_eq!(again.unreadable, Vec::<usize>::new());
            assert!(again.hit.is_some());

            // 登録していない指は外れ
            assert_eq!(ctx.identify(13).await, json!({ "employee_id": null }));
        }
    );
}

#[tokio::test]
async fn test_vein_put_upserts_one_row_per_employee() {
    test_group!("指静脈: 登録し直し");
    test_case!(
        "同じ乗務員の PUT は 1 行を上書きし、別の指に置き換わる",
        {
            let ctx = setup("Vein Upsert Tenant").await;
            let yamada = ctx.employee("山田").await;
            assert_eq!(ctx.put_template(yamada, 21).await.0, 200);
            assert_eq!(ctx.put_template(yamada, 22).await.0, 200);
            let list = ctx.list().await;
            assert_eq!(list["templates"].as_array().unwrap().len(), 1);
            assert_eq!(ctx.identify(22).await["employee_id"], yamada.to_string());
            assert_eq!(ctx.identify(21).await["employee_id"], Value::Null);

            // 未対応の形式は 422 で、登録は変わらない (照合の学習で変わった後の一覧と比べる)
            let list = ctx.list().await;
            let body = json!({ "charas": ["9911AABB"] });
            let path = format!("/vein/templates/{yamada}");
            let (status, v) = ctx.send(reqwest::Method::PUT, &path, Some(body)).await;
            assert_eq!(status, 422);
            assert_eq!(v["error"], "unsupported_chara_format");
            assert!(ctx.list().await == list, "422 の PUT で登録が変わった");
        }
    );
}

#[tokio::test]
async fn test_vein_update_learned_discards_on_conflict() {
    test_group!("指静脈: 書き戻しの競合");
    test_case!(
        "読んだ後に登録し直されていたら、学習の書き戻しは 0 行で捨てる",
        {
            let ctx = setup("Vein Conflict Tenant").await;
            let yamada = ctx.employee("山田").await;
            assert_eq!(ctx.put_template(yamada, 31).await.0, 200);
            let repo = PgVeinTemplatesRepository::new(ctx.pool.clone());
            let read = repo.list(ctx.tenant_id).await.unwrap().remove(0);

            // 間に登録し直し (updated_at が進む)
            assert_eq!(ctx.put_template(yamada, 32).await.0, 200);
            let written = repo
                .update_learned(ctx.tenant_id, read.id, "stale", read.updated_at)
                .await
                .unwrap();
            assert!(!written);
            let now = repo.list(ctx.tenant_id).await.unwrap().remove(0);
            assert_ne!(now.template, "stale");

            // 読んだ値のままなら書ける
            let written = repo
                .update_learned(ctx.tenant_id, now.id, "fresh", now.updated_at)
                .await
                .unwrap();
            assert!(written);
            let fresh = repo.list(ctx.tenant_id).await.unwrap().remove(0);
            assert_eq!(fresh.template, "fresh");
            assert!(fresh.updated_at > now.updated_at);
        }
    );
}

#[tokio::test]
async fn test_vein_tenant_isolation_and_deleted_employees() {
    test_group!("指静脈: テナント分離 / 削除");
    test_case!(
        "別テナントからは見えず・当たらず・登録できない。削除済みの乗務員は除く",
        {
            let a = setup("Vein Tenant A").await;
            let b = setup("Vein Tenant B").await;
            let yamada = a.employee("山田").await;
            let tanaka = a.employee("田中").await;
            assert_eq!(a.put_template(yamada, 41).await.0, 200);
            assert_eq!(a.put_template(tanaka, 42).await.0, 200);

            assert_eq!(b.list().await["templates"], json!([]));
            assert_eq!(b.identify(41).await, json!({ "employee_id": null }));
            let (status, v) = b.put_template(yamada, 41).await;
            assert_eq!(
                (status, v["error"].clone()),
                (404, json!("employee_not_found"))
            );

            // 乗務員を削除 (soft delete) すると、一覧にも照合にも出ず、登録もできない
            let (status, _) = a
                .send(
                    reqwest::Method::DELETE,
                    &format!("/employees/{tanaka}"),
                    None,
                )
                .await;
            assert_eq!(status, 204);
            let list = a.list().await;
            assert_eq!(list["templates"].as_array().unwrap().len(), 1);
            assert_eq!(a.identify(42).await, json!({ "employee_id": null }));
            assert_eq!(a.put_template(tanaka, 42).await.0, 404);

            // テンプレートの削除: 204 → 2 回目は 404。別テナントからは消せない
            let path = format!("/vein/templates/{yamada}");
            assert_eq!(b.send(reqwest::Method::DELETE, &path, None).await.0, 404);
            assert_eq!(a.send(reqwest::Method::DELETE, &path, None).await.0, 204);
            let (status, v) = a.send(reqwest::Method::DELETE, &path, None).await;
            assert_eq!(
                (status, v["error"].clone()),
                (404, json!("vein_template_not_found"))
            );
            assert_eq!(a.list().await["templates"], json!([]));
        }
    );
}
