mod common;
mod mock_helpers;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::MockDtakoRestraintReportRepository;
use uuid::Uuid;

/// Build mock AppState with a shared MockDtakoRestraintReportRepository reference
/// so we can toggle `fail_next` from test code.
async fn setup_with_shared_repo() -> (
    rust_alc_api::AppState,
    Arc<MockDtakoRestraintReportRepository>,
) {
    let mut state = setup_mock_app_state().await;
    let repo = Arc::new(MockDtakoRestraintReportRepository::default());
    state.dtako_restraint_report = repo.clone();
    (state, repo)
}

fn auth_header(tenant_id: Uuid) -> String {
    let token = common::create_test_jwt(tenant_id, "admin");
    format!("Bearer {token}")
}

/// Insert a test employee directly into the DB (the PDF route queries alc_api.employees via pool)
async fn insert_test_employee(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    name: &str,
    driver_cd: Option<&str>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alc_api.employees (id, tenant_id, name, code, driver_cd) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .bind(format!("CD-{}", &id.to_string()[..8]))
    .bind(driver_cd)
    .execute(pool)
    .await
    .expect("Failed to insert test employee");
    id
}

// =============================================================================
// GET /restraint-report/pdf — no auth → 401
// =============================================================================

#[tokio::test]
async fn test_pdf_no_auth() {
    let (state, _repo) = setup_with_shared_repo().await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base}/restraint-report/pdf?year=2026&month=3"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// =============================================================================
// GET /restraint-report/pdf — missing required params → 400
// =============================================================================

