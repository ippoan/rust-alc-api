//! 社員マスタ同期 (`PUT /api/employees/bulk-by-code`) が、社員番号で保留中のカードを結ぶことの
//! DB integration テスト (Refs ippoan/rust-alc-api#644)。
//!
//! カード台帳の一括取り込み (`PUT /api/timecard/cards/bulk-by-code`) は ippoan/alc-people-worker へ
//! 移った (W4d-a)。社員がまだ居ないカードはそこで `employee_id = NULL` +
//! `pending_employee_code = <社員番号>` の保留行として入り、**結び付けは社員が現れる側**
//! (`alc-misc/src/repo/employees.rs` の `link_pending_cards`) が行う。社員の口が W4d-b で移るまで、
//! この結び付けはここで固定する (保留行は SQL で入れる)。

#[macro_use]
mod common;

use serde_json::{json, Value};
use uuid::Uuid;

/// 一括取り込みが入れる行の出所 (alc-people-worker の `CARD_SOURCE_LEDGER_SYNC` と同じ値)
const CARD_SOURCE_LEDGER_SYNC: &str = "timecard-ic-ledger-import";

struct Ctx {
    state: rust_alc_api::AppState,
    base_url: String,
    tenant: Uuid,
    auth: String,
    client: reqwest::Client,
}

async fn setup(name: &str) -> Ctx {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant = common::create_test_tenant(state.pool(), name).await;
    let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
    Ctx {
        state,
        base_url,
        tenant,
        auth,
        client: reqwest::Client::new(),
    }
}

impl Ctx {
    async fn employee(&self, name: &str, code: &str) -> Uuid {
        let emp =
            common::create_test_employee(&self.client, &self.base_url, &self.auth, name, code)
                .await;
        emp["id"].as_str().unwrap().parse().unwrap()
    }

    /// カード 1 枚を台帳に入れる (`owner` が None なら社員番号 `code` で保留)
    async fn card(&self, card_id: &str, owner: Option<Uuid>, code: &str) {
        let pending = owner.is_none().then_some(code);
        sqlx::query(
            "INSERT INTO timecard_cards (tenant_id, employee_id, card_id, source, pending_employee_code) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(self.tenant)
        .bind(owner)
        .bind(card_id)
        .bind(CARD_SOURCE_LEDGER_SYNC)
        .bind(pending)
        .execute(self.state.pool())
        .await
        .unwrap();
    }

    /// 1 枚の `(持ち主, 保留中の社員番号)`。**結び付いたら code は NULL に戻る**
    async fn card_row(&self, card_id: &str) -> (Option<Uuid>, Option<String>) {
        sqlx::query_as::<_, (Option<Uuid>, Option<String>)>(
            "SELECT employee_id, pending_employee_code FROM timecard_cards \
             WHERE tenant_id = $1 AND card_id = $2",
        )
        .bind(self.tenant)
        .bind(card_id)
        .fetch_one(self.state.pool())
        .await
        .unwrap()
    }

    /// 社員マスタ同期 (relay が叩く口) を 1 件で叩く。**`nfc_id` は送らない** — relay は意図的に null を送る
    async fn employees_bulk(&self, code: &str, name: &str) -> Value {
        let res = self
            .client
            .put(format!("{}/api/employees/bulk-by-code", self.base_url))
            .header("Authorization", &self.auth)
            .json(&json!({"items": [{"code": code, "name": name, "nfc_id": null}]}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "employees/bulk-by-code");
        res.json().await.unwrap()
    }
}

#[tokio::test]
async fn test_employee_sync_links_pending_cards() {
    test_group!("社員マスタ同期: 保留カードの結び付け");

    test_case!(
        "社員マスタ同期が社員を入れた時点で結び付き、保留の code は消える",
        {
            let ctx = setup("Pending Cards A").await;
            ctx.card("01401d0b1d37b660", None, "E900").await;

            // relay が後から同じ code の社員を入れる (新規作成の経路)
            let summary = ctx.employees_bulk("E900", "あとから 太郎").await;
            assert_eq!(summary["created"], 1, "{summary}");

            let (owner, pending) = ctx.card_row("01401d0b1d37b660").await;
            let employee_id: Uuid = sqlx::query_as::<_, (Uuid,)>(
                "SELECT id FROM employees WHERE tenant_id = $1 AND code = $2",
            )
            .bind(ctx.tenant)
            .bind("E900")
            .fetch_one(ctx.state.pool())
            .await
            .unwrap()
            .0;
            assert_eq!(owner, Some(employee_id), "社員が現れたら結び付く");
            assert_eq!(pending, None, "結び付いたら保留の code は NULL に戻る");
        }
    );

    test_case!(
        "既に居る社員を同期し直しても別の code の保留カードは巻き込まれない",
        {
            let ctx = setup("Pending Cards B").await;
            let alice = ctx.employee("既に 居る", "E001").await;
            ctx.card("01401d0b1d37b660", Some(alice), "E001").await;
            ctx.card("04a1b2c3d4e5f6", None, "E900").await;

            // 既存社員の更新経路 (code 一致) を通す
            ctx.employees_bulk("E001", "既に 居る").await;

            assert_eq!(ctx.card_row("01401d0b1d37b660").await, (Some(alice), None));
            assert_eq!(
                ctx.card_row("04a1b2c3d4e5f6").await,
                (None, Some("E900".to_string())),
                "別の code の保留カードは触らない"
            );
        }
    );
}
