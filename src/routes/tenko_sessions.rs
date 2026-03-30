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
    CancelTenkoSession, InterruptSession, MedicalDiffs, ResumeSession, SafetyJudgment,
    SelfDeclaration, StartTenkoSession, SubmitAlcoholResult, SubmitCarryingItemChecks,
    SubmitDailyInspection, SubmitMedicalData, SubmitOperationReport, SubmitSelfDeclaration,
    TenkoDashboard, TenkoRecord, TenkoSession, TenkoSessionFilter, TenkoSessionsResponse,
};
use crate::db::repository::TenkoSessionRepository;
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
    let repo = &*state.tenko_sessions;

    // スケジュールあり / なし (遠隔点呼) で分岐
    let (tenko_type, responsible_manager_name, schedule_id_for_insert) =
        if let Some(sid) = body.schedule_id {
            // スケジュール検証: 存在・未消費・乗務員一致
            let schedule = repo
                .get_schedule_unconsumed(tenant_id, sid)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;

            if schedule.employee_id != body.employee_id {
                return Err(StatusCode::BAD_REQUEST);
            }

            // スケジュール消費
            repo.consume_schedule(tenant_id, schedule.id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let tt = schedule.tenko_type.clone();
            let rmn = schedule.responsible_manager_name.clone();
            let sid = schedule.id;

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

    let session = repo
        .create_session(
            tenant_id,
            body.employee_id,
            schedule_id_for_insert,
            &tenko_type,
            initial_status,
            &body.identity_face_photo_url,
            &body.location,
            &responsible_manager_name,
        )
        .await
        .map_err(|e| {
            tracing::error!("start_session DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // スケジュールありの場合: consumed_by_session_id を更新
    if let Some(sid) = schedule_id_for_insert {
        repo.set_consumed_by_session(tenant_id, sid, session.id)
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
    let repo = &*state.tenko_sessions;

    let valid_results = ["pass", "fail", "normal", "over", "error"];
    if !valid_results.contains(&body.alcohol_result.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = repo
        .get(tenant_id, id)
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

    let session = repo
        .update_alcohol(
            tenant_id,
            id,
            next_status,
            body.measurement_id,
            &body.alcohol_result,
            body.alcohol_value,
            &body.alcohol_face_photo_url,
            &cancel_reason,
            completed_at,
        )
        .await
        .map_err(|e| {
            tracing::error!("submit_alcohol DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if is_fail {
        // 不変レコード作成
        let _ = create_tenko_record(repo, &session, tenant_id).await;

        // Webhook: alcohol_detected
        let employee_name = repo
            .get_employee_name(tenant_id, session.employee_id)
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

        let pool = match state.pool.clone() {
            Some(p) => p,
            None => return Ok(Json(session)),
        };
        #[rustfmt::skip]
        tokio::spawn(async move { let _ = crate::webhook::fire_event(&pool, tenant_id, "alcohol_detected", payload).await; });
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
    let repo = &*state.tenko_sessions;

    let session = repo
        .get(tenant_id, id)
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

    let session = repo
        .update_medical(
            tenant_id,
            id,
            body.temperature,
            body.systolic,
            body.diastolic,
            body.pulse,
            body.medical_measured_at,
            body.medical_manual_input,
        )
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
    let repo = &*state.tenko_sessions;

    let session = repo
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if session.status != "instruction_pending" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = repo.confirm_instruction(tenant_id, id).await.map_err(|e| {
        tracing::error!("confirm_instruction DB error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 不変レコード作成
    let _ = create_tenko_record(repo, &session, tenant_id).await;

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
    let repo = &*state.tenko_sessions;

    // 両項目とも必須（テキストまたは "報告なし"）
    if body.vehicle_road_status.trim().is_empty() || body.driver_alternation.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = repo
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if session.tenko_type != "post_operation" || session.status != "report_pending" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 指示事項があるか確認
    let instruction = repo
        .get_schedule_instruction(tenant_id, session.schedule_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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

    let session = repo
        .update_report(
            tenant_id,
            id,
            next_status,
            &body.vehicle_road_status,
            &body.driver_alternation,
            &body.vehicle_road_audio_url,
            &body.driver_alternation_audio_url,
            completed_at,
        )
        .await
        .map_err(|e| {
            tracing::error!("submit_report DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 指示事項なしで完了した場合、レコード作成
    if next_status == "completed" {
        let _ = create_tenko_record(repo, &session, tenant_id).await;
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

        let pool = match state.pool.clone() {
            Some(p) => p,
            None => return Ok(Json(session)),
        };
        tokio::spawn(async move {
            let _ = crate::webhook::fire_event(&pool, tenant_id, "report_submitted", payload).await;
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
    let repo = &*state.tenko_sessions;

    let session = repo
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // 既に終了状態なら不可
    if matches!(session.status.as_str(), "completed" | "cancelled") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = repo
        .cancel(tenant_id, id, &body.reason)
        .await
        .map_err(|e| {
            tracing::error!("cancel_session DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let _ = create_tenko_record(repo, &session, tenant_id).await;

    Ok(Json(session))
}

/// セッション取得
async fn get_session(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenkoSession>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let session = state
        .tenko_sessions
        .get(tenant_id, id)
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

    let result = state
        .tenko_sessions
        .list(tenant_id, &filter, page, per_page)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TenkoSessionsResponse {
        sessions: result.sessions,
        total: result.total,
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

    let overdue_minutes: i64 = std::env::var("TENKO_OVERDUE_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let dashboard = state
        .tenko_sessions
        .dashboard(tenant_id, overdue_minutes)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(dashboard))
}

/// 不変レコード作成
async fn create_tenko_record(
    repo: &dyn TenkoSessionRepository,
    session: &TenkoSession,
    tenant_id: Uuid,
) -> Result<TenkoRecord, StatusCode> {
    let employee_name = repo
        .get_employee_name(tenant_id, session.employee_id)
        .await
        .map_err(|e| {
            tracing::error!("create_tenko_record: employee lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let instruction = repo
        .get_schedule_instruction(tenant_id, session.schedule_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let record_data =
        serde_json::to_value(session).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let canonical =
        serde_json::to_string(&record_data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let hash = format!("{:x}", Sha256::digest(canonical.as_bytes()));

    let record = repo
        .create_tenko_record(
            tenant_id,
            session,
            &employee_name,
            &instruction,
            &record_data,
            &hash,
        )
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
    let repo = &*state.tenko_sessions;

    let session = repo
        .get(tenant_id, id)
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

    let session = repo
        .update_self_declaration(tenant_id, id, &declaration_json)
        .await
        .map_err(|e| {
            tracing::error!("submit_self_declaration DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 安全判定を自動実行 (pool がない場合はスキップ)
    let session = if let Some(pool) = state.pool.as_ref() {
        perform_safety_judgment(repo, &session, tenant_id, pool).await?
    } else {
        session
    };

    Ok(Json(session))
}

/// 安全運転可否の自動判定 (内部ヘルパー)
fn check_self_declaration(
    self_declaration: &Option<serde_json::Value>,
    failed_items: &mut Vec<String>,
) {
    let decl = self_declaration
        .as_ref()
        .and_then(|j| serde_json::from_value::<SelfDeclaration>(j.clone()).ok());
    let Some(decl) = decl else { return };
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

async fn perform_safety_judgment(
    repo: &dyn TenkoSessionRepository,
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
    let baseline = repo
        .get_health_baseline(tenant_id, session.employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    fn check_vital_i32(
        val: Option<i32>,
        base: i32,
        tol: i32,
        diff_out: &mut Option<i32>,
        label: &str,
        failed: &mut Vec<String>,
    ) {
        let Some(v) = val else { return };
        let d = v - base;
        *diff_out = Some(d);
        if d.abs() > tol {
            failed.push(label.to_string());
        }
    }
    fn check_vital_f64(
        val: Option<f64>,
        base: f64,
        tol: f64,
        diff_out: &mut Option<f64>,
        label: &str,
        failed: &mut Vec<String>,
    ) {
        let Some(v) = val else { return };
        let d = v - base;
        *diff_out = Some(d);
        if d.abs() > tol {
            failed.push(label.to_string());
        }
    }
    if let Some(bl) = &baseline {
        check_vital_i32(
            session.systolic,
            bl.baseline_systolic,
            bl.systolic_tolerance,
            &mut medical_diffs.systolic_diff,
            "systolic",
            &mut failed_items,
        );
        check_vital_i32(
            session.diastolic,
            bl.baseline_diastolic,
            bl.diastolic_tolerance,
            &mut medical_diffs.diastolic_diff,
            "diastolic",
            &mut failed_items,
        );
        check_vital_f64(
            session.temperature,
            bl.baseline_temperature,
            bl.temperature_tolerance,
            &mut medical_diffs.temperature_diff,
            "temperature",
            &mut failed_items,
        );
    } else {
        let msg = format!(
            "No health baseline for employee {}, defaulting to pass",
            session.employee_id
        );
        tracing::warn!("{msg}");
    }

    // 自己申告チェック
    check_self_declaration(&session.self_declaration, &mut failed_items);

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

    let session = repo
        .update_safety_judgment(
            tenant_id,
            session.id,
            next_status,
            &judgment_json,
            interrupted_at,
        )
        .await
        .map_err(|e| {
            tracing::error!("perform_safety_judgment DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 判定失敗時: レコード作成 + Webhook発火
    if judgment_status == "fail" {
        let _ = create_tenko_record(repo, &session, tenant_id).await;

        let employee_name = repo
            .get_employee_name(tenant_id, session.employee_id)
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
            let _ =
                crate::webhook::fire_event(&pool, tenant_id, "tenko_interrupted", payload).await;
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
    let repo = &*state.tenko_sessions;

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

    let session = repo
        .get(tenant_id, id)
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
        let carrying_items_count = repo
            .count_carrying_items(tenant_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if carrying_items_count > 0 {
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

    let session = repo
        .update_daily_inspection(
            tenant_id,
            id,
            next_status,
            &inspection_json,
            &cancel_reason,
            completed_at,
        )
        .await
        .map_err(|e| {
            tracing::error!("submit_daily_inspection DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if has_ng {
        // レコード作成 + Webhook
        let _ = create_tenko_record(repo, &session, tenant_id).await;

        let employee_name = repo
            .get_employee_name(tenant_id, session.employee_id)
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

        let pool = match state.pool.clone() {
            Some(p) => p,
            None => return Ok(Json(session)),
        };
        #[rustfmt::skip]
        tokio::spawn(async move { let _ = crate::webhook::fire_event(&pool, tenant_id, "inspection_ng", payload).await; });
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
    let repo = &*state.tenko_sessions;

    let session = repo
        .get(tenant_id, id)
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
        let item_name = repo
            .get_carrying_item_name(tenant_id, check.item_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .unwrap_or_default();

        repo.upsert_carrying_item_check(
            tenant_id,
            id,
            check.item_id,
            &item_name,
            check.checked,
            if check.checked { Some(now) } else { None },
        )
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
    let session = repo
        .update_carrying_items(tenant_id, id, &carrying_json)
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
    let repo = &*state.tenko_sessions;

    let session = repo
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if matches!(
        session.status.as_str(),
        "completed" | "cancelled" | "interrupted"
    ) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = repo
        .interrupt(tenant_id, id, &body.reason)
        .await
        .map_err(|e| {
            tracing::error!("interrupt_session DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Webhook: tenko_interrupted
    let employee_name = repo
        .get_employee_name(tenant_id, session.employee_id)
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

    let pool = match state.pool.clone() {
        Some(p) => p,
        None => return Ok(Json(session)),
    };
    tokio::spawn(async move {
        let _ = crate::webhook::fire_event(&pool, tenant_id, "tenko_interrupted", payload).await;
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
    let repo = &*state.tenko_sessions;

    if body.reason.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = repo
        .get(tenant_id, id)
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

    let session = repo
        .resume(
            tenant_id,
            id,
            resume_to,
            &body.reason,
            Some(auth_user.user_id),
        )
        .await
        .map_err(|e| {
            tracing::error!("resume_session DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(session))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_self_declaration_none() {
        let mut failed = Vec::new();
        check_self_declaration(&None, &mut failed);
        assert!(failed.is_empty());
    }

    #[test]
    fn test_check_self_declaration_all_false() {
        let decl =
            serde_json::json!({"illness": false, "fatigue": false, "sleep_deprivation": false});
        let mut failed = Vec::new();
        check_self_declaration(&Some(decl), &mut failed);
        assert!(failed.is_empty());
    }
}
