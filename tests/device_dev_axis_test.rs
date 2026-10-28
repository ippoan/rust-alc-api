//! dev端末 (開発用の鍵) の印 `X-Device-Dev: 1` を、認証の無い公開 route では接続に立てないことを
//! 実 DB で固定する。
//!
//! Refs ippoan/alc-app#387
//!
//! 印を立てるのは認証を通した要求だけで、行の出し分けは列の既定値と RLS (alc-migrations 153) が担う。
//! 印を立てた要求で行が分かれることの検査は、点呼の口と一緒に alc-tenko-worker へ移った (W7d、Refs #680)。
//!
//! ほかの DB テストは superuser (`postgres`) で繋ぐので RLS を素通りする。ここだけは
//! `common::setup_app_state_as_app_role` で実行用ロール `alc_api_rt` として走らせる。
//!
//! 回し方は他の DB integration テストと同じ:
//!   `make db-up && source .test-config && cargo test --test device_dev_axis_test`
//! CI は ci.yml の `bazel-test-db` shard (postgres service + TEST_DATABASE_URL)。

#[macro_use]
mod common;

struct Ctx {
    /// `alc_api_rt` で繋ぐ、サーバが使う側の pool
    app: sqlx::PgPool,
    base_url: String,
    client: reqwest::Client,
}

/// `alc_api_rt` で動くサーバを立てる
async fn setup(max_connections: u32, reset_on_release: bool) -> Ctx {
    // migration と GRANT を流す (setup_app_state_as_app_role は流さない)
    let _ = common::setup_app_state().await;
    let app_state = common::setup_app_state_as_app_role(max_connections, reset_on_release).await;
    let app = app_state.pool().clone();
    let base_url = common::spawn_test_server(app_state).await;
    Ctx {
        app,
        base_url,
        client: reqwest::Client::new(),
    }
}

/// 接続に残っている `app.device_dev` を、サーバと同じ pool から直接読む
async fn device_dev_on_connection(ctx: &Ctx) -> String {
    sqlx::query_scalar("SELECT coalesce(current_setting('app.device_dev', true), '')")
        .fetch_one(&ctx.app)
        .await
        .unwrap()
}

#[tokio::test]
async fn test_public_route_ignores_the_header() {
    test_group!("認証の無い公開 route");
    test_case!(
        "X-Device-Dev: 1 を付けても接続は dev にならない",
        {
            // after_release なしの 1 接続にして、要求の後の接続の状態をそのまま見る
            let ctx = setup(1, false).await;
            // 公開の口 (文書の閲覧の token の引き当て。DB を読む)。端末の登録の口は alc-devices-worker へ (W6b)、
            // テナントの slug の引き当ては alc-auth-worker へ (W2c) 移った
            let res = ctx
                .client
                .get(format!(
                    "{}/api/notify/v/{}",
                    ctx.base_url,
                    uuid::Uuid::new_v4()
                ))
                .header("X-Device-Dev", "1")
                .send()
                .await
                .unwrap();
            assert_ne!(res.status(), 401, "公開 route なので認証では弾かれない");
            assert_eq!(device_dev_on_connection(&ctx).await, "");
        }
    );
}
