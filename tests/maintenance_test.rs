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

// ---------------------------------------------------------------------------
// 整備記録 (maintenance_records) の CRUD + 一覧フィルタ (Refs #c651-5)
//
// `categories.rs` (#c651-4) が実装する generic master 経由の CRUD には依存せず、
// `maintenance_categories` へ直接 SQL で 1 行入れる (この記録テストは categories
// の実装詳細に結合させないため — 表は migrations/147 で用意済み)。
// ---------------------------------------------------------------------------

/// テスト用に `maintenance_categories` へ直接 1 行入れる (`categories.rs` (#c651-4)
/// の実装詳細に依存せず検証するため)。RLS があるため `TenantConn` 経由で
/// `app.current_tenant_id` をセットしてから insert する。
async fn insert_maintenance_category(pool: &sqlx::PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    let mut tc = alc_core::tenant::TenantConn::acquire(pool, &tenant_id.to_string())
        .await
        .expect("TenantConn::acquire に失敗");
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO maintenance_categories (tenant_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(name)
    .fetch_one(&mut *tc.conn)
    .await
    .expect("maintenance_categories への insert に失敗");
    row.0
}

/// `POST /api/maintenance/vehicles` 経由で車両を1台作り、id (文字列) を返す。
async fn create_test_vehicle(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    registration_number: &str,
) -> String {
    let res = client
        .post(format!("{base_url}/api/maintenance/vehicles"))
        .header("Authorization", auth)
        .json(&serde_json::json!({ "registration_number": registration_number }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let vehicle: Value = res.json().await.unwrap();
    vehicle["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_maintenance_records_crud() {
    test_group!("整備記録: CRUD");
    test_case!(
        "作成→取得→更新→ソフト削除→削除後は一覧にも個別取得にも出ない",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Records CRUD Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let vehicle_id = create_test_vehicle(&client, &base_url, &auth, "品川300か1-11").await;
            let category_id =
                insert_maintenance_category(state.pool(), tenant_id, "定期点検").await;

            // 作成
            let res = client
                .post(format!("{base_url}/api/maintenance/records"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "vehicle_id": vehicle_id,
                    "category_id": category_id,
                    "performed_on": "2026-01-10",
                    "odometer_km": 12345,
                    "vendor": "オートテスト整備工場",
                    "description": "12ヶ月点検",
                    "cost": 33000.0,
                    "next_due_on": "2027-01-10",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let created: Value = res.json().await.unwrap();
            let id = created["id"].as_str().unwrap().to_string();
            assert_eq!(created["vehicle_id"], vehicle_id);
            assert_eq!(created["category_id"], category_id.to_string());
            assert_eq!(created["odometer_km"], 12345);
            assert_eq!(created["vendor"], "オートテスト整備工場");

            // 取得
            let res = client
                .get(format!("{base_url}/api/maintenance/records/{id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let fetched: Value = res.json().await.unwrap();
            assert_eq!(fetched["id"], id);

            // 更新 (odometer_km と vendor のみ)
            let res = client
                .put(format!("{base_url}/api/maintenance/records/{id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "odometer_km": 20000,
                    "vendor": "更新後の整備工場",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let updated: Value = res.json().await.unwrap();
            assert_eq!(updated["odometer_km"], 20000);
            assert_eq!(updated["vendor"], "更新後の整備工場");
            // 未指定のフィールドは変わらない
            assert_eq!(updated["description"], "12ヶ月点検");

            // ソフト削除
            let res = client
                .delete(format!("{base_url}/api/maintenance/records/{id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            // 削除後は個別取得で 404
            let res = client
                .get(format!("{base_url}/api/maintenance/records/{id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);

            // 削除後は一覧にも出ない
            let res = client
                .get(format!("{base_url}/api/maintenance/records"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let list: Value = res.json().await.unwrap();
            let records = list["records"].as_array().unwrap();
            assert!(!records.iter().any(|r| r["id"] == id));
        }
    );
}

#[tokio::test]
async fn test_create_record_with_other_tenant_vehicle_returns_400() {
    test_group!("整備記録: FK テナント検証");
    test_case!(
        "他テナントの vehicle_id を指定すると 500 ではなく 400",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a =
                common::create_test_tenant(state.pool(), "Maintenance Records FK Tenant A").await;
            let tenant_b =
                common::create_test_tenant(state.pool(), "Maintenance Records FK Tenant B").await;
            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let jwt_b = common::create_test_jwt(tenant_b, "admin");
            let client = reqwest::Client::new();

            // vehicle はテナント A のもの
            let vehicle_id =
                create_test_vehicle(&client, &base_url, &format!("Bearer {jwt_a}"), "A専用車両")
                    .await;
            // category はテナント B のもの (category_id は正しいテナントのものを渡す)
            let category_id = insert_maintenance_category(state.pool(), tenant_b, "修理").await;

            let res = client
                .post(format!("{base_url}/api/maintenance/records"))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .json(&serde_json::json!({
                    "vehicle_id": vehicle_id,
                    "category_id": category_id,
                    "performed_on": "2026-02-01",
                }))
                .send()
                .await
                .unwrap();
            assert!(
                res.status() == 400 || res.status() == 404,
                "500 になってはいけない (実際: {})",
                res.status()
            );
        }
    );
}

#[tokio::test]
async fn test_create_record_with_other_tenant_category_returns_400() {
    test_group!("整備記録: FK テナント検証");
    test_case!(
        "他テナントの category_id を指定すると 500 ではなく 400",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a =
                common::create_test_tenant(state.pool(), "Maintenance Records FK2 Tenant A").await;
            let tenant_b =
                common::create_test_tenant(state.pool(), "Maintenance Records FK2 Tenant B").await;
            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let jwt_b = common::create_test_jwt(tenant_b, "admin");
            let client = reqwest::Client::new();

            // vehicle も category もテナント A のものを、テナント B の JWT で作成しようとする
            let vehicle_id =
                create_test_vehicle(&client, &base_url, &format!("Bearer {jwt_a}"), "A専用車両2")
                    .await;
            let category_id = insert_maintenance_category(state.pool(), tenant_a, "部品交換").await;

            let res = client
                .post(format!("{base_url}/api/maintenance/records"))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .json(&serde_json::json!({
                    "vehicle_id": vehicle_id,
                    "category_id": category_id,
                    "performed_on": "2026-02-01",
                }))
                .send()
                .await
                .unwrap();
            assert!(
                res.status() == 400 || res.status() == 404,
                "500 になってはいけない (実際: {})",
                res.status()
            );
        }
    );
}

#[tokio::test]
async fn test_update_record_with_other_tenant_vehicle_returns_400() {
    test_group!("整備記録: FK テナント検証");
    test_case!(
        "更新時も他テナントの vehicle_id を指定すると 500 ではなく 400",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a =
                common::create_test_tenant(state.pool(), "Maintenance Records Update FK Tenant A")
                    .await;
            let tenant_b =
                common::create_test_tenant(state.pool(), "Maintenance Records Update FK Tenant B")
                    .await;
            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let jwt_b = common::create_test_jwt(tenant_b, "admin");
            let client = reqwest::Client::new();
            let auth_b = format!("Bearer {jwt_b}");

            // テナント B 内で正しい記録を1件作る
            let vehicle_b = create_test_vehicle(&client, &base_url, &auth_b, "B専用車両").await;
            let category_b = insert_maintenance_category(state.pool(), tenant_b, "定期点検B").await;
            let res = client
                .post(format!("{base_url}/api/maintenance/records"))
                .header("Authorization", &auth_b)
                .json(&serde_json::json!({
                    "vehicle_id": vehicle_b,
                    "category_id": category_b,
                    "performed_on": "2026-03-01",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let record: Value = res.json().await.unwrap();
            let id = record["id"].as_str().unwrap();

            // テナント A の vehicle_id へ付け替えようとする
            let vehicle_a =
                create_test_vehicle(&client, &base_url, &format!("Bearer {jwt_a}"), "A専用車両3")
                    .await;
            let res = client
                .put(format!("{base_url}/api/maintenance/records/{id}"))
                .header("Authorization", &auth_b)
                .json(&serde_json::json!({ "vehicle_id": vehicle_a }))
                .send()
                .await
                .unwrap();
            assert!(
                res.status() == 400 || res.status() == 404,
                "500 になってはいけない (実際: {})",
                res.status()
            );
        }
    );
}

#[tokio::test]
async fn test_maintenance_records_list_filters() {
    test_group!("整備記録: 一覧フィルタ");
    test_case!(
        "vehicle_id / date_from / date_to / q の各フィルタが効く",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Records Filter Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let vehicle_1 = create_test_vehicle(&client, &base_url, &auth, "足立1号車").await;
            let vehicle_2 = create_test_vehicle(&client, &base_url, &auth, "足立2号車").await;
            let category_id =
                insert_maintenance_category(state.pool(), tenant_id, "フィルタ用カテゴリ").await;

            // vehicle_1: 2026-01-05 のオイル交換
            let res = client
                .post(format!("{base_url}/api/maintenance/records"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "vehicle_id": vehicle_1,
                    "category_id": category_id,
                    "performed_on": "2026-01-05",
                    "vendor": "オイル交換専門店",
                    "description": "エンジンオイル交換",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let record_1: Value = res.json().await.unwrap();
            let id_1 = record_1["id"].as_str().unwrap().to_string();

            // vehicle_2: 2026-03-20 のタイヤ交換
            let res = client
                .post(format!("{base_url}/api/maintenance/records"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "vehicle_id": vehicle_2,
                    "category_id": category_id,
                    "performed_on": "2026-03-20",
                    "vendor": "タイヤ館テスト店",
                    "description": "冬タイヤへ交換",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let record_2: Value = res.json().await.unwrap();
            let id_2 = record_2["id"].as_str().unwrap().to_string();

            // vehicle_id フィルタ: vehicle_1 のみ
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/records?vehicle_id={vehicle_1}"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let list: Value = res.json().await.unwrap();
            let records = list["records"].as_array().unwrap();
            assert_eq!(list["total"], 1);
            assert!(records.iter().any(|r| r["id"] == id_1));
            assert!(!records.iter().any(|r| r["id"] == id_2));

            // date_from / date_to フィルタ: 2026-03-01 以降は record_2 のみ
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/records?date_from=2026-03-01"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let list: Value = res.json().await.unwrap();
            let records = list["records"].as_array().unwrap();
            assert!(records.iter().any(|r| r["id"] == id_2));
            assert!(!records.iter().any(|r| r["id"] == id_1));

            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/records?date_to=2026-01-31"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let list: Value = res.json().await.unwrap();
            let records = list["records"].as_array().unwrap();
            assert!(records.iter().any(|r| r["id"] == id_1));
            assert!(!records.iter().any(|r| r["id"] == id_2));

            // q フィルタ: description の部分一致
            let res = client
                .get(format!("{base_url}/api/maintenance/records?q=オイル"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let list: Value = res.json().await.unwrap();
            let records = list["records"].as_array().unwrap();
            assert!(records.iter().any(|r| r["id"] == id_1));
            assert!(!records.iter().any(|r| r["id"] == id_2));

            // q フィルタ: vendor の部分一致
            let res = client
                .get(format!("{base_url}/api/maintenance/records?q=タイヤ館"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let list: Value = res.json().await.unwrap();
            let records = list["records"].as_array().unwrap();
            assert!(records.iter().any(|r| r["id"] == id_2));
            assert!(!records.iter().any(|r| r["id"] == id_1));
        }
    );
}

#[tokio::test]
async fn test_maintenance_records_tenant_isolation() {
    test_group!("整備記録: RLS");
    test_case!(
        "別テナントの整備記録は一覧にも個別取得にも出ない",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a =
                common::create_test_tenant(state.pool(), "Maintenance Records RLS Tenant A").await;
            let tenant_b =
                common::create_test_tenant(state.pool(), "Maintenance Records RLS Tenant B").await;
            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let jwt_b = common::create_test_jwt(tenant_b, "admin");
            let client = reqwest::Client::new();
            let auth_a = format!("Bearer {jwt_a}");
            let auth_b = format!("Bearer {jwt_b}");

            let vehicle_a = create_test_vehicle(&client, &base_url, &auth_a, "テナントA車両").await;
            let category_a =
                insert_maintenance_category(state.pool(), tenant_a, "テナントA用カテゴリ").await;

            let res = client
                .post(format!("{base_url}/api/maintenance/records"))
                .header("Authorization", &auth_a)
                .json(&serde_json::json!({
                    "vehicle_id": vehicle_a,
                    "category_id": category_a,
                    "performed_on": "2026-04-01",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let record_a: Value = res.json().await.unwrap();
            let id_a = record_a["id"].as_str().unwrap();

            // tenant B の一覧には出ない
            let res = client
                .get(format!("{base_url}/api/maintenance/records"))
                .header("Authorization", &auth_b)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let list: Value = res.json().await.unwrap();
            let records = list["records"].as_array().unwrap();
            assert!(!records.iter().any(|r| r["id"] == id_a));

            // tenant B から個別取得すると 404 (RLS で行が見えない)
            let res = client
                .get(format!("{base_url}/api/maintenance/records/{id_a}"))
                .header("Authorization", &auth_b)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

// ===========================================================================
// 整備記録の添付ファイル (Refs #651、#c651-6)。
//
// `records.rs` (#c651-5) の実装には依存しない — `maintenance_records` の表は
// migrations/147 で既にあるので、ここでは SQL で直接 1 行用意する。
// ===========================================================================

/// `maintenance_records` に検証用の 1 行を直接 INSERT する (vehicle / category も
/// 合成して用意する)。`records.rs` の API を待たずに並列で書けるようにするための
/// SQL 直叩き (このタスクの親指示どおり)。
async fn insert_maintenance_record(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    let vehicle_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alc_api.maintenance_vehicles (tenant_id, registration_number) \
         VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("テスト車両-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("maintenance_vehicles の合成行を入れられない");

    let category_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alc_api.maintenance_categories (tenant_id, name) \
         VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("テストカテゴリ-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("maintenance_categories の合成行を入れられない");

    sqlx::query_scalar(
        "INSERT INTO alc_api.maintenance_records (tenant_id, vehicle_id, category_id, performed_on) \
         VALUES ($1, $2, $3, CURRENT_DATE) RETURNING id",
    )
    .bind(tenant_id)
    .bind(vehicle_id)
    .bind(category_id)
    .fetch_one(pool)
    .await
    .expect("maintenance_records の合成行を入れられない")
}

#[tokio::test]
async fn test_maintenance_files_attach_list_download_delete_cycle() {
    test_group!("整備記録の添付ファイル: 一巡");
    test_case!(
        "添付 → 一覧 → download → ソフト削除 → 一覧から消える",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Files Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            let record_id = insert_maintenance_record(state.pool(), tenant_id).await;

            // 添付
            let form = reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(b"hello maintenance".to_vec())
                    .file_name("photo.jpg")
                    .mime_str("image/jpeg")
                    .unwrap(),
            );
            let res = client
                .post(format!(
                    "{base_url}/api/maintenance/records/{record_id}/files"
                ))
                .header("Authorization", &auth)
                .multipart(form)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let created: Value = res.json().await.unwrap();
            assert_eq!(created["filename"], "photo.jpg");
            assert_eq!(created["content_type"], "image/jpeg");
            assert_eq!(created["size_bytes"], 17);
            let file_id = created["id"].as_str().unwrap().to_string();

            // 一覧
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/records/{record_id}/files"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let list: Value = res.json().await.unwrap();
            let items = list.as_array().unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0]["id"], file_id);

            // download
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/files/{file_id}/download"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
            let body = res.bytes().await.unwrap();
            assert_eq!(body.as_ref(), b"hello maintenance");

            // ソフト削除
            let res = client
                .delete(format!("{base_url}/api/maintenance/files/{file_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            // 削除後は一覧から消える
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/records/{record_id}/files"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let list: Value = res.json().await.unwrap();
            assert!(list.as_array().unwrap().is_empty());

            // 削除後の download は 404 (get の deleted_at IS NULL 述語)
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/files/{file_id}/download"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);

            // 2 回目の削除は 404
            let res = client
                .delete(format!("{base_url}/api/maintenance/files/{file_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

#[tokio::test]
async fn test_maintenance_files_upload_to_foreign_tenant_record_404() {
    test_group!("整備記録の添付ファイル: テナント境界");
    test_case!(
        "他テナントの record_id に添付しようとすると 404",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a =
                common::create_test_tenant(state.pool(), "Maintenance Files Tenant A").await;
            let tenant_b =
                common::create_test_tenant(state.pool(), "Maintenance Files Tenant B").await;
            let jwt_b = common::create_test_jwt(tenant_b, "admin");
            let client = reqwest::Client::new();

            // record は tenant A のもの
            let record_id = insert_maintenance_record(state.pool(), tenant_a).await;

            // tenant B の JWT で添付しようとする
            let form = reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(b"hello".to_vec())
                    .file_name("test.txt")
                    .mime_str("text/plain")
                    .unwrap(),
            );
            let res = client
                .post(format!(
                    "{base_url}/api/maintenance/records/{record_id}/files"
                ))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .multipart(form)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);

            // 一覧も同様に 404 (record_id が自テナントのものか確認してから一覧する)
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/records/{record_id}/files"
                ))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);

            // 存在しない record_id (どのテナントにも属さない) でも 404
            let res = client
                .post(format!(
                    "{base_url}/api/maintenance/records/{}/files",
                    Uuid::new_v4()
                ))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .multipart(
                    reqwest::multipart::Form::new().part(
                        "file",
                        reqwest::multipart::Part::bytes(b"hello".to_vec())
                            .file_name("test.txt")
                            .mime_str("text/plain")
                            .unwrap(),
                    ),
                )
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

#[tokio::test]
async fn test_maintenance_files_foreign_tenant_file_id_404() {
    test_group!("整備記録の添付ファイル: テナント境界");
    test_case!(
        "他テナントの file_id を download / delete しようとすると 404",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a =
                common::create_test_tenant(state.pool(), "Maintenance Files Owner Tenant").await;
            let tenant_b =
                common::create_test_tenant(state.pool(), "Maintenance Files Stranger Tenant").await;
            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let jwt_b = common::create_test_jwt(tenant_b, "admin");
            let client = reqwest::Client::new();

            let record_id = insert_maintenance_record(state.pool(), tenant_a).await;

            let form = reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(b"secret".to_vec())
                    .file_name("secret.txt")
                    .mime_str("text/plain")
                    .unwrap(),
            );
            let res = client
                .post(format!(
                    "{base_url}/api/maintenance/records/{record_id}/files"
                ))
                .header("Authorization", format!("Bearer {jwt_a}"))
                .multipart(form)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let created: Value = res.json().await.unwrap();
            let file_id = created["id"].as_str().unwrap().to_string();
            // storage_key にテナント ID が入っていることを確かめる (認可の根拠には
            // しないが、prefix の形は固定しておく)
            let storage_key = created["storage_key"].as_str().unwrap();
            assert!(storage_key.starts_with(&format!("{tenant_a}/maintenance/{record_id}/")));

            // tenant B から download しようとすると 404
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/files/{file_id}/download"
                ))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);

            // tenant B から delete しようとすると 404 (ファイルは消えない)
            let res = client
                .delete(format!("{base_url}/api/maintenance/files/{file_id}"))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);

            // tenant A からは引き続き download できる (tenant B の操作で消えていない)
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/files/{file_id}/download"
                ))
                .header("Authorization", format!("Bearer {jwt_a}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
        }
    );
}

#[tokio::test]
async fn test_maintenance_files_tenant_isolation_in_list() {
    test_group!("整備記録の添付ファイル: RLS");
    test_case!("別テナントのファイルは一覧に出ない", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_a =
            common::create_test_tenant(state.pool(), "Maintenance Files RLS Tenant A").await;
        let tenant_b =
            common::create_test_tenant(state.pool(), "Maintenance Files RLS Tenant B").await;
        let jwt_a = common::create_test_jwt(tenant_a, "admin");
        let jwt_b = common::create_test_jwt(tenant_b, "admin");
        let client = reqwest::Client::new();

        let record_a = insert_maintenance_record(state.pool(), tenant_a).await;
        let record_b = insert_maintenance_record(state.pool(), tenant_b).await;

        // tenant A の記録に添付
        let form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(b"a-file".to_vec())
                .file_name("a.txt")
                .mime_str("text/plain")
                .unwrap(),
        );
        let res = client
            .post(format!(
                "{base_url}/api/maintenance/records/{record_a}/files"
            ))
            .header("Authorization", format!("Bearer {jwt_a}"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        // tenant B 自身の記録の一覧には tenant A のファイルは出ない (そもそも
        // 別の record_id なので空のまま)
        let res = client
            .get(format!(
                "{base_url}/api/maintenance/records/{record_b}/files"
            ))
            .header("Authorization", format!("Bearer {jwt_b}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let list: Value = res.json().await.unwrap();
        assert!(list.as_array().unwrap().is_empty());

        // tenant B が tenant A の record_id を直接指定しても 404 (RLS + 明示述語)
        let res = client
            .get(format!(
                "{base_url}/api/maintenance/records/{record_a}/files"
            ))
            .header("Authorization", format!("Bearer {jwt_b}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_carins_import_candidates_and_import() {
    test_group!("車両マスタ: carins 取り込み");
    test_case!(
        "未取り込みの carins を一覧し、1 トランザクションで作成/紐づけ/skip する",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id =
                common::create_test_tenant(state.pool(), "Maintenance Carins Import Tenant").await;
            let jwt = common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();
            let auth = format!("Bearer {jwt}");

            // (a) 既存車両と登録番号が一致する車検証 (全角 — SQL 側 translate で突き合わせる)
            let car_id_link = test_car_id();
            insert_car_inspection(
                &state,
                tenant_id,
                &test_cert_no(),
                &car_id_link,
                "品川３３０あ１２－３４",
            )
            .await;
            // (b) 一致する車両が無い車検証
            let car_id_create = test_car_id();
            insert_car_inspection(
                &state,
                tenant_id,
                &test_cert_no(),
                &car_id_create,
                "足立300さ99-99",
            )
            .await;

            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "registration_number": "品川330あ12-34" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let existing: Value = res.json().await.unwrap();
            let existing_id = existing["id"].as_str().unwrap().to_string();

            // 候補一覧: 2 件とも未取り込みで出る。(a) だけ existing_vehicle_id が付く
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/vehicles/carins-import-candidates"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let candidates: Value = res.json().await.unwrap();
            let candidates = candidates.as_array().unwrap().clone();
            let link_row = candidates
                .iter()
                .find(|c| c["car_id"] == car_id_link)
                .unwrap_or_else(|| panic!("紐づけ対象が候補に出ること: {candidates:?}"));
            assert_eq!(
                link_row["existing_vehicle_id"],
                Value::String(existing_id.clone())
            );
            assert_eq!(link_row["car_no"], "品川３３０あ１２－３４");
            // 最小方針: 所有者・住所・車台番号は返さない。
            // JSON オブジェクトのキー順は契約ではない (serde_json は preserve_order
            // 無しだと BTreeMap = アルファベット順) ので、集合として比べる。
            let keys: std::collections::BTreeSet<&str> = link_row
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(
                keys,
                ["car_id", "cert_no", "car_no", "existing_vehicle_id"]
                    .into_iter()
                    .collect::<std::collections::BTreeSet<&str>>(),
                "所有者・住所・車台番号を含まない最小の 4 フィールドちょうどであること"
            );
            let create_row = candidates
                .iter()
                .find(|c| c["car_id"] == car_id_create)
                .unwrap_or_else(|| panic!("新規作成対象が候補に出ること: {candidates:?}"));
            assert!(create_row["existing_vehicle_id"].is_null());

            // 取り込み: 1 件は既存に紐づけ、1 件は新規作成
            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles/carins-import"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "car_ids": [car_id_link, car_id_create] }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let result: Value = res.json().await.unwrap();
            assert_eq!(result["created"], 1);
            assert_eq!(result["linked"], 1);
            assert_eq!(result["skipped"], 0);

            // 既存行が新しく作られず、その場で紐づいていること
            let res = client
                .get(format!("{base_url}/api/maintenance/vehicles/{existing_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let vehicle: Value = res.json().await.unwrap();
            assert_eq!(vehicle["car_id"], car_id_link);
            assert!(!vehicle["carins_linked_at"].is_null());

            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/vehicles?q=足立300さ99-99"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let list: Value = res.json().await.unwrap();
            assert_eq!(list["total"], 1, "新規作成は 1 行だけ: {list:?}");
            assert_eq!(list["items"][0]["car_id"], car_id_create);

            // 取り込み済みは候補に出ない
            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/vehicles/carins-import-candidates"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            let candidates: Value = res.json().await.unwrap();
            assert!(
                candidates.as_array().unwrap().is_empty(),
                "取り込み済みは候補から消えること: {candidates:?}"
            );

            // 再実行しても重複行は増えない (冪等 — 全部 skip)
            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles/carins-import"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "car_ids": [car_id_link, car_id_create] }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let result: Value = res.json().await.unwrap();
            assert_eq!(result["created"], 0);
            assert_eq!(result["linked"], 0);
            assert_eq!(result["skipped"], 2);

            // 空の car_ids は 400
            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles/carins-import"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "car_ids": [] }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 400);
        }
    );
}

#[tokio::test]
async fn test_carins_import_tenant_isolation() {
    test_group!("車両マスタ: carins 取り込みの RLS");
    test_case!(
        "他テナントの CarId は候補に出ず、直接指定しても取り込まれない",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a =
                common::create_test_tenant(state.pool(), "Maintenance Import RLS Tenant A").await;
            let tenant_b =
                common::create_test_tenant(state.pool(), "Maintenance Import RLS Tenant B").await;
            let jwt_b = common::create_test_jwt(tenant_b, "admin");
            let client = reqwest::Client::new();

            let car_id_a = test_car_id();
            insert_car_inspection(
                &state,
                tenant_a,
                &test_cert_no(),
                &car_id_a,
                "横浜500た55-55",
            )
            .await;

            let res = client
                .get(format!(
                    "{base_url}/api/maintenance/vehicles/carins-import-candidates"
                ))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let candidates: Value = res.json().await.unwrap();
            assert!(
                !candidates
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["car_id"] == car_id_a),
                "他テナントの CarId が候補に出ないこと: {candidates:?}"
            );

            let res = client
                .post(format!("{base_url}/api/maintenance/vehicles/carins-import"))
                .header("Authorization", format!("Bearer {jwt_b}"))
                .json(&serde_json::json!({ "car_ids": [car_id_a] }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let result: Value = res.json().await.unwrap();
            assert_eq!(result["created"], 0);
            assert_eq!(result["linked"], 0);
            assert_eq!(result["skipped"], 1);
        }
    );
}
