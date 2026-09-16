use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    CreateTimePunchByCard, CreateTimecardCard, TimePunchFilter, TimePunchWithEmployee,
    TimePunchesResponse, TimecardCard, TimecardCardBulkUpsert, TimecardCardDeleteByCard,
    TimecardCardDeleteResult, TimecardCardUpsertSummary, MAX_BULK_UPSERT_ITEMS,
};
use alc_core::repository::timecard::{
    is_valid_bulk_card_id, normalize_card_id, prepare_bulk_cards,
};
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/timecard/cards", post(create_card).get(list_cards))
        .route("/timecard/cards/{id}", get(get_card).delete(delete_card))
        // 社員番号キーの一括取り込み (Refs ippoan/rust-alc-api#644)。
        // **tenant_router に置く** — 呼び出し元 (Worker) は auth-worker の
        // forwardAlcTenantData 経由で来て X-Tenant-ID は auth-worker が注入する。
        // `PUT /employees/bulk-by-code` とまったく同じ経路
        .route(
            "/timecard/cards/bulk-by-code",
            put(bulk_upsert_cards_by_code),
        )
        // 継続同期の削除側 (Refs ippoan/rust-alc-api#644)。
        // **POST だけを登録する** — 呼び出し元 (auth-worker の転送 allowlist) は
        // method を見ず path だけで通すので、閉じるのはこちら側の責務。
        // 登録が POST 1 つなら axum が他 method に 405 を返す
        .route(
            "/timecard/cards/delete-by-card",
            post(delete_card_by_card_id),
        )
        .route(
            "/timecard/cards/by-card/{card_id}",
            get(get_card_by_card_id),
        )
        .route("/timecard/punch", post(punch))
        .route("/timecard/punches", get(list_punches))
        .route("/timecard/punches/csv", get(export_csv))
}

// --- Card CRUD ---

async fn create_card(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTimecardCard>,
) -> Result<(StatusCode, Json<TimecardCard>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let card = state
        .timecard
        .create_card(
            tenant_id,
            body.employee_id,
            // 登録も照合と同じ正規化形で入れる。読み側だけ正規化すると
            // `ABC` と `abc` が同時に一致し得て打刻が別人に着く
            &normalize_card_id(&body.card_id),
            body.label.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("create_card error: {e}");
            if e.to_string().contains("idx_timecard_cards_unique") {
                return StatusCode::CONFLICT;
            }
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(card)))
}

