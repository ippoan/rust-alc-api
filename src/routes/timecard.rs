use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{
    CreateTimePunchByCard, CreateTimecardCard, TimePunch, TimePunchFilter, TimePunchWithDevice,
    TimePunchWithEmployee, TimePunchesResponse, TimecardCard,
};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/timecard/cards", post(create_card).get(list_cards))
        .route("/timecard/cards/{id}", get(get_card).delete(delete_card))
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let card = sqlx::query_as::<_, TimecardCard>(
        r#"
        INSERT INTO timecard_cards (tenant_id, employee_id, card_id, label)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(body.employee_id)
    .bind(&body.card_id)
    .bind(&body.label)
    .fetch_one(&mut *conn)
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let cards = if let Some(eid) = filter.employee_id {
        sqlx::query_as::<_, TimecardCard>(
            "SELECT * FROM timecard_cards WHERE tenant_id = $1 AND employee_id = $2 ORDER BY created_at",
        )
        .bind(tenant_id)
        .bind(eid)
        .fetch_all(&mut *conn)
        .await
    } else {
        sqlx::query_as::<_, TimecardCard>(
            "SELECT * FROM timecard_cards WHERE tenant_id = $1 ORDER BY created_at",
        )
        .bind(tenant_id)
        .fetch_all(&mut *conn)
        .await
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(cards))
}

async fn get_card(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TimecardCard>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let card = sqlx::query_as::<_, TimecardCard>(
        "SELECT * FROM timecard_cards WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let card = sqlx::query_as::<_, TimecardCard>(
        "SELECT * FROM timecard_cards WHERE tenant_id = $1 AND card_id = $2",
    )
    .bind(tenant_id)
    .bind(&card_id)
    .fetch_optional(&mut *conn)
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM timecard_cards WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // カードIDから社員を特定 (timecard_cards → employees.nfc_id フォールバック)
    let employee_id = if let Some(card) = sqlx::query_as::<_, TimecardCard>(
        "SELECT * FROM timecard_cards WHERE tenant_id = $1 AND card_id = $2",
    )
    .bind(tenant_id)
    .bind(&body.card_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        card.employee_id
    } else {
        // フォールバック: employees.nfc_id で検索
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM employees WHERE tenant_id = $1 AND nfc_id = $2",
        )
        .bind(tenant_id)
        .bind(&body.card_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?
    };

    // 打刻記録
    let punch = sqlx::query_as::<_, TimePunch>(
        r#"
        INSERT INTO time_punches (tenant_id, employee_id, device_id)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(employee_id)
    .bind(body.device_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("punch error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 社員名を取得
    let employee_name: String =
        sqlx::query_scalar("SELECT name FROM employees WHERE id = $1 AND tenant_id = $2")
            .bind(employee_id)
            .bind(tenant_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 当日の打刻一覧
    let today_punches = sqlx::query_as::<_, TimePunch>(
        r#"
        SELECT * FROM time_punches
        WHERE tenant_id = $1 AND employee_id = $2
          AND punched_at >= CURRENT_DATE
        ORDER BY punched_at
        "#,
    )
    .bind(tenant_id)
    .bind(employee_id)
    .fetch_all(&mut *conn)
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

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut conditions = vec!["tp.tenant_id = $1".to_string()];
    let mut param_idx = 2u32;

    if filter.employee_id.is_some() {
        conditions.push(format!("tp.employee_id = ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_from.is_some() {
        conditions.push(format!("tp.punched_at >= ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_to.is_some() {
        conditions.push(format!("tp.punched_at <= ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = conditions.join(" AND ");

    // Count
    let count_sql = format!("SELECT COUNT(*) FROM time_punches tp WHERE {where_clause}");
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
    if let Some(eid) = filter.employee_id {
        count_query = count_query.bind(eid);
    }
    if let Some(df) = filter.date_from {
        count_query = count_query.bind(df);
    }
    if let Some(dt) = filter.date_to {
        count_query = count_query.bind(dt);
    }
    let total = count_query
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // List
    let sql = format!(
        r#"SELECT tp.id, tp.tenant_id, tp.employee_id, tp.device_id, d.device_name,
                  e.name as employee_name, tp.punched_at, tp.created_at
           FROM time_punches tp
           LEFT JOIN devices d ON d.id = tp.device_id
           LEFT JOIN employees e ON e.id = tp.employee_id
           WHERE {where_clause}
           ORDER BY tp.punched_at DESC LIMIT ${param_idx} OFFSET ${}"#,
        param_idx + 1
    );
    let mut query = sqlx::query_as::<_, TimePunchWithDevice>(&sql).bind(tenant_id);
    if let Some(eid) = filter.employee_id {
        query = query.bind(eid);
    }
    if let Some(df) = filter.date_from {
        query = query.bind(df);
    }
    if let Some(dt) = filter.date_to {
        query = query.bind(dt);
    }
    query = query.bind(per_page).bind(offset);

    let punches = query
        .fetch_all(&mut *conn)
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

async fn export_csv(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TimePunchFilter>,
) -> Result<impl IntoResponse, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut conditions = vec!["tp.tenant_id = $1".to_string()];
    let mut param_idx = 2u32;

    if filter.employee_id.is_some() {
        conditions.push(format!("tp.employee_id = ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_from.is_some() {
        conditions.push(format!("tp.punched_at >= ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_to.is_some() {
        conditions.push(format!("tp.punched_at <= ${param_idx}"));
        param_idx += 1;
    }
    let _ = param_idx;

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        r#"
        SELECT tp.id, tp.punched_at, e.name as employee_name, e.code as employee_code,
               d.device_name
        FROM time_punches tp
        JOIN employees e ON e.id = tp.employee_id
        LEFT JOIN devices d ON d.id = tp.device_id
        WHERE {where_clause}
        ORDER BY tp.punched_at DESC
        "#
    );

    #[derive(sqlx::FromRow)]
    struct CsvRow {
        id: Uuid,
        punched_at: chrono::DateTime<chrono::Utc>,
        employee_name: String,
        employee_code: Option<String>,
        device_name: Option<String>,
    }

    let mut query = sqlx::query_as::<_, CsvRow>(&sql).bind(tenant_id);
    if let Some(eid) = filter.employee_id {
        query = query.bind(eid);
    }
    if let Some(df) = filter.date_from {
        query = query.bind(df);
    }
    if let Some(dt) = filter.date_to {
        query = query.bind(dt);
    }

    let rows = query
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.write_record(["ID", "社員コード", "社員名", "打刻日時", "デバイス"])
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for r in &rows {
        wtr.write_record([
            r.id.to_string(),
            r.employee_code.clone().unwrap_or_default(),
            r.employee_name.clone(),
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
