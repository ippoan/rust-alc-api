//! 車両整備記録 (maintenance) の土台 — 車両マスタ API の実 DB 統合テスト
//! (Refs ippoan/rust-alc-api#651)。
//!
//! carins 紐づけ (`PUT/DELETE .../carins`, `GET .../carins-candidates`) は
//! `alc-core::repo::car_inspections::lookup_expiry` を経由する生 SQL で mock を
//! 挟めないため、`car_inspections_expiry_test.rs` と同じ作法で実 DB に検証する。
//! partial unique (`tenant_id, car_id`) の 409 と RLS のテナント分離もここで固定する。

#[macro_use]
mod common;

use serde_json::Value;
use uuid::Uuid;

/// car_inspection に合成の行を 1 件入れる (`car_inspections_expiry_test.rs` の
/// `insert_car_inspection` と同じ作法。取り込みと同じ UPSERT を通す)。
/// `car_id` は 14 英数字、`cert_no` は 12〜13 桁の数字で渡すこと
/// (`normalize_carins_numbers` の形チェックに合わせるため)。
async fn insert_car_inspection(
    state: &rust_alc_api::AppState,
    tenant: Uuid,
    cert_no: &str,
    car_id: &str,
    entry_no_car_no: &str,
) {
    let cert_info = serde_json::json!({
        "ElectCertMgNo": cert_no,
        "CarId": car_id,
        "EntryNoCarNo": entry_no_car_no,
        "TwodimensionCodeInfoValidPeriodExpirdate": "301231",
    });
    state
        .car_inspections
        .upsert_from_json(tenant, &cert_info, "test")
        .await
        .expect("car_inspection の合成行を入れられない");
}

/// 14 英数字の合成 CarId を作る (テストごとに一意)。
fn test_car_id() -> String {
    format!("C{}", &Uuid::new_v4().simple().to_string()[..13]).to_uppercase()
}

/// 12 桁の合成 ElectCertMgNo を作る (テストごとに一意)。
fn test_cert_no() -> String {
    let n: u64 = Uuid::new_v4().as_u128() as u64 % 1_000_000_000_000;
    format!("{n:012}")
}

#[tokio::test]
async fn test_create_vehicle_with_registration_number_only() {
    test_group!("車両マスタ: 作成");
    test_case!(
        "registration_number だけで作成でき、car_id は NULL のまま",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Vehicles Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "registration_number": "品川330あ12-34" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let vehicle: Value = res.json().await.unwrap();
            assert_eq!(vehicle["registration_number"], "品川330あ12-34");
            assert!(vehicle["car_id"].is_null());
            assert!(vehicle["carins_linked_at"].is_null());
        }
    );
}

