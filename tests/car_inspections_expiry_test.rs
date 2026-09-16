//! 車検期限の照合 (`alc-core` の `repo/car_inspections.rs::lookup_expiry`) が、
//! 2 次元コード列が空文字列の行でも和暦 4 列から期限を組み立てられることを
//! 実 DB で固定する (Refs ippoan/alc-app-s3#110)。
//!
//! **実 DB でしか検証できない** — 変換は正規表現・LATERAL JOIN・`make_date` を
//! 含む生 SQL で、repository を差し替える mock テストでは 1 行も通らない。
//!
//! 回し方は他の DB integration テストと同じ:
//!   `make db-up && source .test-config && cargo test --test car_inspections_expiry_test`
//! CI は ci.yml の `bazel-test-db` shard (postgres service + TEST_DATABASE_URL)。

#[macro_use]
mod common;

use alc_core::repo::car_inspections::lookup_expiry;
use chrono::NaiveDate;
use uuid::Uuid;

/// car_inspection に合成の行を 1 件入れる (取り込みと同じ UPSERT を通す。
/// 実在の管理番号・車両 ID・登録番号は使わない)
#[allow(clippy::too_many_arguments)]
async fn insert_car_inspection(
    state: &rust_alc_api::AppState,
    tenant: Uuid,
    cert_no: &str,
    twod_expiry: &str,
    era: &str,
    wareki_y: &str,
    wareki_m: &str,
    wareki_d: &str,
) {
    let cert_info = serde_json::json!({
        "ElectCertMgNo": cert_no,
        "CarId": format!("CARID-{cert_no}"),
        "EntryNoCarNo": format!("TEST-CAR-NO-{cert_no}"),
        "TwodimensionCodeInfoValidPeriodExpirdate": twod_expiry,
        "ValidPeriodExpirdateE": era,
        "ValidPeriodExpirdateY": wareki_y,
        "ValidPeriodExpirdateM": wareki_m,
        "ValidPeriodExpirdateD": wareki_d,
    });
    state
        .car_inspections
        .upsert_from_json(tenant, &cert_info, "test")
        .await
        .expect("car_inspection の合成行を入れられない");
}

#[tokio::test]
async fn lookup_expiry_uses_2d_column_when_valid() {
    test_group!("車検期限照合 (和暦フォールバック)");
    test_case!(
        "2D の列が正しい行 → その日付が返る (和暦列が空でも既存の振る舞いは変わらない)",
        {
            let state = common::setup_app_state().await;
            let tenant = common::create_test_tenant(state.pool(), "Carins Wareki 2D").await;
            let cert_no = format!("t2d-{}", Uuid::new_v4().simple());
            insert_car_inspection(&state, tenant, &cert_no, "271231", "", "", "", "").await;

            let mut conn = state.pool().acquire().await.expect("接続取得に失敗");
            let result = lookup_expiry(&mut conn, Some(&cert_no), None)
                .await
                .expect("lookup_expiry がエラーを返した");

            assert_eq!(result.expires_on, NaiveDate::from_ymd_opt(2027, 12, 31));
            assert_eq!(result.matched_by, "cert_no");
        }
    );
}

#[tokio::test]
async fn lookup_expiry_falls_back_to_wareki_when_2d_empty() {
    test_group!("車検期限照合 (和暦フォールバック)");
    test_case!(
        "2D の列が空文字列で、和暦4列が入っている行 → 和暦から組み立てた日付が返る",
        {
            let state = common::setup_app_state().await;
            let tenant = common::create_test_tenant(state.pool(), "Carins Wareki Fallback").await;
            let cert_no = format!("wareki-{}", Uuid::new_v4().simple());
            insert_car_inspection(&state, tenant, &cert_no, "", "令和", "8", "2", "13").await;

            let mut conn = state.pool().acquire().await.expect("接続取得に失敗");
            let result = lookup_expiry(&mut conn, Some(&cert_no), None)
                .await
                .expect("lookup_expiry がエラーを返した");

            assert_eq!(result.expires_on, NaiveDate::from_ymd_opt(2026, 2, 13));
            assert_eq!(result.matched_by, "cert_no");
        }
    );
}

#[tokio::test]
async fn lookup_expiry_returns_none_without_error_when_both_broken() {
    test_group!("車検期限照合 (和暦フォールバック)");
    test_case!(
        "2D も和暦も壊れている行 → expires_on は None、エラーにはならず matched_by は当たったまま",
        {
            let state = common::setup_app_state().await;
            let tenant = common::create_test_tenant(state.pool(), "Carins Wareki Broken").await;
            let cert_no = format!("broken-{}", Uuid::new_v4().simple());
            // 2D 列は正規表現に外れる形、和暦は月が 13 (範囲外)
            insert_car_inspection(
                &state,
                tenant,
                &cert_no,
                "not-a-date",
                "令和",
                "8",
                "13",
                "01",
            )
            .await;

            let mut conn = state.pool().acquire().await.expect("接続取得に失敗");
            let result = lookup_expiry(&mut conn, Some(&cert_no), None)
                .await
                .expect("lookup_expiry がエラーを返した (壊れた値で例外になってはいけない)");

            assert_eq!(result.expires_on, None);
            assert_eq!(result.matched_by, "cert_no");
        }
    );
}

#[tokio::test]
async fn lookup_expiry_returns_none_for_unknown_era() {
    test_group!("車検期限照合 (和暦フォールバック)");
    test_case!(
        "和暦の元号が未知の値 → None (エラーにしない)",
        {
            let state = common::setup_app_state().await;
            let tenant =
                common::create_test_tenant(state.pool(), "Carins Wareki Unknown Era").await;
            let cert_no = format!("unknown-era-{}", Uuid::new_v4().simple());
            insert_car_inspection(&state, tenant, &cert_no, "", "大正", "1", "1", "1").await;

            let mut conn = state.pool().acquire().await.expect("接続取得に失敗");
            let result = lookup_expiry(&mut conn, Some(&cert_no), None)
                .await
                .expect("lookup_expiry がエラーを返した");

            assert_eq!(result.expires_on, None);
            assert_eq!(result.matched_by, "cert_no");
        }
    );
}