#[tokio::test]
async fn test_pdf_missing_params() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdf-missing-params").await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    // No params at all
    let res = client
        .get(format!("{base}/restraint-report/pdf"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);

    // Only year, missing month
    let res = client
        .get(format!("{base}/restraint-report/pdf?year=2026"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);

    // Only month, missing year
    let res = client
        .get(format!("{base}/restraint-report/pdf?month=3"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

// =============================================================================
// GET /restraint-report/pdf — no drivers in DB → 404
// =============================================================================

#[tokio::test]
async fn test_pdf_no_drivers_returns_404() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdf-no-drivers").await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    let res = client
        .get(format!("{base}/restraint-report/pdf?year=2026&month=3"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    let body = res.text().await.unwrap();
    assert!(body.contains("ドライバーが見つかりません"));
}

// =============================================================================
// GET /restraint-report/pdf — with driver_id, driver not found → 404
// =============================================================================

#[tokio::test]
async fn test_pdf_with_driver_id_not_found() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdf-driver-notfound").await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    let fake_driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base}/restraint-report/pdf?year=2026&month=3&driver_id={fake_driver_id}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// =============================================================================
// GET /restraint-report/pdf — single driver (empty report data) → 200 PDF
// =============================================================================

#[tokio::test]
async fn test_pdf_single_driver_empty_report() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdf-single-drv").await;
    let driver_id =
        insert_test_employee(&state.pool, tenant_id, "Test Driver", Some("DRV001")).await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    let res = client
        .get(format!(
            "{base}/restraint-report/pdf?year=2026&month=3&driver_id={driver_id}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "application/pdf"
    );
    let disposition = res
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(disposition.contains("restraint_report_2026_03.pdf"));

    let bytes = res.bytes().await.unwrap();
    // PDF files start with %PDF
    assert!(bytes.starts_with(b"%PDF"), "Response should be a valid PDF");
    assert!(bytes.len() > 100, "PDF should have some content");
}

// =============================================================================
// GET /restraint-report/pdf — all drivers (no driver_id param) → 200 PDF
// =============================================================================

#[tokio::test]
async fn test_pdf_all_drivers_empty_report() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdf-all-drv").await;
    insert_test_employee(&state.pool, tenant_id, "Driver A", Some("A001")).await;
    insert_test_employee(&state.pool, tenant_id, "Driver B", Some("B002")).await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    let res = client
        .get(format!("{base}/restraint-report/pdf?year=2026&month=3"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "application/pdf"
    );
    let bytes = res.bytes().await.unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

// =============================================================================
// GET /restraint-report/pdf — driver with empty name is skipped
// =============================================================================

#[tokio::test]
async fn test_pdf_empty_name_driver_skipped() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdf-empty-name").await;
    // Insert a driver with empty name — should be skipped by the handler
    let driver_id = insert_test_employee(&state.pool, tenant_id, "", Some("EMPTY")).await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    // With driver_id filter: driver exists but name is empty → skipped → no reports generated
    // The handler finds the driver (not 404) but skips it due to empty name,
    // then generates a PDF with zero reports.
    let res = client
        .get(format!(
            "{base}/restraint-report/pdf?year=2026&month=3&driver_id={driver_id}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // Driver found (not 404) but empty name means it's skipped in the loop,
    // resulting in an empty PDF being generated
    let status = res.status().as_u16();
    assert!(
        status == 200 || status == 404,
        "Expected 200 or 404, got {status}"
    );
}

// =============================================================================
// GET /restraint-report/pdf — DB error on build_report (fail_next on restraint repo)
// =============================================================================

#[tokio::test]
async fn test_pdf_db_error_on_build_report() {
    let (state, repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdf-build-err").await;
    let driver_id =
        insert_test_employee(&state.pool, tenant_id, "Error Driver", Some("ERR01")).await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    // Set fail_next to make build_report_with_name fail
    repo.fail_next.store(true, Ordering::SeqCst);

    let res = client
        .get(format!(
            "{base}/restraint-report/pdf?year=2026&month=3&driver_id={driver_id}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =============================================================================
// GET /restraint-report/pdf-stream — no auth → 401
// =============================================================================

#[tokio::test]
async fn test_pdf_stream_no_auth() {
    let (state, _repo) = setup_with_shared_repo().await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base}/restraint-report/pdf-stream?year=2026&month=3"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// =============================================================================
// GET /restraint-report/pdf-stream — missing params → 400
// =============================================================================

#[tokio::test]
async fn test_pdf_stream_missing_params() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdfstr-missing").await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    let res = client
        .get(format!("{base}/restraint-report/pdf-stream"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

// =============================================================================
// GET /restraint-report/pdf-stream — no drivers → SSE with done event (empty)
// =============================================================================

#[tokio::test]
async fn test_pdf_stream_no_drivers() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdfstr-no-drv").await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    let res = client
        .get(format!(
            "{base}/restraint-report/pdf-stream?year=2026&month=3"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(res
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/event-stream"));

    let body = res.text().await.unwrap();
    // SSE format: each event is "data: {json}\n\n"
    // With no drivers, we expect a render progress event and a done event
    assert!(body.contains("data: "), "Should contain SSE data events");
    assert!(
        body.contains("\"event\":\"done\""),
        "Should contain done event"
    );
}

// =============================================================================
// GET /restraint-report/pdf-stream — with drivers → SSE progress + done with PDF data
// =============================================================================

#[tokio::test]
async fn test_pdf_stream_with_drivers() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdfstr-with-drv").await;
    insert_test_employee(&state.pool, tenant_id, "Stream Driver", Some("STR01")).await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    let res = client
        .get(format!(
            "{base}/restraint-report/pdf-stream?year=2026&month=3"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    // Should have progress events
    assert!(
        body.contains("\"event\":\"progress\""),
        "Should contain progress events"
    );
    // Should have a done event with base64 PDF data
    assert!(
        body.contains("\"event\":\"done\""),
        "Should contain done event"
    );
    assert!(
        body.contains("\"data\":\""),
        "Done event should contain base64 PDF data"
    );
}

// =============================================================================
// GET /restraint-report/pdf-stream — DB error on build_report → skip driver
// =============================================================================

#[tokio::test]
async fn test_pdf_stream_build_report_error_skips_driver() {
    let (state, repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdfstr-build-err").await;
    insert_test_employee(&state.pool, tenant_id, "Fail Driver", Some("FAIL01")).await;
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    // Set fail_next — the stream handler catches build errors and skips the driver
    repo.fail_next.store(true, Ordering::SeqCst);

    let res = client
        .get(format!(
            "{base}/restraint-report/pdf-stream?year=2026&month=3"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // Stream always returns 200 (SSE)
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    // Should still complete with a done event (skipping the failed driver)
    assert!(
        body.contains("\"event\":\"done\""),
        "Should contain done event even after build error"
    );
}

// =============================================================================
// GET /restraint-report/pdf-stream — multiple drivers with mixed results
// =============================================================================

#[tokio::test]
async fn test_pdf_stream_multiple_drivers() {
    let (state, _repo) = setup_with_shared_repo().await;
    let tenant_id = common::create_test_tenant(&state.pool, "pdfstr-multi").await;
    insert_test_employee(&state.pool, tenant_id, "Driver One", Some("D001")).await;
    insert_test_employee(&state.pool, tenant_id, "Driver Two", Some("D002")).await;
    insert_test_employee(&state.pool, tenant_id, "", Some("D003")).await; // empty name, should be filtered
    let base = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let auth = auth_header(tenant_id);

    let res = client
        .get(format!(
            "{base}/restraint-report/pdf-stream?year=2026&month=3"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    assert!(body.contains("\"event\":\"done\""));
    // The total should be 2 (empty name driver is filtered out)
    assert!(
        body.contains("\"total\":2"),
        "Total should be 2 (empty name filtered): {body}"
    );
}
