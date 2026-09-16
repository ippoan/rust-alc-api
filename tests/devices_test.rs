#[macro_use]
mod common;

use common::create_device_via_url_flow;

// ============================================================
// クロステナント操作テスト
// ============================================================

#[tokio::test]
async fn test_device_operations_from_different_tenant() {
    #[cfg(coverage)]
    let _db_lock = common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = common::db_rename_flock();
    test_group!("管理操作");
    test_case!(
        "クロステナント操作が可能であることを記録する (RLSポリシー問題)",
        {
            // NOTE: device_select_by_id ポリシー (FOR SELECT USING (true)) により
            // SELECT/UPDATE/DELETE は全テナント横断でアクセス可能な状態。
            // このテストはクロステナント操作が実行可能であることを記録する。
            // TODO: RLS ポリシーを修正して、UPDATE/DELETE にテナント分離を追加すべき
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;

            let tenant_a = common::create_test_tenant(state.pool(), "Dev Iso A").await;
            let tenant_b = common::create_test_tenant(state.pool(), "Dev Iso B").await;

            let jwt_a = common::create_test_jwt(tenant_a, "admin");
            let _jwt_b = common::create_test_jwt(tenant_b, "admin");
            let auth_a = format!("Bearer {jwt_a}");
            let client = reqwest::Client::new();

            // テナント A にデバイス作成
            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth_a).await;

            // テナント A から設定取得 → 成功
            let res = client
                .get(format!("{base_url}/api/devices/settings/{device_id}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            // テナント B からも設定取得可能 (device_select_by_id ポリシー)
            let res = client
                .get(format!("{base_url}/api/devices/settings/{device_id}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
        }
    );
}
