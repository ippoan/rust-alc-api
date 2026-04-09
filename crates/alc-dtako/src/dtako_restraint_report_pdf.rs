use crate::dtako_restraint_report::build_report_with_name;
use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_pdf::generate_pdf;
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, Response, StatusCode},
    routing::get,
    Router,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/restraint-report/pdf", get(get_restraint_report_pdf))
        .route(
            "/restraint-report/pdf-stream",
            get(get_restraint_report_pdf_stream),
        )
}

#[derive(Debug, Deserialize)]
pub struct PdfFilter {
    pub year: i32,
    pub month: u32,
    pub driver_id: Option<uuid::Uuid>,
}

#[derive(Debug, Serialize)]
struct PdfProgressEvent {
    event: String,
    current: Option<usize>,
    total: Option<usize>,
    driver_name: Option<String>,
    step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

async fn get_restraint_report_pdf(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<PdfFilter>,
) -> Result<Response<Body>, (StatusCode, String)> {
    let tenant_id = tenant.0 .0;

    let repo = &state.dtako_restraint_report_pdf;
    let drivers = if let Some(did) = filter.driver_id {
        repo.get_driver(tenant_id, did).await
    } else {
        repo.list_drivers(tenant_id).await
    }
    .map_err(|e| {
        tracing::error!("fetch drivers error: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal error".to_string(),
        )
    })?;

    if drivers.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            "ドライバーが見つかりません".to_string(),
        ));
    }

    let mut reports = Vec::new();
    let mut driver_cds = Vec::new();
    for driver in &drivers {
        if driver.driver_name.is_empty() {
            continue;
        }
        let report = build_report_with_name(
            state.dtako_restraint_report.as_ref(),
            tenant_id,
            driver.id,
            &driver.driver_name,
            filter.year,
            filter.month,
        )
        .await?;
        driver_cds.push(driver.driver_cd.clone().unwrap_or_default());
        reports.push(report);
    }

    let pdf_bytes = generate_pdf(&reports, &driver_cds, filter.year, filter.month);

    let filename = format!("restraint_report_{}_{:02}.pdf", filter.year, filter.month);

    Ok(Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(pdf_bytes))
        .expect("valid response builder"))
}

async fn get_restraint_report_pdf_stream(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<PdfFilter>,
) -> Response<Body> {
    let tenant_id = tenant.0 .0;
    let year = filter.year;
    let month = filter.month;

    let (tx, rx) = mpsc::channel::<String>(32);

    tokio::spawn(async move {
        let send = |evt: PdfProgressEvent| {
            let tx = tx.clone();
            async move {
                let json = serde_json::to_string(&evt).unwrap_or_default();
                let _ = tx.send(format!("data: {json}\n\n")).await;
            }
        };

        let drivers = match state
            .dtako_restraint_report_pdf
            .list_drivers(tenant_id)
            .await
        {
            Ok(d) => d,
            Err(e) => {
                send(PdfProgressEvent {
                    event: "error".into(),
                    current: None,
                    total: None,
                    driver_name: None,
                    step: None,
                    data: None,
                    message: Some(format!("ドライバー取得エラー: {e}")),
                })
                .await;
                return;
            }
        };

        let drivers: Vec<_> = drivers
            .into_iter()
            .filter(|d| !d.driver_name.is_empty())
            .collect();
        let total = drivers.len();

        let mut reports = Vec::new();
        let mut driver_cds = Vec::new();
        for (i, driver) in drivers.iter().enumerate() {
            send(PdfProgressEvent {
                event: "progress".into(),
                current: Some(i + 1),
                total: Some(total),
                driver_name: Some(driver.driver_name.clone()),
                step: Some("fetch".into()),
                data: None,
                message: None,
            })
            .await;

            match build_report_with_name(
                state.dtako_restraint_report.as_ref(),
                tenant_id,
                driver.id,
                &driver.driver_name,
                year,
                month,
            )
            .await
            {
                Ok(report) => {
                    driver_cds.push(driver.driver_cd.clone().unwrap_or_default());
                    reports.push(report);
                }
                Err((_status, msg)) => {
                    tracing::warn!("skip driver {}: {msg}", driver.driver_name);
                }
            }
        }

        send(PdfProgressEvent {
            event: "progress".into(),
            current: Some(total),
            total: Some(total),
            driver_name: None,
            step: Some("render".into()),
            data: None,
            message: Some("PDF生成中...".into()),
        })
        .await;

        let pdf_bytes = generate_pdf(&reports, &driver_cds, year, month);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&pdf_bytes);
        send(PdfProgressEvent {
            event: "done".into(),
            current: Some(total),
            total: Some(total),
            driver_name: None,
            step: Some("save".into()),
            data: Some(b64),
            message: None,
        })
        .await;
    });

    let stream =
        tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok::<_, std::convert::Infallible>);

    Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap()
}
