//! `PUT /api/timecard/cards/bulk-by-code` の DB integration テスト
//! (Refs ippoan/rust-alc-api#644)。
//!
//! **mock テスト (`tests/mock_tests/mock_timecard_test.rs`) は repository を丸ごと
//! 差し替えるので SQL を 1 行も通りません。** この口の肝は
//! `INSERT ... ON CONFLICT (tenant_id, card_id) DO UPDATE ... WHERE` 1 文で
//! 「新規 / 付け替え / 同じ持ち主だから何もしない」を分ける所と、`card_id` の
//! CHECK 制約 (`timecard_cards_card_id_normalized`、migration 134) に当たらないこと
//! なので、**実 DB でしか固定できません**。

#[macro_use]
mod common;

use serde_json::{json, Value};
use uuid::Uuid;

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

    async fn bulk(&self, body: Value) -> reqwest::Response {
        self.client
            .put(format!("{}/api/timecard/cards/bulk-by-code", self.base_url))
            .header("Authorization", &self.auth)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn bulk_ok(&self, body: Value) -> Value {
        let res = self.bulk(body).await;
        assert_eq!(res.status(), 200, "skip があっても 200");
        res.json().await.unwrap()
    }

    async fn cards(&self) -> Vec<(String, Uuid)> {
        sqlx::query_as::<_, (String, Uuid)>(
            "SELECT card_id, employee_id FROM timecard_cards WHERE tenant_id = $1 ORDER BY card_id",
        )
        .bind(self.tenant)
        .fetch_all(self.state.pool())
        .await
        .unwrap()
    }

    async fn card_count(&self) -> i64 {
        sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM timecard_cards WHERE tenant_id = $1")
            .bind(self.tenant)
            .fetch_one(self.state.pool())
            .await
            .unwrap()
            .0
    }
}

/// 受け入れ 2: 大文字 / `:` 区切り / 前後空白 の 3 表記は同じ 1 枚。
#[tokio::test]
async fn test_three_spellings_of_one_card_become_one_row() {
    test_group!("bulk-by-code: 表記ゆれ");

    test_case!(
        "同じバッチ内の 3 表記 → 1 行 + duplicate_in_batch 2 件",
        {
            let ctx = setup("Bulk Cards A").await;
            ctx.employee("取込 太郎", "E001").await;
            ctx.employee("取込 次郎", "E002").await;
            ctx.employee("取込 三郎", "E003").await;

            let body = ctx
                .bulk_ok(json!({"items": [
                    {"code": "E001", "card_id": "01401D0B1D37B660"},
                    {"code": "E002", "card_id": "01:40:1D:0B:1D:37:B6:60"},
                    {"code": "E003", "card_id": "  01401d0b1d37b660  "}
                ]}))
                .await;

            assert_eq!(body["created"], 1, "{body}");
            let skipped = body["skipped"].as_array().unwrap();
            assert_eq!(skipped.len(), 2);
            assert!(skipped
                .iter()
                .all(|s| s["reason"] == "duplicate_in_batch" && s.get("card_id").is_none()));

            let cards = ctx.cards().await;
            assert_eq!(cards.len(), 1, "行数 1: {cards:?}");
            // CHECK 制約 (migration 134) に当たらない = 正規化形で入っている
            assert_eq!(cards[0].0, "01401d0b1d37b660");
        }
    );

    test_case!(
        "3 表記を 3 リクエストに分けても 1 行 (2 回目以降 unchanged)",
        {
            let ctx = setup("Bulk Cards A2").await;
            ctx.employee("取込 太郎", "E001").await;

            for raw in [
                "01401D0B1D37B660",
                "01:40:1d:0b:1d:37:b6:60",
                " 01401d0b1d37b660 ",
            ] {
                ctx.bulk_ok(json!({"items": [{"code": "E001", "card_id": raw}]}))
                    .await;
            }

            let cards = ctx.cards().await;
            assert_eq!(cards.len(), 1, "{cards:?}");
            assert_eq!(cards[0].0, "01401d0b1d37b660");
        }
    );
}

