use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::db::models::{
    CancelTenkoSession, EmployeeHealthBaseline, InterruptSession, MedicalDiffs, ResumeSession,
    SafetyJudgment, SelfDeclaration, StartTenkoSession, SubmitAlcoholResult,
    SubmitCarryingItemChecks, SubmitDailyInspection, SubmitMedicalData, SubmitOperationReport,
    SubmitSelfDeclaration, TenkoDashboard, TenkoRecord, TenkoSchedule, TenkoSession,
    TenkoSessionFilter, TenkoSessionsResponse,
};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::{AuthUser, TenantId};
use crate::AppState;

/// JWT 必須ルート (管理者)
/// テナント対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/tenko/sessions", get(list_sessions))
        .route("/tenko/dashboard", get(dashboard))
        .route("/tenko/sessions/{id}/interrupt", post(interrupt_session))
        .route("/tenko/sessions/{id}/resume", post(resume_session))
        .route("/tenko/sessions/start", post(start_session))
        .route("/tenko/sessions/{id}", get(get_session))
        .route("/tenko/sessions/{id}/alcohol", put(submit_alcohol))
        .route("/tenko/sessions/{id}/medical", put(submit_medical))
        .route(
            "/tenko/sessions/{id}/instruction-confirm",
            put(confirm_instruction),
        )
        .route("/tenko/sessions/{id}/report", put(submit_report))
        .route("/tenko/sessions/{id}/cancel", post(cancel_session))
        .route(
            "/tenko/sessions/{id}/self-declaration",
            put(submit_self_declaration),
        )
        .route(
            "/tenko/sessions/{id}/daily-inspection",
            put(submit_daily_inspection),
        )
        .route(
            "/tenko/sessions/{id}/carrying-items",
            put(submit_carrying_items),
        )
}