/// `PUT /timecard/cards/bulk-by-code` — 社員番号 (code) キーのカード台帳一括 upsert
/// (Refs ippoan/rust-alc-api#644)。
///
/// 既存タイムカード (別システム) の中央 DB にある台帳を、こちらの `timecard_cards`
/// へ初回移行するための口。**単発 CRUD をループで叩かせない**のが眼目で、
/// 「既にある / 別人に付いている / 新規」の判定と code→UUID の解決を受け側に閉じる
/// (呼び出し元は public repo の Worker なので、判定を向こうに持たせると二重化する)。
///
/// **skip があっても 200。** 1 件の不備で全体を落とすと、数百件の移行が
/// 1 行のせいで前に進まなくなる。
async fn bulk_upsert_cards_by_code(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<TimecardCardBulkUpsert>,
) -> Result<(StatusCode, Json<TimecardCardUpsertSummary>), (StatusCode, String)> {
    if body.items.is_empty() || body.items.len() > MAX_BULK_UPSERT_ITEMS {
        return Err((StatusCode::BAD_REQUEST, "items が不正です".to_string()));
    }

    // 正規化・形の検査・バッチ内重複は DB を引かずに決まる (pure)。
    // **card_id の正規化点はここ 1 か所** — 送り手は大文字の生値を送ってくる
    let (prepared, invalid) = prepare_bulk_cards(&body.items);

    let mut summary = state
        .timecard
        .bulk_upsert_cards_by_code(tenant.0 .0, &prepared, body.on_conflict, body.dry_run)
        .await
        .map_err(|e| {
            // ★ card_id はログにも出さない (応答と同じ理由)
            tracing::error!("bulk_upsert_cards_by_code error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            )
        })?;

    // 送り手が items を走査しながら突き合わせられるよう index 順に戻す
    summary.skipped.extend(invalid);
    summary.skipped.sort_by_key(|s| s.index);

    Ok((StatusCode::OK, Json(summary)))
}

/// `POST /timecard/cards/delete-by-card` — 同期で入れたカード 1 枚を外す
/// (Refs ippoan/rust-alc-api#644)。
///
/// 外部の画面でカードを削除したときに 1 枚ずつ反映するための口。追加側は
/// `PUT /timecard/cards/bulk-by-code` を items 1 件で再利用するので、専用の口はこれだけ。
///
/// **消す範囲は `source = CARD_SOURCE_LEDGER_SYNC` の行ちょうど。** `timecard_cards` には
/// この同期と無関係に alc 側で直接登録されたカード (`source IS NULL`) が在り得るので、
/// 無条件に消すとそれを巻き込む。
///
/// **範囲外・不在・形不正はどれも 200 + `reason`。** 404 にすると呼び出し元の画面が
/// 「消えた」と誤解する文面を出す。`code` (`employees.code`) は「誰のカードを外したか」を
/// 画面に出すために返す。
///
/// **`card_id` は応答にも `tracing` のログにも出さない** (一括取り込みと同じ理由)。
async fn delete_card_by_card_id(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<TimecardCardDeleteByCard>,
) -> Result<(StatusCode, Json<TimecardCardDeleteResult>), (StatusCode, String)> {
    // 正規化も形の検査も一括取り込みと同じ関数を通す。2 実装目を作ると
    // 「同期では入るのに削除では当たらない」が生まれる
    let card_id = normalize_card_id(&body.card_id);
    if !is_valid_bulk_card_id(&card_id) {
        return Ok((
            StatusCode::OK,
            Json(TimecardCardDeleteResult {
                deleted: 0,
                reason: "invalid_card_id".to_string(),
                code: None,
            }),
        ));
    }

    let result = state
        .timecard
        .delete_card_by_card_id_from_sync(tenant.0 .0, &card_id, body.dry_run)
        .await
        .map_err(|e| {
            // ★ card_id はログにも出さない (応答と同じ理由)
            tracing::error!("delete_card_by_card_id error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            )
        })?;

    Ok((StatusCode::OK, Json(result)))
}

#[derive(Debug, serde::Deserialize)]
struct CardFilter {
    employee_id: Option<Uuid>,
}

async fn list_cards(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<CardFilter>,
) -> Result<Json<Vec<TimecardCard>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let cards = state
        .timecard
        .list_cards(tenant_id, filter.employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(cards))
}

async fn get_card(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TimecardCard>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let card = state
        .timecard
        .get_card(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(card))
}

async fn get_card_by_card_id(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(card_id): Path<String>,
) -> Result<Json<TimecardCard>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let card = state
        .timecard
        .get_card_by_card_id(tenant_id, &normalize_card_id(&card_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(card))
}

async fn delete_card(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;

    let deleted = state
        .timecard
        .delete_card(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- Punch ---

async fn punch(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTimePunchByCard>,
) -> Result<(StatusCode, Json<TimePunchWithEmployee>), StatusCode> {
    let tenant_id = tenant.0 .0;

    // カードIDから社員を特定 (timecard_cards -> employees.nfc_id フォールバック)。
    // 照合は NFC タイムカード端末の打刻中継 (alc-devices の hub_measurements、
    // Refs ippoan/alc-app-s3#134) と**同じ 1 か所**を使う — 2 実装目を作ると
    // どちらか片方だけフォールバックがズレる
    let employee_id = alc_core::repository::timecard::resolve_employee_by_card(
        state.timecard.as_ref(),
        tenant_id,
        &body.card_id,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // 打刻記録
    let punch = state
        .timecard
        .create_punch(tenant_id, employee_id, body.device_id, &body.card_id)
        .await
        .map_err(|e| {
            tracing::error!("punch error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 社員名を取得
    let employee_name = state
        .timecard
        .get_employee_name(tenant_id, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 当日の打刻一覧
    let today_punches = state
        .timecard
        .list_today_punches(tenant_id, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        Json(TimePunchWithEmployee {
            punch,
            employee_name,
            today_punches,
        }),
    ))
}

// --- List Punches ---

async fn list_punches(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TimePunchFilter>,
) -> Result<Json<TimePunchesResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let per_page = filter.per_page.unwrap_or(50).min(200);
    let page = filter.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let total = state
        .timecard
        .count_punches(
            tenant_id,
            filter.employee_id,
            filter.date_from,
            filter.date_to,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let punches = state
        .timecard
        .list_punches(
            tenant_id,
            filter.employee_id,
            filter.date_from,
            filter.date_to,
            per_page,
            offset,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TimePunchesResponse {
        punches,
        total,
        page,
        per_page,
    }))
}

// --- CSV Export ---

/// CSV の「区分」列の表示名。未知の値はそのまま出す (隠すと診断できない)
fn csv_kind_label(kind: &str) -> String {
    match kind {
        "timecard" => "打刻".to_string(),
        "license" => "点呼".to_string(),
        other => other.to_string(),
    }
}

async fn export_csv(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TimePunchFilter>,
) -> Result<impl IntoResponse, StatusCode> {
    let tenant_id = tenant.0 .0;

    let rows = state
        .timecard
        .list_punches_for_csv(
            tenant_id,
            filter.employee_id,
            filter.date_from,
            filter.date_to,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut wtr = csv::Writer::from_writer(vec![]);
    // **区分列は必須。** 一覧は打刻 (timecard) と点呼 (license) を両方返すので、
    // 列が無いと点呼が打刻として集計される
    wtr.write_record(["ID", "区分", "社員コード", "社員名", "打刻日時", "デバイス"])
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for r in &rows {
        wtr.write_record([
            r.id.to_string(),
            csv_kind_label(&r.kind),
            r.employee_code.clone().unwrap_or_default(),
            r.employee_name.clone().unwrap_or_default(),
            r.punched_at
                .with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).unwrap())
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            r.device_name.clone().unwrap_or_default(),
        ])
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let csv_data = wtr
        .into_inner()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut output = vec![0xEF, 0xBB, 0xBF];
    output.extend_from_slice(&csv_data);

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"time_punches.csv\"",
            ),
        ],
        output,
    ))
}
