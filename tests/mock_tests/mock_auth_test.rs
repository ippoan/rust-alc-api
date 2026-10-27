// 保護ルートの認証の mock テスト。
//
// #479 PR-3 で旧ログイン経路は auth-worker へ、W2b (Refs #680) で me・logout・my-orgs も
// alc-auth-worker へ移り、rust から撤去された。ここには「不正な token は保護ルートで 401」だけを残す
// (保護ルートの例は rust に残る Bot 設定の export)。

use uuid::Uuid;

use std::sync::Arc;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockAuthRepository;

// ============================================================
// protected route — invalid/malformed Bearer token returns 401
// ============================================================

#[tokio::test]
async fn test_require_jwt_invalid_token_returns_401() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let client = reqwest::Client::new();
    // 不正な Bearer token を保護 endpoint (require_tenant_header の後ろ) に送る
    let res = client
        .get(format!(
            "{base_url}/api/admin/bot/configs/export?tenant_id={}",
            Uuid::new_v4()
        ))
        .header("Authorization", "Bearer invalid-token-here")
        .send()
        .await
        .unwrap();

    // test_proxy_inject の decode に失敗 → identity ヘッダー非注入 →
    // require_tenant_header で 401
    assert_eq!(res.status(), 401);
}