/// セッション開始 (顔認証完了後)
async fn start_session(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<StartTenkoSession>,
) -> Result<(StatusCode, Json<TenkoSession>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // スケジュールあり / なし (遠隔点呼) で分岐
    let (tenko_type, responsible_manager_name, schedule_id_for_insert) =
        if let Some(sid) = body.schedule_id {
            // スケジュール検証: 存在・未消費・乗務員一致
            let schedule = sqlx::query_as::<_, TenkoSchedule>(
                "SELECT * FROM tenko_schedules WHERE id = $1 AND tenant_id = $2 AND consumed = FALSE",
            )
            .bind(sid)
            .bind(tenant_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

            if schedule.employee_id != body.employee_id {
                return Err(StatusCode::BAD_REQUEST);
            }

            // スケジュール消費
            sqlx::query(
                "UPDATE tenko_schedules SET consumed = TRUE, updated_at = NOW() WHERE id = $1",
            )
            .bind(schedule.id)
            .execute(&mut *conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let tt = schedule.tenko_type.clone();
            let rmn = schedule.responsible_manager_name.clone();
            let sid = schedule.id;

            // consumed_by_session_id は session 作成後に更新するため schedule.id を返す
            (tt, Some(rmn), Some(sid))
        } else {
            // 遠隔点呼: スケジュールなし
            let tt = body
                .tenko_type
                .clone()
                .unwrap_or_else(|| "pre_operation".to_string());
            (tt, None::<String>, None::<Uuid>)
        };

    // セッション作成 (業務前は体温・血圧から開始)
    let initial_status = match tenko_type.as_str() {
        "pre_operation" => "medical_pending",
        _ => "identity_verified",
    };

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        INSERT INTO tenko_sessions (
            tenant_id, employee_id, schedule_id, tenko_type, status,
            identity_verified_at, identity_face_photo_url, location,
            responsible_manager_name, started_at
        )
        VALUES ($1, $2, $3, $4, $8, NOW(), $5, $6, $7, NOW())
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(body.employee_id)
    .bind(schedule_id_for_insert)
    .bind(&tenko_type)
    .bind(&body.identity_face_photo_url)
    .bind(&body.location)
    .bind(&responsible_manager_name)
    .bind(initial_status)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("start_session DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // スケジュールありの場合: consumed_by_session_id を更新
    if let Some(sid) = schedule_id_for_insert {
        sqlx::query("UPDATE tenko_schedules SET consumed_by_session_id = $1 WHERE id = $2")
            .bind(session.id)
            .bind(sid)
            .execute(&mut *conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok((StatusCode::CREATED, Json(session)))
}

/// アルコール結果送信
async fn submit_alcohol(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitAlcoholResult>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let valid_results = ["pass", "fail", "normal", "over", "error"];
    if !valid_results.contains(&body.alcohol_result.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if session.status != "identity_verified" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let is_fail = matches!(body.alcohol_result.as_str(), "fail" | "over");

    let next_status = if is_fail {
        "cancelled"
    } else {
        match session.tenko_type.as_str() {
            "pre_operation" => "instruction_pending",
            "post_operation" => "report_pending",
            _ => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    };

    let cancel_reason = if is_fail {
        Some("アルコール検知".to_string())
    } else {
        None
    };

    let completed_at = if is_fail { Some(Utc::now()) } else { None };

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            status = $1,
            measurement_id = $2,
            alcohol_result = $3,
            alcohol_value = $4,
            alcohol_tested_at = NOW(),
            alcohol_face_photo_url = $5,
            cancel_reason = $6,
            completed_at = $7,
            updated_at = NOW()
        WHERE id = $8 AND tenant_id = $9
        RETURNING *
        "#,
    )
    .bind(next_status)
    .bind(body.measurement_id)
    .bind(&body.alcohol_result)
    .bind(body.alcohol_value)
    .bind(&body.alcohol_face_photo_url)
    .bind(&cancel_reason)
    .bind(completed_at)
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("submit_alcohol DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if is_fail {
        // 不変レコード作成
        let _ = create_tenko_record(&mut conn, &session, tenant_id).await;

        // Webhook: alcohol_detected
        let employee_name: Option<String> =
            sqlx::query_scalar("SELECT name FROM employees WHERE id = $1")
                .bind(session.employee_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let payload = serde_json::json!({
            "event": "alcohol_detected",
            "timestamp": Utc::now(),
            "tenant_id": tenant_id,
            "data": {
                "session_id": session.id,
                "employee_id": session.employee_id,
                "employee_name": employee_name.unwrap_or_default(),
                "alcohol_value": body.alcohol_value,
                "alcohol_result": body.alcohol_result,
                "responsible_manager_name": session.responsible_manager_name,
                "tenko_type": session.tenko_type,
            }
        });

        let pool = state.pool.clone();
        tokio::spawn(async move {
            if let Err(e) =
                crate::webhook::fire_event(&pool, tenant_id, "alcohol_detected", payload).await
            {
                tracing::error!("Webhook fire_event error: {e}");
            }
        });
    }

    Ok(Json(session))
}

/// 医療データ送信 (業務前のみ)
async fn submit_medical(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitMedicalData>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // 業務前のみ + 適切な状態
    if session.tenko_type != "pre_operation" {
        return Err(StatusCode::BAD_REQUEST);
    }
    if session.status != "medical_pending" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            status = 'self_declaration_pending',
            temperature = COALESCE($1, temperature),
            systolic = COALESCE($2, systolic),
            diastolic = COALESCE($3, diastolic),
            pulse = COALESCE($4, pulse),
            medical_measured_at = COALESCE($5, NOW()),
            medical_manual_input = $6,
            updated_at = NOW()
        WHERE id = $7 AND tenant_id = $8
        RETURNING *
        "#,
    )
    .bind(body.temperature)
    .bind(body.systolic)
    .bind(body.diastolic)
    .bind(body.pulse)
    .bind(body.medical_measured_at)
    .bind(body.medical_manual_input)
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("submit_medical DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(session))
}

/// 指示事項確認
async fn confirm_instruction(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if session.status != "instruction_pending" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            status = 'completed',
            instruction_confirmed_at = NOW(),
            completed_at = NOW(),
            updated_at = NOW()
        WHERE id = $1 AND tenant_id = $2
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("confirm_instruction DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 不変レコード作成
    let _ = create_tenko_record(&mut conn, &session, tenant_id).await;

    Ok(Json(session))
}

/// 運行状況報告 (業務後のみ)
async fn submit_report(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitOperationReport>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    // 両項目とも必須（テキストまたは "報告なし"）
    if body.vehicle_road_status.trim().is_empty() || body.driver_alternation.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if session.tenko_type != "post_operation" || session.status != "report_pending" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 指示事項があるか確認
    let instruction: Option<String> = sqlx::query_scalar(
        "SELECT instruction FROM tenko_schedules WHERE id = $1",
    )
    .bind(session.schedule_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .flatten();

    let next_status = if instruction.is_some() {
        "instruction_pending"
    } else {
        "completed"
    };

    let completed_at = if next_status == "completed" {
        Some(Utc::now())
    } else {
        None
    };

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            status = $1,
            report_vehicle_road_status = $2,
            report_driver_alternation = $3,
            report_vehicle_road_audio_url = $4,
            report_driver_alternation_audio_url = $5,
            report_submitted_at = NOW(),
            completed_at = $6,
            updated_at = NOW()
        WHERE id = $7 AND tenant_id = $8
        RETURNING *
        "#,
    )
    .bind(next_status)
    .bind(&body.vehicle_road_status)
    .bind(&body.driver_alternation)
    .bind(&body.vehicle_road_audio_url)
    .bind(&body.driver_alternation_audio_url)
    .bind(completed_at)
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("submit_report DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 指示事項なしで完了した場合、レコード作成
    if next_status == "completed" {
        let _ = create_tenko_record(&mut conn, &session, tenant_id).await;
    }

    // Webhook: report_submitted イベント発火
    {
        let payload = serde_json::json!({
            "event": "report_submitted",
            "timestamp": Utc::now(),
            "tenant_id": tenant_id,
            "data": {
                "session_id": session.id,
                "employee_id": session.employee_id,
                "vehicle_road_status": body.vehicle_road_status,
                "driver_alternation": body.driver_alternation,
                "vehicle_road_audio_url": body.vehicle_road_audio_url,
                "driver_alternation_audio_url": body.driver_alternation_audio_url,
            }
        });

        let pool = state.pool.clone();
        tokio::spawn(async move {
            if let Err(e) =
                crate::webhook::fire_event(&pool, tenant_id, "report_submitted", payload).await
            {
                tracing::error!("Webhook fire_event error: {e}");
            }
        });
    }

    Ok(Json(session))
}

/// セッション中止
async fn cancel_session(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<CancelTenkoSession>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // 既に終了状態なら不可
    if matches!(session.status.as_str(), "completed" | "cancelled") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            status = 'cancelled',
            cancel_reason = $1,
            completed_at = NOW(),
            updated_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        RETURNING *
        "#,
    )
    .bind(&body.reason)
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("cancel_session DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let _ = create_tenko_record(&mut conn, &session, tenant_id).await;

    Ok(Json(session))
}

/// セッション取得
async fn get_session(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(session))
}

/// セッション一覧 (管理者)
async fn list_sessions(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TenkoSessionFilter>,
) -> Result<Json<TenkoSessionsResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let per_page = filter.per_page.unwrap_or(50).min(100);
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

    let mut conditions = vec!["s.tenant_id = $1".to_string()];
    let mut param_idx = 2u32;

    if filter.employee_id.is_some() {
        conditions.push(format!("s.employee_id = ${param_idx}"));
        param_idx += 1;
    }
    if filter.status.is_some() {
        conditions.push(format!("s.status = ${param_idx}"));
        param_idx += 1;
    }
    if filter.tenko_type.is_some() {
        conditions.push(format!("s.tenko_type = ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_from.is_some() {
        conditions.push(format!("s.started_at >= ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_to.is_some() {
        conditions.push(format!("s.started_at <= ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = conditions.join(" AND ");

    let count_sql = format!("SELECT COUNT(*) FROM tenko_sessions s WHERE {where_clause}");
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
    if let Some(employee_id) = filter.employee_id {
        count_query = count_query.bind(employee_id);
    }
    if let Some(ref status) = filter.status {
        count_query = count_query.bind(status);
    }
    if let Some(ref tenko_type) = filter.tenko_type {
        count_query = count_query.bind(tenko_type);
    }
    if let Some(date_from) = filter.date_from {
        count_query = count_query.bind(date_from);
    }
    if let Some(date_to) = filter.date_to {
        count_query = count_query.bind(date_to);
    }
    let total = count_query
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let sql = format!(
        "SELECT s.* FROM tenko_sessions s WHERE {where_clause} ORDER BY s.created_at DESC LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    );
    let mut query = sqlx::query_as::<_, TenkoSession>(&sql).bind(tenant_id);
    if let Some(employee_id) = filter.employee_id {
        query = query.bind(employee_id);
    }
    if let Some(ref status) = filter.status {
        query = query.bind(status);
    }
    if let Some(ref tenko_type) = filter.tenko_type {
        query = query.bind(tenko_type);
    }
    if let Some(date_from) = filter.date_from {
        query = query.bind(date_from);
    }
    if let Some(date_to) = filter.date_to {
        query = query.bind(date_to);
    }
    query = query.bind(per_page).bind(offset);

    let sessions = query
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TenkoSessionsResponse {
        sessions,
        total,
        page,
        per_page,
    }))
}

/// ダッシュボード集計
async fn dashboard(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<TenkoDashboard>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let pending_schedules: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tenko_schedules WHERE tenant_id = $1 AND consumed = FALSE",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let active_sessions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tenko_sessions WHERE tenant_id = $1 AND status NOT IN ('completed', 'cancelled', 'interrupted')",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let interrupted_sessions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tenko_sessions WHERE tenant_id = $1 AND status = 'interrupted'",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let completed_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tenko_sessions WHERE tenant_id = $1 AND status = 'completed' AND completed_at >= CURRENT_DATE",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let cancelled_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tenko_sessions WHERE tenant_id = $1 AND status = 'cancelled' AND completed_at >= CURRENT_DATE",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let overdue_minutes: i64 = std::env::var("TENKO_OVERDUE_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let overdue_schedules = sqlx::query_as::<_, TenkoSchedule>(
        r#"
        SELECT * FROM tenko_schedules
        WHERE tenant_id = $1
          AND consumed = FALSE
          AND scheduled_at + ($2 || ' minutes')::INTERVAL < NOW()
        ORDER BY scheduled_at ASC
        "#,
    )
    .bind(tenant_id)
    .bind(overdue_minutes.to_string())
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TenkoDashboard {
        pending_schedules,
        active_sessions,
        interrupted_sessions,
        completed_today,
        cancelled_today,
        overdue_schedules,
    }))
}

/// 不変レコード作成
async fn create_tenko_record(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    session: &TenkoSession,
    tenant_id: Uuid,
) -> Result<TenkoRecord, StatusCode> {
    let employee_name: String =
        sqlx::query_scalar("SELECT name FROM employees WHERE id = $1")
            .bind(session.employee_id)
            .fetch_one(&mut **conn)
            .await
            .map_err(|e| {
                tracing::error!("create_tenko_record: employee lookup error: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let instruction: Option<String> =
        sqlx::query_scalar("SELECT instruction FROM tenko_schedules WHERE id = $1")
            .bind(session.schedule_id)
            .fetch_optional(&mut **conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .flatten();

    let record_data = serde_json::to_value(session).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let canonical =
        serde_json::to_string(&record_data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let hash = format!("{:x}", Sha256::digest(canonical.as_bytes()));

    let has_face_photo = session.alcohol_face_photo_url.is_some();

    let record = sqlx::query_as::<_, TenkoRecord>(
        r#"
        INSERT INTO tenko_records (
            tenant_id, session_id, employee_id, tenko_type, status,
            record_data, employee_name, responsible_manager_name,
            location, alcohol_result, alcohol_value, alcohol_has_face_photo,
            temperature, systolic, diastolic, pulse,
            instruction, instruction_confirmed_at,
            report_vehicle_road_status, report_driver_alternation, report_no_report,
            report_vehicle_road_audio_url, report_driver_alternation_audio_url,
            started_at, completed_at, record_hash,
            self_declaration, safety_judgment, daily_inspection,
            interrupted_at, resumed_at, resume_reason
        )
        VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8,
            $9, $10, $11, $12,
            $13, $14, $15, $16,
            $17, $18,
            $19, $20, $21,
            $22, $23,
            $24, $25, $26,
            $27, $28, $29,
            $30, $31, $32
        )
        RETURNING *
        "#,
    )
    .bind(tenant_id)
    .bind(session.id)
    .bind(session.employee_id)
    .bind(&session.tenko_type)
    .bind(&session.status)
    .bind(&record_data)
    .bind(&employee_name)
    .bind(&session.responsible_manager_name)
    .bind(&session.location)
    .bind(&session.alcohol_result)
    .bind(session.alcohol_value)
    .bind(has_face_photo)
    .bind(session.temperature)
    .bind(session.systolic)
    .bind(session.diastolic)
    .bind(session.pulse)
    .bind(&instruction)
    .bind(session.instruction_confirmed_at)
    .bind(&session.report_vehicle_road_status)
    .bind(&session.report_driver_alternation)
    .bind(session.report_no_report)
    .bind(&session.report_vehicle_road_audio_url)
    .bind(&session.report_driver_alternation_audio_url)
    .bind(session.started_at)
    .bind(session.completed_at)
    .bind(&hash)
    .bind(&session.self_declaration)
    .bind(&session.safety_judgment)
    .bind(&session.daily_inspection)
    .bind(session.interrupted_at)
    .bind(session.resumed_at)
    .bind(&session.resume_reason)
    .fetch_one(&mut **conn)
    .await
    .map_err(|e| {
        tracing::error!("create_tenko_record DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(record)
}

/// 自己申告送信 (業務前のみ)
async fn submit_self_declaration(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitSelfDeclaration>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if session.tenko_type != "pre_operation" || session.status != "self_declaration_pending" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 自己申告データ保存
    let declaration = SelfDeclaration {
        illness: body.illness,
        fatigue: body.fatigue,
        sleep_deprivation: body.sleep_deprivation,
        declared_at: Utc::now(),
    };
    let declaration_json =
        serde_json::to_value(&declaration).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            self_declaration = $1,
            updated_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        RETURNING *
        "#,
    )
    .bind(&declaration_json)
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("submit_self_declaration DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 安全判定を自動実行
    let session =
        perform_safety_judgment(&mut conn, &session, tenant_id, &state.pool).await?;

    Ok(Json(session))
}

/// 安全運転可否の自動判定 (内部ヘルパー)
async fn perform_safety_judgment(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    session: &TenkoSession,
    tenant_id: Uuid,
    pool: &sqlx::PgPool,
) -> Result<TenkoSession, StatusCode> {
    let mut failed_items: Vec<String> = Vec::new();
    let mut medical_diffs = MedicalDiffs {
        systolic_diff: None,
        diastolic_diff: None,
        temperature_diff: None,
    };

    // 基準値取得
    let baseline = sqlx::query_as::<_, EmployeeHealthBaseline>(
        "SELECT * FROM employee_health_baselines WHERE tenant_id = $1 AND employee_id = $2",
    )
    .bind(tenant_id)
    .bind(session.employee_id)
    .fetch_optional(&mut **conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(bl) = &baseline {
        // 血圧・体温の判定
        if let Some(systolic) = session.systolic {
            let diff = systolic - bl.baseline_systolic;
            medical_diffs.systolic_diff = Some(diff);
            if diff.abs() > bl.systolic_tolerance {
                failed_items.push("systolic".to_string());
            }
        }
        if let Some(diastolic) = session.diastolic {
            let diff = diastolic - bl.baseline_diastolic;
            medical_diffs.diastolic_diff = Some(diff);
            if diff.abs() > bl.diastolic_tolerance {
                failed_items.push("diastolic".to_string());
            }
        }
        if let Some(temperature) = session.temperature {
            let diff = temperature - bl.baseline_temperature;
            medical_diffs.temperature_diff = Some(diff);
            if diff.abs() > bl.temperature_tolerance {
                failed_items.push("temperature".to_string());
            }
        }
    } else {
        tracing::warn!(
            "No health baseline for employee {}, defaulting to pass",
            session.employee_id
        );
    }

    // 自己申告チェック
    if let Some(ref decl_json) = session.self_declaration {
        if let Ok(decl) = serde_json::from_value::<SelfDeclaration>(decl_json.clone()) {
            if decl.illness {
                failed_items.push("illness".to_string());
            }
            if decl.fatigue {
                failed_items.push("fatigue".to_string());
            }
            if decl.sleep_deprivation {
                failed_items.push("sleep_deprivation".to_string());
            }
        }
    }

    let judgment_status = if failed_items.is_empty() {
        "pass"
    } else {
        "fail"
    };

    let judgment = SafetyJudgment {
        status: judgment_status.to_string(),
        failed_items: failed_items.clone(),
        judged_at: Utc::now(),
        medical_diffs: Some(medical_diffs),
    };
    let judgment_json =
        serde_json::to_value(&judgment).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let next_status = if judgment_status == "pass" {
        "daily_inspection_pending"
    } else {
        "interrupted"
    };

    let interrupted_at = if judgment_status == "fail" {
        Some(Utc::now())
    } else {
        None
    };

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            status = $1,
            safety_judgment = $2,
            interrupted_at = COALESCE($3, interrupted_at),
            updated_at = NOW()
        WHERE id = $4 AND tenant_id = $5
        RETURNING *
        "#,
    )
    .bind(next_status)
    .bind(&judgment_json)
    .bind(interrupted_at)
    .bind(session.id)
    .bind(tenant_id)
    .fetch_one(&mut **conn)
    .await
    .map_err(|e| {
        tracing::error!("perform_safety_judgment DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 判定失敗時: レコード作成 + Webhook発火
    if judgment_status == "fail" {
        let _ = create_tenko_record(conn, &session, tenant_id).await;

        let employee_name: Option<String> =
            sqlx::query_scalar("SELECT name FROM employees WHERE id = $1")
                .bind(session.employee_id)
                .fetch_optional(&mut **conn)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let payload = serde_json::json!({
            "event": "tenko_interrupted",
            "timestamp": Utc::now(),
            "tenant_id": tenant_id,
            "data": {
                "session_id": session.id,
                "employee_id": session.employee_id,
                "employee_name": employee_name.unwrap_or_default(),
                "failed_items": failed_items,
                "responsible_manager_name": session.responsible_manager_name,
                "tenko_type": session.tenko_type,
            }
        });

        let pool = pool.clone();
        tokio::spawn(async move {
            if let Err(e) =
                crate::webhook::fire_event(&pool, tenant_id, "tenko_interrupted", payload).await
            {
                tracing::error!("Webhook fire_event error: {e}");
            }
        });
    }

    Ok(session)
}

/// 日常点検送信 (業務前のみ)
async fn submit_daily_inspection(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitDailyInspection>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    // 全項目が "ok" or "ng" であることを検証
    let items = [
        &body.brakes,
        &body.tires,
        &body.lights,
        &body.steering,
        &body.wipers,
        &body.mirrors,
        &body.horn,
        &body.seatbelts,
    ];
    for item in &items {
        if *item != "ok" && *item != "ng" {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if session.tenko_type != "pre_operation" || session.status != "daily_inspection_pending" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let has_ng = items.iter().any(|i| *i == "ng");

    let inspection_json = serde_json::json!({
        "brakes": body.brakes,
        "tires": body.tires,
        "lights": body.lights,
        "steering": body.steering,
        "wipers": body.wipers,
        "mirrors": body.mirrors,
        "horn": body.horn,
        "seatbelts": body.seatbelts,
        "inspected_at": Utc::now(),
    });

    let next_status = if has_ng {
        "cancelled"
    } else {
        // テナントに携行品マスタがあれば carrying_items_pending、なければ identity_verified
        let carrying_items_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::BIGINT FROM alc_api.carrying_items WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if carrying_items_count.0 > 0 {
            "carrying_items_pending"
        } else {
            "identity_verified"
        }
    };
    let cancel_reason = if has_ng {
        Some("日常点検異常".to_string())
    } else {
        None
    };
    let completed_at = if has_ng { Some(Utc::now()) } else { None };

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            status = $1,
            daily_inspection = $2,
            cancel_reason = COALESCE($3, cancel_reason),
            completed_at = COALESCE($4, completed_at),
            updated_at = NOW()
        WHERE id = $5 AND tenant_id = $6
        RETURNING *
        "#,
    )
    .bind(next_status)
    .bind(&inspection_json)
    .bind(&cancel_reason)
    .bind(completed_at)
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("submit_daily_inspection DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if has_ng {
        // レコード作成 + Webhook
        let _ = create_tenko_record(&mut conn, &session, tenant_id).await;

        let employee_name: Option<String> =
            sqlx::query_scalar("SELECT name FROM employees WHERE id = $1")
                .bind(session.employee_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let payload = serde_json::json!({
            "event": "inspection_ng",
            "timestamp": Utc::now(),
            "tenant_id": tenant_id,
            "data": {
                "session_id": session.id,
                "employee_id": session.employee_id,
                "employee_name": employee_name.unwrap_or_default(),
                "inspection": inspection_json,
                "responsible_manager_name": session.responsible_manager_name,
            }
        });

        let pool = state.pool.clone();
        tokio::spawn(async move {
            if let Err(e) =
                crate::webhook::fire_event(&pool, tenant_id, "inspection_ng", payload).await
            {
                tracing::error!("Webhook fire_event error: {e}");
            }
        });
    }

    Ok(Json(session))
}

/// 携行品チェック結果送信
async fn submit_carrying_items(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitCarryingItemChecks>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if session.status != "carrying_items_pending" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 各チェック結果を tenko_carrying_item_checks に挿入
    let now = Utc::now();
    let mut check_results = Vec::new();
    for check in &body.checks {
        // item_name をマスタから取得
        let item_name: Option<String> = sqlx::query_scalar(
            "SELECT item_name FROM alc_api.carrying_items WHERE id = $1",
        )
        .bind(check.item_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let item_name = item_name.unwrap_or_default();

        sqlx::query(
            r#"INSERT INTO alc_api.tenko_carrying_item_checks
               (session_id, item_id, item_name, checked, checked_at)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (session_id, item_id) DO UPDATE SET
               checked = EXCLUDED.checked, checked_at = EXCLUDED.checked_at"#,
        )
        .bind(id)
        .bind(check.item_id)
        .bind(&item_name)
        .bind(check.checked)
        .bind(if check.checked { Some(now) } else { None })
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        check_results.push(serde_json::json!({
            "item_id": check.item_id,
            "item_name": item_name,
            "checked": check.checked,
        }));
    }

    let carrying_json = serde_json::json!({
        "items": check_results,
        "checked_at": now,
    });

    // ステータスを identity_verified に遷移
    let session = sqlx::query_as::<_, TenkoSession>(
        r#"UPDATE tenko_sessions SET
            status = 'identity_verified',
            carrying_items_checked = $1,
            updated_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        RETURNING *"#,
    )
    .bind(&carrying_json)
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("submit_carrying_items DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(session))
}

/// セッション中断 (管理者)
async fn interrupt_session(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<InterruptSession>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if matches!(session.status.as_str(), "completed" | "cancelled" | "interrupted") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            status = 'interrupted',
            interrupted_at = NOW(),
            cancel_reason = COALESCE($1, cancel_reason),
            updated_at = NOW()
        WHERE id = $2 AND tenant_id = $3
        RETURNING *
        "#,
    )
    .bind(&body.reason)
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("interrupt_session DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Webhook: tenko_interrupted
    let employee_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM employees WHERE id = $1")
            .bind(session.employee_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let payload = serde_json::json!({
        "event": "tenko_interrupted",
        "timestamp": Utc::now(),
        "tenant_id": tenant_id,
        "data": {
            "session_id": session.id,
            "employee_id": session.employee_id,
            "employee_name": employee_name.unwrap_or_default(),
            "reason": body.reason,
            "responsible_manager_name": session.responsible_manager_name,
            "tenko_type": session.tenko_type,
        }
    });

    let pool = state.pool.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::webhook::fire_event(&pool, tenant_id, "tenko_interrupted", payload).await
        {
            tracing::error!("Webhook fire_event error: {e}");
        }
    });

    Ok(Json(session))
}

/// セッション再開 (管理者)
async fn resume_session(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    auth_user: axum::Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<ResumeSession>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    if body.reason.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = sqlx::query_as::<_, TenkoSession>(
        "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if session.status != "interrupted" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 再開先の状態を判定
    let resume_to = if session.daily_inspection.is_none() {
        "daily_inspection_pending"
    } else if session.self_declaration.is_none() {
        "self_declaration_pending"
    } else {
        "daily_inspection_pending"
    };

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        UPDATE tenko_sessions SET
            status = $1,
            resumed_at = NOW(),
            resume_reason = $2,
            resumed_by_user_id = $3,
            updated_at = NOW()
        WHERE id = $4 AND tenant_id = $5
        RETURNING *
        "#,
    )
    .bind(resume_to)
    .bind(&body.reason)
    .bind(auth_user.user_id)
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("resume_session DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(session))
}