/// 受け入れ 3: 長さ 8 / 14 / 16 は入る。外れた 1 件だけ skip され他は入る。
#[tokio::test]
async fn test_accepts_4_7_8_byte_cards_and_skips_only_the_malformed_one() {
    test_group!("bulk-by-code: カードの長さ");

    test_case!(
        "8 / 14 / 16 桁が入り、長すぎる 1 件だけ invalid_card_id",
        {
            let ctx = setup("Bulk Cards B").await;
            ctx.employee("四 バイト", "E001").await;
            ctx.employee("七 バイト", "E002").await;
            ctx.employee("八 バイト", "E003").await;
            ctx.employee("壊れ 値", "E004").await;

            let body = ctx
                .bulk_ok(json!({"items": [
                    {"code": "E001", "card_id": "01AB23CD"},
                    {"code": "E002", "card_id": "04A1B2C3D4E5F6"},
                    {"code": "E003", "card_id": "01401D0B1D37B660"},
                    {"code": "E004", "card_id": "01401D0B1D37B660112233"}
                ]}))
                .await;

            assert_eq!(
                body["created"], 3,
                "16 桁固定にすると 8/14 桁を落とす: {body}"
            );
            let skipped = body["skipped"].as_array().unwrap();
            assert_eq!(skipped.len(), 1);
            assert_eq!(skipped[0]["index"], 3);
            assert_eq!(skipped[0]["code"], "E004");
            assert_eq!(skipped[0]["reason"], "invalid_card_id");
            assert_eq!(ctx.card_count().await, 3);
        }
    );
}

/// 受け入れ 4: 未知の code は skip、他の行は入る (500 にしない)。
#[tokio::test]
async fn test_unknown_code_is_skipped_without_failing_the_batch() {
    test_group!("bulk-by-code: 未知の社員番号");

    test_case!("employee_not_found で skip、他の行は入る", {
        let ctx = setup("Bulk Cards C").await;
        ctx.employee("居る 人", "E001").await;

        let body = ctx
            .bulk_ok(json!({"items": [
                {"code": "NOBODY", "card_id": "01401d0b1d37b660"},
                {"code": "E001", "card_id": "04a1b2c3d4e5f6"}
            ]}))
            .await;

        assert_eq!(body["created"], 1);
        let skipped = body["skipped"].as_array().unwrap();
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0]["reason"], "employee_not_found");
        assert_eq!(skipped[0]["code"], "NOBODY");
        assert_eq!(ctx.card_count().await, 1);
    });
}

/// 受け入れ 5: 同じ body の 2 回目は 1 行も書かない。
#[tokio::test]
async fn test_replaying_the_same_body_is_all_unchanged() {
    test_group!("bulk-by-code: 再実行");

    test_case!("2 回目は created:0 / updated:0 / unchanged:N", {
        let ctx = setup("Bulk Cards D").await;
        ctx.employee("再実行 太郎", "E001").await;
        ctx.employee("再実行 次郎", "E002").await;
        let body = json!({"items": [
            {"code": "E001", "card_id": "01401d0b1d37b660"},
            {"code": "E002", "card_id": "04a1b2c3d4e5f6"}
        ]});

        let first = ctx.bulk_ok(body.clone()).await;
        assert_eq!(first["created"], 2);

        let second = ctx.bulk_ok(body).await;
        assert_eq!(second["created"], 0, "{second}");
        assert_eq!(second["updated"], 0);
        assert_eq!(second["unchanged"], 2);
        assert_eq!(ctx.card_count().await, 2);
    });
}

/// 受け入れ 6: 別社員のカードは既定で守り、`reassign` のときだけ付け替える。
#[tokio::test]
async fn test_card_owner_conflict_and_reassign() {
    test_group!("bulk-by-code: 持ち主の衝突");

    test_case!("既定は card_owner_conflict、reassign で updated", {
        let ctx = setup("Bulk Cards E").await;
        let alice = ctx.employee("衝突 アリス", "E001").await;
        let bob = ctx.employee("衝突 ボブ", "E002").await;

        ctx.bulk_ok(json!({"items": [{"code": "E001", "card_id": "01401d0b1d37b660"}]}))
            .await;

        let kept = ctx
            .bulk_ok(json!({"items": [{"code": "E002", "card_id": "01401d0b1d37b660"}]}))
            .await;
        assert_eq!(kept["created"], 0);
        assert_eq!(kept["updated"], 0);
        assert_eq!(kept["skipped"][0]["reason"], "card_owner_conflict");
        assert_eq!(
            ctx.cards().await,
            vec![("01401d0b1d37b660".to_string(), alice)]
        );

        let moved = ctx
            .bulk_ok(json!({
                "on_conflict": "reassign",
                "items": [{"code": "E002", "card_id": "01401d0b1d37b660"}]
            }))
            .await;
        assert_eq!(moved["updated"], 1, "{moved}");
        assert!(moved["skipped"].as_array().unwrap().is_empty());
        assert_eq!(
            ctx.cards().await,
            vec![("01401d0b1d37b660".to_string(), bob)]
        );
        assert_eq!(ctx.card_count().await, 1, "付け替えで行は増えない");
    });
}