#[tokio::test]
async fn test_maintenance_vehicles_crud() {
    test_group!("車両マスタ: CRUD");
    test_case!(
        "作成→一覧→取得→更新→削除→削除後404",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance CRUD Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "registration_number": "足立300さ11-11" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let created: Value = res.json().await.unwrap();
            let id = created["id"].as_str().unwrap();

            // list: q で部分一致
            let res = client
                .get(format!("{base_url}/api/maintenance/vehicles?q=足立300"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let list: Value = res.json().await.unwrap();
            assert_eq!(list["total"], 1);
            assert_eq!(list["items"][0]["id"], id);

            // get
            let res = client
                .get(format!("{base_url}/api/maintenance/vehicles/{id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            // update
            let res = client
                .put(format!("{base_url}/api/maintenance/vehicles/{id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "display_name": "配送1号車" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let updated: Value = res.json().await.unwrap();
            assert_eq!(updated["display_name"], "配送1号車");
            // registration_number は未指定なので変わらない
            assert_eq!(updated["registration_number"], "足立300さ11-11");

            // delete (soft)
            let res = client
                .delete(format!("{base_url}/api/maintenance/vehicles/{id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            // 削除後は 404
            let res = client
                .get(format!("{base_url}/api/maintenance/vehicles/{id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

#[tokio::test]
async fn test_link_carins_none_returns_400() {
    test_group!("車両マスタ: carins 紐づけ");
    test_case!("一致する car_inspection が無ければ 400", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id =
            common::create_test_tenant(state.pool(), "Maintenance Carins None Tenant").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();
        let auth = format!("Bearer {jwt}");

        let res = client
            .post(format!("{base_url}/api/maintenance/vehicles"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "registration_number": "練馬500む99-99" }))
            .send()
            .await
            .unwrap();
        let vehicle: Value = res.json().await.unwrap();
        let id = vehicle["id"].as_str().unwrap();

        // 存在しない cert_no → lookup_expiry が matched_by="none" を返す → 400
        let res = client
            .put(format!("{base_url}/api/maintenance/vehicles/{id}/carins"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "cert_no": test_cert_no() }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

#[tokio::test]
async fn test_link_carins_conflict_when_car_id_already_taken() {
    test_group!("車両マスタ: carins 紐づけ");
    test_case!(
        "同じ car_id を 2 台目に紐づけようとすると 409 (partial unique)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Carins Conflict Tenant")
                    .await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let car_id = test_car_id();
            insert_car_inspection(&state, tenant_id, &test_cert_no(), &car_id, "TESTCARNO001")
                .await;

            // 1 台目: 紐づけ成功
            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "registration_number": "1号車" }))
                .send()
                .await
                .unwrap();
            let vehicle1: Value = res.json().await.unwrap();
            let id1 = vehicle1["id"].as_str().unwrap();

            let res = client
                .put(format!("{base_url}/api/maintenance/vehicles/{id1}/carins"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "car_id": car_id }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let linked: Value = res.json().await.unwrap();
            assert_eq!(linked["car_id"], car_id);
            assert!(!linked["carins_linked_at"].is_null());

            // 2 台目: 同じ car_id を紐づけようとすると 409
            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "registration_number": "2号車" }))
                .send()
                .await
                .unwrap();
            let vehicle2: Value = res.json().await.unwrap();
            let id2 = vehicle2["id"].as_str().unwrap();

            let res = client
                .put(format!("{base_url}/api/maintenance/vehicles/{id2}/carins"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "car_id": car_id }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 409);

            // 解除 → 再度 get すると car_id が NULL
            let res = client
                .delete(format!("{base_url}/api/maintenance/vehicles/{id1}/carins"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);
            let res = client
                .get(format!("{base_url}/api/maintenance/vehicles/{id1}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let after_unlink: Value = res.json().await.unwrap();
            assert!(after_unlink["car_id"].is_null());
            assert!(after_unlink["carins_linked_at"].is_null());
        }
    );
}

#[tokio::test]
async fn test_link_carins_by_cert_no_resolves_car_id() {
    test_group!("車両マスタ: carins 紐づけ");
    test_case!(
        "cert_no だけの入力でも一致した行の car_id が保存される",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Carins CertNo Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let cert_no = test_cert_no();
            let car_id = test_car_id();
            insert_car_inspection(&state, tenant_id, &cert_no, &car_id, "TESTCARNO002").await;

            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "registration_number": "3号車" }))
                .send()
                .await
                .unwrap();
            let vehicle: Value = res.json().await.unwrap();
            let id = vehicle["id"].as_str().unwrap();

            let res = client
                .put(format!("{base_url}/api/maintenance/vehicles/{id}/carins"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "cert_no": cert_no }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let linked: Value = res.json().await.unwrap();
            assert_eq!(linked["car_id"], car_id);
        }
    );
}

#[tokio::test]
async fn test_carins_candidates_matches_normalized_registration_number() {
    test_group!("車両マスタ: carins 候補");
    test_case!(
        "全角の登録番号を正規化して car_inspection の候補を返す",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Carins Candidates Tenant")
                    .await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let car_id = test_car_id();
            // EntryNoCarNo は半角で保存されている想定
            insert_car_inspection(&state, tenant_id, &test_cert_no(), &car_id, "12-34").await;

            // 車両側は全角のダッシュ・数字で登録 (正規化しないと一致しない)
            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "registration_number": "品川330あ１２－３４" }))
                .send()
                .await
                .unwrap();
            let vehicle: Value = res.json().await.unwrap();
            let id = vehicle["id"].as_str().unwrap();

            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/vehicles/{id}/carins-candidates"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let candidates: Value = res.json().await.unwrap();
            let candidates = candidates.as_array().unwrap();
            assert!(
                candidates.iter().any(|c| c["car_id"] == car_id),
                "候補に一致する CarId が含まれること: {candidates:?}"
            );
        }
    );
}

#[tokio::test]
async fn test_maintenance_vehicles_tenant_isolation() {
    test_group!("車両マスタ: RLS");
    test_case!(
        "別テナントの車両は一覧にも個別取得にも出ない",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a =
                common::create_test_tenant(state.pool(), "Maintenance RLS Tenant A").await;
            let tenant_b =
                common::create_test_tenant(state.pool(), "Maintenance RLS Tenant B").await;
            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let jwt_b = common::create_test_jwt(tenant_b, "admin");
            let client = reqwest::Client::new();

            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles"))
                .header("Authorization", format!("Bearer {jwt_a}"))
                .json(&serde_json::json!({ "registration_number": "テナントA専用車両" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let vehicle_a: Value = res.json().await.unwrap();
            let id_a = vehicle_a["id"].as_str().unwrap();

            // tenant B の一覧には出ない
            let res = client
                .get(format!("{base_url}/api/maintenance/vehicles"))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let list: Value = res.json().await.unwrap();
            let items = list["items"].as_array().unwrap();
            assert!(!items.iter().any(|v| v["id"] == id_a));

            // tenant B から個別取得すると 404 (RLS で行が見えない)
            let res = client
                .get(format!("{base_url}/api/maintenance/vehicles/{id_a}"))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

// ===========================================================================
// 整備カテゴリ (maintenance_categories) — generic master の 5 番目の利用者
// (Refs ippoan/rust-alc-api#651)
// ===========================================================================

#[tokio::test]
async fn test_maintenance_categories_auto_seed_when_empty() {
    test_group!("整備カテゴリ: auto-seed");
    test_case!(
        "空のテナントで一覧を取ると既定 5 件が seed されて返る",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Categories Seed Tenant")
                    .await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let res = client
                .get(format!("{base_url}/api/maintenance/categories"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let categories: Value = res.json().await.unwrap();
            let categories = categories.as_array().unwrap();
            assert_eq!(categories.len(), 5);
            let names: Vec<&str> = categories
                .iter()
                .map(|c| c["name"].as_str().unwrap())
                .collect();
            assert_eq!(
                names,
                vec!["定期点検", "修理", "部品交換", "タイヤ交換", "オイル交換"]
            );
        }
    );
}

#[tokio::test]
async fn test_maintenance_categories_no_reseed_when_not_empty() {
    test_group!("整備カテゴリ: auto-seed");
    test_case!(
        "既にカテゴリがあれば既定 seed は走らず、追加分だけが返る",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Categories No Reseed Tenant")
                    .await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let res = client
                .post(format!("{base_url}/api/maintenance/categories"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "name": "独自カテゴリ" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);

            let res = client
                .get(format!("{base_url}/api/maintenance/categories"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let categories: Value = res.json().await.unwrap();
            let categories = categories.as_array().unwrap();
            assert_eq!(categories.len(), 1);
            assert_eq!(categories[0]["name"], "独自カテゴリ");
        }
    );
}

#[tokio::test]
async fn test_maintenance_categories_duplicate_name_returns_409() {
    test_group!("整備カテゴリ: 409");
    test_case!("同名で作成すると 409", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id =
            common::create_test_tenant(state.pool(), "Maintenance Categories Conflict Tenant")
                .await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();
        let auth = format!("Bearer {jwt}");

        let res = client
            .post(format!("{base_url}/api/maintenance/categories"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "name": "板金塗装" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        let res = client
            .post(format!("{base_url}/api/maintenance/categories"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "name": "板金塗装" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 409);
    });
}

#[tokio::test]
async fn test_maintenance_categories_tenant_isolation() {
    test_group!("整備カテゴリ: RLS");
    test_case!(
        "別テナントのカテゴリは一覧にも sort_order 更新にも出ない",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a =
                common::create_test_tenant(state.pool(), "Maintenance Categories RLS Tenant A")
                    .await;
            let tenant_b =
                common::create_test_tenant(state.pool(), "Maintenance Categories RLS Tenant B")
                    .await;
            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let jwt_b = common::create_test_jwt(tenant_b, "admin");
            let client = reqwest::Client::new();

            let res = client
                .post(format!("{base_url}/api/maintenance/categories"))
                .header("Authorization", format!("Bearer {jwt_a}"))
                .json(&serde_json::json!({ "name": "テナントA専用カテゴリ" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let category_a: Value = res.json().await.unwrap();
            let id_a = category_a["id"].as_str().unwrap();

            // tenant B の一覧には出ない (tenant B は auto-seed の既定 5 件のみ)
            let res = client
                .get(format!("{base_url}/api/maintenance/categories"))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let list: Value = res.json().await.unwrap();
            let items = list.as_array().unwrap();
            assert!(!items.iter().any(|c| c["id"] == id_a));

            // tenant B から tenant A のカテゴリを sort_order 更新しようとすると 404
            let res = client
                .put(format!("{base_url}/api/maintenance/categories/{id_a}"))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .json(&serde_json::json!({ "sort_order": 9 }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}
