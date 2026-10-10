//! tenant を渡して叩く query が、別テナントの id では 0 行 / 影響なしになること
//! (Refs ippoan/alc-app#387)。
//!
//! backend が表の所有者で繋いでいる間、`FORCE ROW LEVEL SECURITY` の無い表には
//! RLS が掛からず、テナントの分離は query の `WHERE tenant_id` だけが担う。
//! テスト DB の接続も RLS を素通りするので、ここで落ちる = query 側の条件が
//! 抜けている、ということ (実行用ロールへ切り替えた後は RLS が二重に止める)。
//!
//! mock (`tests/mock_helpers/`) は SQL を 1 文字も通らないので、実 DB でしか縛れない。

mod common;

use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use alc_tenko::overdue::TenkoOverdueRepository;
use rust_alc_api::db::repository::{
    AuthRepository, DeviceRepository, PgAuthRepository, PgDeviceRepository,
    PgTenantUsersRepository, PgTenkoCallRepository, PgTenkoOverdueRepository,
    TenantUsersRepository, TenkoCallRepository,
};

async fn setup() -> sqlx::PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::test_database_url())
        .await
        .expect("Failed to connect to test DB");
    common::migrate_and_grant(&pool).await;
    pool
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4().simple())
}

async fn insert_employee(pool: &sqlx::PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO alc_api.employees (tenant_id, nfc_id, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(Uuid::new_v4().to_string())
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("employee insert failed")
}

async fn insert_device(pool: &sqlx::PgPool, tenant_id: Uuid, fcm_token: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO alc_api.devices (tenant_id, device_name, device_type, status, fcm_token) \
         VALUES ($1, 'Scoped Device', 'android', 'active', $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(fcm_token)
    .fetch_one(pool)
    .await
    .expect("device insert failed")
}

/// 招待の消費 (`delete_invitation`) とログアウト (`clear_refresh_token`) は、
/// 渡したテナントの行にしか効かない。LINE の通知先の自動登録は tenant 付きで入る。
#[tokio::test]
async fn auth_repo_writes_are_tenant_scoped() {
    let pool = setup().await;
    let auth = PgAuthRepository::new(pool.clone());
    let tenant_users = PgTenantUsersRepository::new(pool.clone());
    let tenant_a = common::create_test_tenant(&pool, "Scoped Auth A").await;
    let tenant_b = common::create_test_tenant(&pool, "Scoped Auth B").await;

    // --- delete_invitation ---
    let email = format!("{}@example.test", unique("invite"));
    let invitation = tenant_users
        .invite_user(tenant_a, &email, "viewer")
        .await
        .expect("invite_user failed");
    assert_eq!(invitation.tenant_id, tenant_a);

    auth.delete_invitation(tenant_b, invitation.id)
        .await
        .expect("delete_invitation (other tenant) failed");
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.tenant_allowed_emails WHERE id = $1")
            .bind(invitation.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 1, "別テナントからは招待を消せないこと");

    auth.delete_invitation(tenant_a, invitation.id)
        .await
        .expect("delete_invitation failed");
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.tenant_allowed_emails WHERE id = $1")
            .bind(invitation.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "自テナントの招待は消えること");

    // --- clear_refresh_token ---
    let user = auth
        .create_user_line(tenant_a, &unique("line"), "Scoped User")
        .await
        .expect("create_user_line failed");
    auth.save_refresh_token(
        user.id,
        "scoped-hash",
        Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .expect("save_refresh_token failed");

    auth.clear_refresh_token(tenant_b, user.id)
        .await
        .expect("clear_refresh_token (other tenant) failed");
    let hash: Option<String> =
        sqlx::query_scalar("SELECT refresh_token_hash FROM alc_api.users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        hash.as_deref(),
        Some("scoped-hash"),
        "別テナントからは refresh token を消せないこと"
    );

    auth.clear_refresh_token(tenant_a, user.id)
        .await
        .expect("clear_refresh_token failed");
    let hash: Option<String> =
        sqlx::query_scalar("SELECT refresh_token_hash FROM alc_api.users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(hash.is_none(), "自テナントの refresh token は消えること");

    // --- register_line_recipient ---
    let line_user_id = unique("recipient");
    auth.register_line_recipient(tenant_a, "Scoped Recipient", &line_user_id)
        .await
        .expect("register_line_recipient failed");
    let recipient_tenant: Uuid = sqlx::query_scalar(
        "SELECT tenant_id FROM alc_api.notify_recipients WHERE line_user_id = $1",
    )
    .bind(&line_user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recipient_tenant, tenant_a);
}

/// 管理画面のユーザー / 招待の一覧・削除・role 変更は、自テナントの行だけに届く。
#[tokio::test]
async fn tenant_users_repo_is_tenant_scoped() {
    let pool = setup().await;
    let auth = PgAuthRepository::new(pool.clone());
    let repo = PgTenantUsersRepository::new(pool.clone());
    let tenant_a = common::create_test_tenant(&pool, "Scoped Users A").await;
    let tenant_b = common::create_test_tenant(&pool, "Scoped Users B").await;

    let user_a = auth
        .create_user_line(tenant_a, &unique("line-a"), "User A")
        .await
        .expect("create_user_line A failed");
    let user_b = auth
        .create_user_line(tenant_b, &unique("line-b"), "User B")
        .await
        .expect("create_user_line B failed");
    let invite_a = repo
        .invite_user(tenant_a, &format!("{}@example.test", unique("a")), "viewer")
        .await
        .expect("invite_user A failed");
    let invite_b = repo
        .invite_user(tenant_b, &format!("{}@example.test", unique("b")), "viewer")
        .await
        .expect("invite_user B failed");

    // 一覧
    let users = repo.list_users(tenant_a).await.expect("list_users failed");
    assert!(users.iter().any(|u| u.id == user_a.id));
    assert!(
        users.iter().all(|u| u.id != user_b.id),
        "別テナントのユーザーが一覧に出ないこと"
    );
    let invitations = repo
        .list_invitations(tenant_a)
        .await
        .expect("list_invitations failed");
    assert!(invitations.iter().any(|i| i.id == invite_a.id));
    assert!(
        invitations.iter().all(|i| i.tenant_id == tenant_a),
        "別テナントの招待が一覧に出ないこと"
    );

    // role 変更 (email 指定): 別テナントからは 0 行
    assert!(!repo
        .update_role_by_email(tenant_a, &user_b.email, "admin")
        .await
        .expect("update_role_by_email (user) failed"));
    assert!(!repo
        .update_role_by_email(tenant_a, &invite_b.email, "admin")
        .await
        .expect("update_role_by_email (invite) failed"));
    let role: String = sqlx::query_scalar("SELECT role FROM alc_api.users WHERE id = $1")
        .bind(user_b.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(role, "viewer", "別テナントのユーザーの role が動かないこと");
    let role: String =
        sqlx::query_scalar("SELECT role FROM alc_api.tenant_allowed_emails WHERE id = $1")
            .bind(invite_b.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(role, "viewer", "別テナントの招待の role が動かないこと");

    // 削除 (id 指定 / email 指定): 別テナントからは 0 行
    repo.delete_invitation(tenant_a, invite_b.id)
        .await
        .expect("delete_invitation failed");
    repo.delete_user(tenant_a, user_b.id)
        .await
        .expect("delete_user failed");
    assert!(!repo
        .delete_by_email(tenant_a, &user_b.email)
        .await
        .expect("delete_by_email (user) failed"));
    assert!(!repo
        .delete_by_email(tenant_a, &invite_b.email)
        .await
        .expect("delete_by_email (invite) failed"));
    let users_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.users WHERE id = $1")
        .bind(user_b.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(users_left, 1, "別テナントのユーザーが消えないこと");
    let invites_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.tenant_allowed_emails WHERE id = $1")
            .bind(invite_b.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(invites_left, 1, "別テナントの招待が消えないこと");

    // 自テナントには効く
    assert!(repo
        .update_role_by_email(tenant_b, &user_b.email, "admin")
        .await
        .expect("update_role_by_email (own) failed"));
    assert!(repo
        .delete_by_email(tenant_b, &invite_b.email)
        .await
        .expect("delete_by_email (own) failed"));
    repo.delete_user(tenant_b, user_b.id)
        .await
        .expect("delete_user (own) failed");
    let users_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.users WHERE id = $1")
        .bind(user_b.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(users_left, 0);
}

/// 端末のクレーム (QR 永久) と dismiss 用の FCM トークン一覧は、渡したテナントの行だけ。
#[tokio::test]
async fn devices_repo_is_tenant_scoped() {
    let pool = setup().await;
    let repo = PgDeviceRepository::new(pool.clone());
    let tenant_a = common::create_test_tenant(&pool, "Scoped Devices A").await;
    let tenant_b = common::create_test_tenant(&pool, "Scoped Devices B").await;

    // --- claim_update_permanent_qr ---
    let code = unique("qr");
    repo.create_permanent_qr(tenant_a, &code, "Before", false, false)
        .await
        .expect("create_permanent_qr failed");
    let req_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM alc_api.device_registration_requests WHERE registration_code = $1",
    )
    .bind(&code)
    .fetch_one(&pool)
    .await
    .unwrap();

    repo.claim_update_permanent_qr(tenant_b, req_id, Some("000-0000-0000"), "Hijacked")
        .await
        .expect("claim_update_permanent_qr (other tenant) failed");
    let name: String = sqlx::query_scalar(
        "SELECT device_name FROM alc_api.device_registration_requests WHERE id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        name, "Before",
        "別テナントからは登録リクエストを書き換えられないこと"
    );

    repo.claim_update_permanent_qr(tenant_a, req_id, Some("000-0000-0000"), "After")
        .await
        .expect("claim_update_permanent_qr failed");
    let name: String = sqlx::query_scalar(
        "SELECT device_name FROM alc_api.device_registration_requests WHERE id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(name, "After");

    // --- list_tenant_fcm_tokens_except / get_device_fcm_token ---
    let token_a1 = unique("fcm-a1");
    let token_a2 = unique("fcm-a2");
    let token_b = unique("fcm-b");
    let device_a1 = insert_device(&pool, tenant_a, &token_a1).await;
    insert_device(&pool, tenant_a, &token_a2).await;
    let device_b = insert_device(&pool, tenant_b, &token_b).await;

    let tokens = repo
        .list_tenant_fcm_tokens_except(tenant_a, device_a1)
        .await
        .expect("list_tenant_fcm_tokens_except failed");
    assert_eq!(tokens, vec![token_a2.clone()]);

    // 設定変更の通知 (update_call_settings) はこの tenant 付きの口でトークンを引く
    let own = repo
        .get_device_fcm_token(tenant_a, device_a1)
        .await
        .expect("get_device_fcm_token failed");
    assert_eq!(own, Some(Some(token_a1)));
    let other = repo
        .get_device_fcm_token(tenant_a, device_b)
        .await
        .expect("get_device_fcm_token (other tenant) failed");
    assert_eq!(other, None, "別テナントの端末のトークンは引けないこと");
}

/// 超過検知の従業員名の取得・通知済みの印と、点呼用電話番号の削除は、渡したテナントの行だけ。
#[tokio::test]
async fn tenko_repos_are_tenant_scoped() {
    let pool = setup().await;
    let overdue = PgTenkoOverdueRepository::new(pool.clone());
    let tenko_call = PgTenkoCallRepository::new(pool.clone());
    let tenant_a = common::create_test_tenant(&pool, "Scoped Tenko A").await;
    let tenant_b = common::create_test_tenant(&pool, "Scoped Tenko B").await;

    // --- get_employee_name / mark_overdue_notified ---
    let employee = insert_employee(&pool, tenant_a, "Scoped Driver").await;
    let schedule: Uuid = sqlx::query_scalar(
        "INSERT INTO alc_api.tenko_schedules
            (tenant_id, employee_id, tenko_type, responsible_manager_name, scheduled_at, instruction)
         VALUES ($1, $2, 'pre_operation', 'Manager', NOW() - INTERVAL '2 hours', 'note')
         RETURNING id",
    )
    .bind(tenant_a)
    .bind(employee)
    .fetch_one(&pool)
    .await
    .expect("schedule insert failed");

    assert_eq!(
        overdue
            .get_employee_name(tenant_a, employee)
            .await
            .expect("get_employee_name failed")
            .as_deref(),
        Some("Scoped Driver")
    );
    assert_eq!(
        overdue
            .get_employee_name(tenant_b, employee)
            .await
            .expect("get_employee_name (other tenant) failed"),
        None,
        "別テナントからは従業員名を引けないこと"
    );

    overdue
        .mark_overdue_notified(tenant_b, schedule)
        .await
        .expect("mark_overdue_notified (other tenant) failed");
    let notified: bool = sqlx::query_scalar(
        "SELECT overdue_notified_at IS NOT NULL FROM alc_api.tenko_schedules WHERE id = $1",
    )
    .bind(schedule)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!notified, "別テナントからは通知済みにできないこと");

    overdue
        .mark_overdue_notified(tenant_a, schedule)
        .await
        .expect("mark_overdue_notified failed");
    let notified: bool = sqlx::query_scalar(
        "SELECT overdue_notified_at IS NOT NULL FROM alc_api.tenko_schedules WHERE id = $1",
    )
    .bind(schedule)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(notified);

    // --- delete_number ---
    let call_number = unique("call");
    let number_id = tenko_call
        .create_number(&call_number, &tenant_a.to_string(), None)
        .await
        .expect("create_number failed");

    tenko_call
        .delete_number(&tenant_b.to_string(), number_id)
        .await
        .expect("delete_number (other tenant) failed");
    let left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.tenko_call_numbers WHERE id = $1")
            .bind(number_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 1, "別テナントからは電話番号を消せないこと");

    tenko_call
        .delete_number(&tenant_a.to_string(), number_id)
        .await
        .expect("delete_number failed");
    let left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.tenko_call_numbers WHERE id = $1")
            .bind(number_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0);
}

/// 参加申請の一覧・承認・却下は、管理者のテナントの申請にしか届かない (HTTP 経由)。
#[tokio::test]
async fn access_requests_are_tenant_scoped() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();
    let pool = state.pool().clone();
    let auth = PgAuthRepository::new(pool.clone());

    let tenant_a = common::create_test_tenant(&pool, "Scoped Access A").await;
    let tenant_b = common::create_test_tenant(&pool, "Scoped Access B").await;
    let applicant = auth
        .create_user_line(tenant_b, &unique("applicant"), "Applicant")
        .await
        .expect("create_user_line failed");

    let mut request_ids = Vec::new();
    for _ in 0..2 {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO alc_api.access_requests (tenant_id, user_id) VALUES ($1, $2) RETURNING id",
        )
        .bind(tenant_a)
        .bind(applicant.id)
        .fetch_one(&pool)
        .await
        .expect("access request insert failed");
        request_ids.push(id);
    }
    let (approve_id, decline_id) = (request_ids[0], request_ids[1]);

    let admin_a = format!("Bearer {}", common::create_test_jwt(tenant_a, "admin"));
    let admin_b = format!("Bearer {}", common::create_test_jwt(tenant_b, "admin"));

    // 別テナントの管理者: 一覧に出ず、承認も却下も 404
    let res = client
        .get(format!("{base_url}/api/access-requests"))
        .header("Authorization", &admin_b)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["requests"].as_array().unwrap().len(), 0);

    let res = client
        .post(format!(
            "{base_url}/api/access-requests/{approve_id}/approve"
        ))
        .header("Authorization", &admin_b)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    let res = client
        .post(format!(
            "{base_url}/api/access-requests/{decline_id}/decline"
        ))
        .header("Authorization", &admin_b)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);

    // 自テナントの管理者: 一覧 (status 絞り込みあり / なし)・承認・却下が通る
    let res = client
        .get(format!("{base_url}/api/access-requests"))
        .header("Authorization", &admin_a)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["requests"].as_array().unwrap().len(), 2);

    let res = client
        .post(format!(
            "{base_url}/api/access-requests/{approve_id}/approve"
        ))
        .header("Authorization", &admin_a)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    let res = client
        .post(format!(
            "{base_url}/api/access-requests/{decline_id}/decline"
        ))
        .header("Authorization", &admin_a)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    let res = client
        .get(format!("{base_url}/api/access-requests?status=pending"))
        .header("Authorization", &admin_a)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["requests"].as_array().unwrap().len(), 0);
}