/// 受け入れ 7: `dry_run` は 1 行も書かず、summary は apply と同じ。
#[tokio::test]
async fn test_dry_run_writes_nothing_and_matches_apply() {
    test_group!("bulk-by-code: dry_run");

    test_case!("前後で件数不変、summary は apply と同一", {
        let ctx = setup("Bulk Cards F").await;
        ctx.employee("試し 太郎", "E001").await;
        ctx.employee("試し 次郎", "E002").await;
        let items = json!([
            {"code": "E001", "card_id": "01401D0B1D37B660"},
            {"code": "NOBODY", "card_id": "04a1b2c3d4e5f6"}
        ]);

        let before = ctx.card_count().await;
        let dry = ctx.bulk_ok(json!({"dry_run": true, "items": items})).await;
        assert_eq!(ctx.card_count().await, before, "dry_run で書いてはいけない");

        let applied = ctx.bulk_ok(json!({"items": items})).await;
        assert_eq!(dry, applied, "判定は同じコードを通る");
        assert_eq!(applied["created"], 1);
        assert_eq!(ctx.card_count().await, before + 1);
    });
}

/// 受け入れ 8: items 0 件 / 501 件は 400。
#[tokio::test]
async fn test_item_count_limits_are_rejected() {
    test_group!("bulk-by-code: items の上限");

    test_case!("0 件も 501 件も 400", {
        let ctx = setup("Bulk Cards G").await;

        assert_eq!(ctx.bulk(json!({"items": []})).await.status(), 400);

        let items: Vec<Value> = (0..501)
            .map(|i| json!({"code": format!("E{i}"), "card_id": "01401d0b1d37b660"}))
            .collect();
        assert_eq!(ctx.bulk(json!({"items": items})).await.status(), 400);
        assert_eq!(ctx.card_count().await, 0);
    });
}

/// 受け入れ 9: 別テナントの同じ card_id は衝突しない。
#[tokio::test]
async fn test_same_card_id_in_two_tenants_does_not_collide() {
    test_group!("bulk-by-code: テナント分離");

    test_case!("同じ card_id を 2 テナントが持てる", {
        let a = setup("Bulk Cards H1").await;
        let b = setup("Bulk Cards H2").await;
        let emp_a = a.employee("A 社 太郎", "E001").await;
        let emp_b = b.employee("B 社 太郎", "E001").await;

        let card = "01401d0b1d37b660";
        assert_eq!(
            a.bulk_ok(json!({"items": [{"code": "E001", "card_id": card}]}))
                .await["created"],
            1
        );
        // 同じ card_id・同じ code でも別テナントなので新規で入る
        // (`idx_timecard_cards_unique` は (tenant_id, card_id))
        assert_eq!(
            b.bulk_ok(json!({"items": [{"code": "E001", "card_id": card}]}))
                .await["created"],
            1
        );

        assert_eq!(a.cards().await, vec![(card.to_string(), emp_a)]);
        assert_eq!(b.cards().await, vec![(card.to_string(), emp_b)]);
    });
}

/// 受け入れ 10: `X-Tenant-ID` 無しは 401。
#[tokio::test]
async fn test_requires_tenant_identity() {
    test_group!("bulk-by-code: 認証");

    test_case!(
        "認証ヘッダー無しは 401 (台帳は誰でも書けない)",
        {
            let ctx = setup("Bulk Cards I").await;
            let res = ctx
                .client
                .put(format!("{}/api/timecard/cards/bulk-by-code", ctx.base_url))
                .json(&json!({"items": [{"code": "E001", "card_id": "01401d0b1d37b660"}]}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 401);
        }
    );
}
