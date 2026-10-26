//! 打刻一覧 (`/api/timecard/punches` と `/punches/csv`) の DB integration テスト。
//!
//! **このファイルが無いと、打刻の読み出し SQL は 1 度も実行されないまま出ます。**
//! mock テスト (`tests/mock_tests/mock_timecard_test.rs`) は repository を丸ごと
//! 差し替えるので SQL を通りません。打刻一覧は #134 で旧・打刻表の読み出しから
//! `hub_measurements` の導出 (CTE + 3 段の COALESCE + 2 本の LEFT JOIN) に変わり、
//! **SQL 側だけで壊れうる範囲が一気に増えた**ので、実 DB で固定します。

#[macro_use]
mod common;

use serde_json::{json, Value};
use uuid::Uuid;

/// 端末の打刻を ingest 経路 (cf-alc-recorder → 内部 proxy) で入れる。
/// 直接 INSERT せず本番と同じ口を通すのは、**ingest 時の凍結
/// (`freeze_employee_id`) と読み出しの噛み合わせ**まで含めて固定するため。
async fn post_timecard(
    client: &reqwest::Client,
    base_url: &str,
    tenant_id: Uuid,
    seq: i64,
    card_id: &str,
    recorded_at_ms: Option<i64>,
) -> reqwest::Response {
    let mut item = json!({
        "device_id": "timecard-dev-1",
        "kind": "timecard",
        "seq": seq,
        "payload": { "card_id": card_id, "card_kind": "felica_idm" }
    });
    if let Some(ms) = recorded_at_ms {
        item["recorded_at_ms"] = json!(ms);
    }
    client
        .post(format!("{base_url}/api/hub/measurements"))
        .header(
            "X-Internal-Shared-Secret",
            common::TEST_INTERNAL_SHARED_SECRET,
        )
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(&json!([item]))
        .send()
        .await
        .unwrap()
}

async fn list_punches(client: &reqwest::Client, base_url: &str, auth: &str) -> Value {
    let res = client
        .get(format!("{base_url}/api/timecard/punches"))
        .header("Authorization", auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /api/timecard/punches");
    res.json().await.unwrap()
}

#[tokio::test]
async fn test_punches_are_derived_from_hub_measurements() {
    test_group!("timecard punches (derived)");

    test_case!(
        "登録カード → 社員が解決され、未登録カードも行として出る",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Punch Derive A").await;
            let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "打刻 太郎", "E001").await;
            let employee_id = emp["id"].as_str().unwrap().to_string();

            // カード登録は**大文字**で投げる (端末が送る IDm の生形)。
            // サーバが小文字へ正規化して保存する (migration 134)
            let res = client
                .post(format!("{base_url}/api/timecard/cards"))
                .header("Authorization", &auth)
                .json(&json!({ "employee_id": employee_id, "card_id": "01401D0B1D37B660" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);

            // 登録済みカードのタップ + 未登録カードのタップ
            assert_eq!(
                post_timecard(&client, &base_url, tenant, 1, "01401D0B1D37B660", None)
                    .await
                    .status(),
                201
            );
            assert_eq!(
                post_timecard(&client, &base_url, tenant, 2, "DEADBEEFDEADBEEF", None)
                    .await
                    .status(),
                201
            );

            let body = list_punches(&client, &base_url, &auth).await;
            let punches = body["punches"].as_array().unwrap();
            assert_eq!(punches.len(), 2, "2 タップとも一覧に出る: {body}");
            assert_eq!(body["total"], 2);

            // 登録済みカードは社員に解決されている
            let resolved = punches
                .iter()
                .find(|p| p["employee_id"].as_str() == Some(employee_id.as_str()))
                .unwrap_or_else(|| panic!("解決済みの打刻が無い: {body}"));
            assert_eq!(resolved["employee_name"], "打刻 太郎");
            // 端末は device_name に入る (device_id は UUID FK なので常に null)
            assert_eq!(resolved["device_name"], "timecard-dev-1");
            assert!(resolved["device_id"].is_null());

            // 未登録カードは employee_id が null。**行ごと落とさない** —
            // 落とすと「タップしたのに履歴に出ない」で登録漏れに気付けなくなる
            let unresolved = punches
                .iter()
                .find(|p| p["employee_id"].is_null())
                .unwrap_or_else(|| panic!("未解決の打刻が無い: {body}"));
            assert!(unresolved["employee_name"].is_null());
            // **どのカードが未登録かを出せること。** これが無いと画面は
            // 「未登録カード」としか言えず、登録しに行けない
            assert_eq!(unresolved["card_id"], "DEADBEEFDEADBEEF");
        }
    );
}

#[tokio::test]
async fn test_punched_at_uses_terminal_time_not_arrival_time() {
    test_group!("timecard punches (時刻)");

    test_case!("recorded_at があればそれ、無ければ created_at", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant = common::create_test_tenant(state.pool(), "Punch Derive B").await;
        let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
        let client = reqwest::Client::new();

        // 端末計時あり (回線断のあいだ溜めて後から送られた打刻を模す)
        let tapped_ms = 1_752_300_000_000i64; // 2025-07-12T06:00:00Z
        post_timecard(&client, &base_url, tenant, 1, "AAAA", Some(tapped_ms)).await;
        // 端末計時なし (時計未同期 → recorded_at が NULL)
        post_timecard(&client, &base_url, tenant, 2, "BBBB", None).await;

        let body = list_punches(&client, &base_url, &auth).await;
        let punches = body["punches"].as_array().unwrap();
        assert_eq!(punches.len(), 2, "{body}");

        // 端末計時のある行は**届いた時刻ではなくタップ時刻**で出る
        assert!(
            punches.iter().any(|p| p["punched_at"]
                .as_str()
                .unwrap()
                .starts_with("2025-07-12T06:00:00")),
            "recorded_at がそのまま punched_at にならない: {body}"
        );
        // recorded_at が NULL の行も落ちない (created_at に倒れる)
        assert_eq!(
            punches
                .iter()
                .filter(|p| !p["punched_at"].is_null())
                .count(),
            2,
            "punched_at が NULL の行がある: {body}"
        );
    });
}

#[tokio::test]
async fn test_punches_csv_keeps_unresolved_rows() {
    test_group!("timecard punches (CSV)");

    test_case!(
        "未登録カードも行として出る (社員名は空欄)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Punch Derive C").await;
            let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
            let client = reqwest::Client::new();

            post_timecard(&client, &base_url, tenant, 1, "DEADBEEFDEADBEEF", None).await;

            let res = client
                .get(format!("{base_url}/api/timecard/punches/csv"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let bytes = res.bytes().await.unwrap();
            let csv = std::str::from_utf8(&bytes[3..]).unwrap();

            // ヘッダ + 1 行。端末 ID は残る
            assert_eq!(
                csv.lines().filter(|l| !l.trim().is_empty()).count(),
                2,
                "{csv}"
            );
            assert!(csv.contains("timecard-dev-1"), "{csv}");
        }
    );
}

#[tokio::test]
async fn test_punches_are_tenant_isolated() {
    test_group!("timecard punches (テナント分離)");

    test_case!("別テナントの打刻は見えない", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_a = common::create_test_tenant(state.pool(), "Punch Derive D1").await;
        let tenant_b = common::create_test_tenant(state.pool(), "Punch Derive D2").await;
        let client = reqwest::Client::new();

        post_timecard(&client, &base_url, tenant_a, 1, "AAAA", None).await;

        let auth_b = format!("Bearer {}", common::create_test_jwt(tenant_b, "admin"));
        let body = list_punches(&client, &base_url, &auth_b).await;
        assert_eq!(body["punches"].as_array().unwrap().len(), 0, "{body}");
        assert_eq!(body["total"], 0);
    });
}

/// 点呼 (`kind=license`) も同じ一覧に並ぶが、**`kind` で区別できること**。
/// 列が無いと画面も CSV も点呼を打刻として扱ってしまう
#[tokio::test]
async fn test_punches_expose_kind_to_separate_tenko_from_timecard() {
    test_group!("timecard punches (区分)");

    test_case!("timecard と license が kind で見分けられる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant = common::create_test_tenant(state.pool(), "Punch Kind").await;
        let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
        let client = reqwest::Client::new();

        // 打刻機のタップ
        post_timecard(&client, &base_url, tenant, 1, "AAAA", None).await;
        // 点呼の免許証読み取り (同じ表に別 kind で入る)
        let lic = json!([{
            "device_id": "cores3-1",
            "kind": "license",
            "seq": 1,
            "payload": { "nfc_id": "2023060920280513", "issue": "20230609", "expiry": "20280513" }
        }]);
        let res = client
            .post(format!("{base_url}/api/hub/measurements"))
            .header(
                "X-Internal-Shared-Secret",
                common::TEST_INTERNAL_SHARED_SECRET,
            )
            .header("X-Tenant-ID", tenant.to_string())
            .json(&lic)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        let body = list_punches(&client, &base_url, &auth).await;
        let punches = body["punches"].as_array().unwrap();
        assert_eq!(punches.len(), 2, "{body}");
        assert_eq!(
            punches.iter().filter(|p| p["kind"] == "timecard").count(),
            1,
            "{body}"
        );
        assert_eq!(
            punches.iter().filter(|p| p["kind"] == "license").count(),
            1,
            "{body}"
        );

        // CSV にも区分列が出る
        let res = client
            .get(format!("{base_url}/api/timecard/punches/csv"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let bytes = res.bytes().await.unwrap();
        let csv = std::str::from_utf8(&bytes[3..]).unwrap();
        assert!(csv.lines().next().unwrap().contains("区分"), "{csv}");
        assert!(csv.contains(",打刻,"), "{csv}");
        assert!(csv.contains(",点呼,"), "{csv}");
    });
}

/// 端末が送る `payload.card_kind` を一覧と CSV まで通す (Refs ippoan/rust-alc-api#644)。
///
/// **ファームは打刻の `kind` を常に `timecard` で送る**ので、免許証か他の IC カードかは
/// `payload.card_kind` にしか入っていない。`PUNCHES_CTE` がこれを SELECT していないと
/// API が返さず、画面にも CSV にも届かない (データは入っているのに出ない状態だった)。
///
/// `kind` (打刻 / 点呼) とは**別の軸**なので、両方の列が同時に出ることを固定する。
async fn post_hub_item(
    client: &reqwest::Client,
    base_url: &str,
    tenant_id: Uuid,
    seq: i64,
    kind: &str,
    payload: Value,
) -> reqwest::Response {
    client
        .post(format!("{base_url}/api/hub/measurements"))
        .header(
            "X-Internal-Shared-Secret",
            common::TEST_INTERNAL_SHARED_SECRET,
        )
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(&json!([{
            "device_id": "timecard-dev-1",
            "kind": kind,
            "seq": seq,
            "payload": payload,
        }]))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_punches_expose_card_kind() {
    test_group!("timecard punches (カード種別)");

    test_case!(
        "免許証 / IC カード / 点呼をカード種別で見分けられる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Punch Card Kind").await;
            let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "種別 太郎", "E030").await;
            let employee_id = emp["id"].as_str().unwrap().to_string();
            let res = client
                .post(format!("{base_url}/api/timecard/cards"))
                .header("Authorization", &auth)
                .json(&json!({ "employee_id": employee_id, "card_id": "FEEDFACEFEEDFACE" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);

            // ① FeliCa (社員証など) のタップ
            assert_eq!(
                post_hub_item(
                    &client,
                    &base_url,
                    tenant,
                    1,
                    "timecard",
                    json!({ "card_id": "FEEDFACEFEEDFACE", "card_kind": "felica_idm" }),
                )
                .await
                .status(),
                201
            );
            // ② 免許証のタップ。**kind は timecard のまま** (ファームが常にそう送る)
            assert_eq!(
                post_hub_item(
                    &client,
                    &base_url,
                    tenant,
                    2,
                    "timecard",
                    json!({ "card_id": "LICENSECARD01", "card_kind": "license" }),
                )
                .await
                .status(),
                201
            );
            // ③ 点呼開始時の免許証読み取り (kind = license)。payload に card_kind は
            //    載らないので、CTE の CASE が免許証として補う
            assert_eq!(
                post_hub_item(
                    &client,
                    &base_url,
                    tenant,
                    3,
                    "license",
                    json!({ "nfc_id": "TENKOLICENSE01" }),
                )
                .await
                .status(),
                201
            );
            let body = list_punches(&client, &base_url, &auth).await;
            let punches = body["punches"].as_array().unwrap();
            assert_eq!(punches.len(), 3, "3 行とも一覧に出る: {body}");

            let card_kind_of = |card: &str| -> Value {
                punches
                    .iter()
                    .find(|p| p["card_id"].as_str() == Some(card))
                    .unwrap_or_else(|| panic!("{card} の行が無い: {body}"))["card_kind"]
                    .clone()
            };

            assert_eq!(card_kind_of("FEEDFACEFEEDFACE"), "felica_idm");
            assert_eq!(card_kind_of("LICENSECARD01"), "license");
            // 点呼の行も「免許証」として出る
            assert_eq!(card_kind_of("TENKOLICENSE01"), "license");

            // **`kind` は別の軸。点呼の行だけが license** —
            // card_kind を kind に流用していないことの担保
            assert_eq!(
                punches.iter().filter(|p| p["kind"] == "license").count(),
                1,
                "{body}"
            );

            // --- CSV 側 ---
            let res = client
                .get(format!("{base_url}/api/timecard/punches/csv"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let bytes = res.bytes().await.unwrap();
            let csv = std::str::from_utf8(&bytes[3..]).unwrap();

            let header = csv.lines().next().unwrap();
            // **「区分」列は消さない。「カード」列を足すだけ**
            assert!(header.contains("区分"), "{csv}");
            assert!(header.contains("カード"), "{csv}");

            // 免許証の行は「免許証」、FeliCa は「ICカード」
            assert_eq!(
                csv.lines().filter(|l| l.contains(",免許証,")).count(),
                2,
                "免許証のタップと点呼の 2 行が免許証になっていない: {csv}"
            );
            assert!(csv.contains(",ICカード,"), "{csv}");
        }
    );
}

// ---------------------------------------------------------------------------
// 免許証 1 回のタッチ = 打刻の一覧では 1 行 (Refs ippoan/alc-app#387)
//
// 免許証を端末にタッチすると `kind = 'timecard'` の行が入り、そのまま点呼を始めると
// `kind = 'license'` の行がもう 1 行入る。`hub_measurements` の行は両方残したまま、
// 一覧・件数・CSV・当日の一覧からは、対の打刻が在る license の行だけを外す。
// ---------------------------------------------------------------------------

/// 対のテストの基準時刻 (2025-07-12T06:00:00Z)。各行はここからの秒差で置く
const FOLD_BASE_EPOCH: i64 = 1_752_300_000;

/// `hub_measurements` に置く 1 行
struct HubRow {
    device: &'static str,
    kind: &'static str,
    /// `timecard` は `payload.card_id`、`license` は `payload.nfc_id` に入る (ファームと同じ形)。
    /// None はその項目を載せない
    card: Option<&'static str>,
    /// `FOLD_BASE_EPOCH` からの秒差
    at: i64,
    is_dev: bool,
}

impl HubRow {
    fn timecard(card: &'static str, at: i64) -> Self {
        Self {
            device: "cores3-1",
            kind: "timecard",
            card: Some(card),
            at,
            is_dev: false,
        }
    }

    fn license(card: &'static str, at: i64) -> Self {
        Self {
            kind: "license",
            ..Self::timecard(card, at)
        }
    }

    fn payload(&self) -> Value {
        match (self.kind, self.card) {
            ("timecard", Some(card)) => json!({ "card_id": card, "card_kind": "license" }),
            ("timecard", None) => json!({ "card_kind": "license" }),
            (_, Some(card)) => json!({ "type": "license", "nfc_id": card }),
            (_, None) => json!({ "type": "license" }),
        }
    }
}

/// 行を直接入れて id を返す。時刻と dev の軸 (`is_dev`) を行ごとに決めるため、ingest の口は通さない
/// (`pool` は RLS を素通りする側)。`recorded_at` は SQL の式で渡す
async fn insert_hub_row_at(
    pool: &sqlx::PgPool,
    tenant: Uuid,
    seq: i64,
    row: &HubRow,
    recorded_at_sql: &str,
) -> String {
    let id: Uuid = sqlx::query_scalar(&format!(
        r#"INSERT INTO hub_measurements
               (tenant_id, device_id, kind, payload, seq, recorded_at, is_dev)
           VALUES ($1, $2, $3, $4, $5, {recorded_at_sql}, $6)
           RETURNING id"#
    ))
    .bind(tenant)
    .bind(row.device)
    .bind(row.kind)
    .bind(row.payload())
    .bind(seq)
    .bind(row.is_dev)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("hub_measurements に行を入れられない: {e}"));
    id.to_string()
}

/// `rows` を順に入れて id を返す (seq は並び順)
async fn insert_hub_rows(pool: &sqlx::PgPool, tenant: Uuid, rows: &[HubRow]) -> Vec<String> {
    let mut ids = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let at = format!("to_timestamp({})", FOLD_BASE_EPOCH + row.at);
        ids.push(insert_hub_row_at(pool, tenant, i as i64, row, &at).await);
    }
    ids
}

/// 一覧・件数・CSV の 3 つの口から見えた行
struct Listed {
    /// 一覧の (kind, id)。kind の順
    rows: Vec<(String, String)>,
    total: i64,
    /// CSV のデータ行の数 (ヘッダを除く)
    csv_rows: usize,
    /// CSV の「区分」が点呼の行の数
    csv_tenko_rows: usize,
}

impl Listed {
    fn kinds(&self) -> Vec<&str> {
        self.rows.iter().map(|(kind, _)| kind.as_str()).collect()
    }

    fn ids(&self) -> Vec<&str> {
        self.rows.iter().map(|(_, id)| id.as_str()).collect()
    }
}

/// 一覧と CSV を引く。`dev` が true なら dev端末の印を付ける
async fn listed(client: &reqwest::Client, base_url: &str, auth: &str, dev: bool) -> Listed {
    let get = |path: &str| {
        let rb = client
            .get(format!("{base_url}{path}"))
            .header("Authorization", auth);
        if dev {
            rb.header("X-Device-Dev", "1")
        } else {
            rb
        }
    };

    let res = get("/api/timecard/punches").send().await.unwrap();
    assert_eq!(res.status(), 200, "GET /api/timecard/punches");
    let body: Value = res.json().await.unwrap();
    let mut rows: Vec<(String, String)> = body["punches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["kind"].as_str().unwrap().to_string(),
                p["id"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    rows.sort();

    let res = get("/api/timecard/punches/csv").send().await.unwrap();
    assert_eq!(res.status(), 200, "GET /api/timecard/punches/csv");
    let bytes = res.bytes().await.unwrap();
    let csv = std::str::from_utf8(&bytes[3..]).unwrap();
    let lines: Vec<&str> = csv.lines().filter(|l| !l.trim().is_empty()).collect();

    Listed {
        rows,
        total: body["total"].as_i64().unwrap(),
        csv_rows: lines.len() - 1,
        csv_tenko_rows: lines.iter().filter(|l| l.contains(",点呼,")).count(),
    }
}

#[tokio::test]
async fn test_license_row_is_folded_into_its_timecard_punch() {
    test_group!("timecard punches (免許証のタッチは 1 行)");

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    // (題, 入れる行, 一覧に出る kind)。license の行はどれも基準時刻 (秒差 0) に置く
    let both = vec!["license", "timecard"];
    let cases: Vec<(&str, Vec<HubRow>, Vec<&str>)> = vec![
        (
            "対の 2 行 (同じ機体・同じカード・6 秒差) → 打刻の 1 行",
            vec![HubRow::timecard("LIC-A", -6), HubRow::license("LIC-A", 0)],
            vec!["timecard"],
        ),
        (
            "打刻だけの回 → 1 行",
            vec![HubRow::timecard("LIC-A", -6)],
            vec!["timecard"],
        ),
        (
            "対の無い license の行 → 出る",
            vec![HubRow::license("LIC-A", 0)],
            vec!["license"],
        ),
        (
            "境界: ちょうど 30 秒前の打刻 → 外す",
            vec![HubRow::timecard("LIC-A", -30), HubRow::license("LIC-A", 0)],
            vec!["timecard"],
        ),
        (
            "境界: 同時刻の打刻 → 外す",
            vec![HubRow::timecard("LIC-A", 0), HubRow::license("LIC-A", 0)],
            vec!["timecard"],
        ),
        (
            "31 秒前の打刻 → 外さない",
            vec![HubRow::timecard("LIC-A", -31), HubRow::license("LIC-A", 0)],
            both.clone(),
        ),
        (
            "license の行より後の打刻 → 外さない",
            vec![HubRow::timecard("LIC-A", 1), HubRow::license("LIC-A", 0)],
            both.clone(),
        ),
        (
            "別の機体の打刻 → 外さない",
            vec![
                HubRow {
                    device: "cores3-2",
                    ..HubRow::timecard("LIC-A", -6)
                },
                HubRow::license("LIC-A", 0),
            ],
            both.clone(),
        ),
        (
            "別のカードの打刻 → 外さない",
            vec![HubRow::timecard("LIC-B", -6), HubRow::license("LIC-A", 0)],
            both.clone(),
        ),
        (
            "カードの値がどちらにも無い 2 行 → 対にしない",
            vec![
                HubRow {
                    card: None,
                    ..HubRow::timecard("", -6)
                },
                HubRow {
                    card: None,
                    ..HubRow::license("", 0)
                },
            ],
            both.clone(),
        ),
        // ↓ 2 件は RLS を素通りする接続 (このテストの pool) だから両方の軸が見える。
        //   対の判定が is_dev を自分で見ていないと、license の行が消える
        (
            "dev の機体の打刻は、本番の license の行を外さない",
            vec![
                HubRow {
                    is_dev: true,
                    ..HubRow::timecard("LIC-A", -6)
                },
                HubRow::license("LIC-A", 0),
            ],
            both.clone(),
        ),
        (
            "本番の打刻は、dev の機体の license の行を外さない",
            vec![
                HubRow::timecard("LIC-A", -6),
                HubRow {
                    is_dev: true,
                    ..HubRow::license("LIC-A", 0)
                },
            ],
            both.clone(),
        ),
    ];

    for (i, (label, rows, expected)) in cases.iter().enumerate() {
        test_case!(label, {
            let tenant = common::create_test_tenant(state.pool(), &format!("Punch Fold {i}")).await;
            let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
            let ids = insert_hub_rows(state.pool(), tenant, rows).await;

            let got = listed(&client, &base_url, &auth, false).await;
            assert_eq!(&got.kinds(), expected, "一覧: {label}");
            assert_eq!(got.total, expected.len() as i64, "total: {label}");
            assert_eq!(got.csv_rows, expected.len(), "CSV の行数: {label}");
            assert_eq!(
                got.csv_tenko_rows,
                expected.iter().filter(|k| **k == "license").count(),
                "CSV の点呼の行: {label}"
            );
            // 残る打刻の行は、入れた timecard の行そのもの (行を作り替えていない)
            for (row, id) in rows.iter().zip(&ids) {
                assert_eq!(
                    got.ids().contains(&id.as_str()),
                    row.kind == "timecard" || expected.contains(&"license"),
                    "{} の行: {label}",
                    row.kind
                );
            }

            // 行そのものは消していない (導出だけを変えている)
            let stored: i64 =
                sqlx::query_scalar("SELECT count(*) FROM hub_measurements WHERE tenant_id = $1")
                    .bind(tenant)
                    .fetch_one(state.pool())
                    .await
                    .unwrap();
            assert_eq!(
                stored,
                rows.len() as i64,
                "hub_measurements の行数: {label}"
            );
        });
    }
}

#[tokio::test]
async fn test_license_row_is_not_folded_across_tenants() {
    test_group!("timecard punches (免許証のタッチは 1 行: テナント)");

    test_case!("別テナントの打刻の行とは対にならない", {
        // RLS を素通りする接続なので、対の判定が tenant_id を自分で見ていないと
        // テナント A の license の行が、テナント B の打刻のせいで消える
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let client = reqwest::Client::new();
        let tenant_a = common::create_test_tenant(state.pool(), "Punch Fold T1").await;
        let tenant_b = common::create_test_tenant(state.pool(), "Punch Fold T2").await;

        insert_hub_rows(state.pool(), tenant_a, &[HubRow::license("LIC-A", 0)]).await;
        insert_hub_rows(state.pool(), tenant_b, &[HubRow::timecard("LIC-A", -6)]).await;

        for (tenant, expected) in [(tenant_a, "license"), (tenant_b, "timecard")] {
            let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
            let got = listed(&client, &base_url, &auth, false).await;
            assert_eq!(got.kinds(), vec![expected]);
            assert_eq!(got.total, 1);
            assert_eq!(got.csv_rows, 1);
        }
    });
}

/// 実行用ロール `alc_api_rt` (RLS が効く) でも同じ結果になること。
/// 接続の作り方は `tests/device_dev_axis_test.rs` と同じ
#[tokio::test]
async fn test_license_row_fold_as_app_role() {
    test_group!("timecard punches (免許証のタッチは 1 行: 実行用ロール)");

    let admin_state = common::setup_app_state().await;
    let admin = admin_state.pool().clone();
    let base_url =
        common::spawn_test_server(common::setup_app_state_as_app_role(5, true).await).await;
    let tenant = common::create_test_tenant(&admin, "Punch Fold App Role").await;
    let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
    let client = reqwest::Client::new();

    // 本番の軸: カード A の対 + カード B の打刻だけ。dev の軸: カード B の license だけ
    // (同じ機体・同じカード・6 秒差だが、軸が違うので対ではない)
    let ids = insert_hub_rows(
        &admin,
        tenant,
        &[
            HubRow::timecard("LIC-A", -6),
            HubRow::license("LIC-A", 0),
            HubRow::timecard("LIC-B", -6),
            HubRow {
                is_dev: true,
                ..HubRow::license("LIC-B", 0)
            },
        ],
    )
    .await;

    test_case!("本番の軸: 対の license の行だけが外れる", {
        let got = listed(&client, &base_url, &auth, false).await;
        assert_eq!(got.kinds(), vec!["timecard", "timecard"]);
        assert_eq!(got.total, 2);
        assert_eq!(got.csv_rows, 2);
        assert_eq!(got.csv_tenko_rows, 0);
        assert!(!got.ids().contains(&ids[1].as_str()), "対の license の行");
    });

    test_case!(
        "dev の軸: 本番の打刻は dev の license の行を外さない",
        {
            let got = listed(&client, &base_url, &auth, true).await;
            assert_eq!(got.ids(), vec![ids[3].as_str()]);
            assert_eq!(got.kinds(), vec!["license"]);
            assert_eq!(got.total, 1);
            assert_eq!(got.csv_tenko_rows, 1);
        }
    );
}

// ============================================================
// 打刻に結ばれた測定 (Refs ippoan/alc-app#436)
//
// 画面は打刻の payload に、その時点で始まっている測定の id を `measurement_id` として
// 載せる。一覧と CSV はそれを測定に結び、値・判定・点呼として記録されたかを返す。
// 結ぶのは tenant・is_dev・社員・id が全部そろったときだけ。
// ============================================================

/// 測定を直接入れて id を返す (`pool` は RLS を素通りする側。is_dev を行ごとに決めるため)
async fn insert_measurement(
    pool: &sqlx::PgPool,
    tenant: Uuid,
    employee: Uuid,
    alcohol_level: f64,
    result: &str,
    is_dev: bool,
) -> Uuid {
    sqlx::query_scalar(
        r#"INSERT INTO measurements (tenant_id, employee_id, alcohol_level, result, is_dev)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id"#,
    )
    .bind(tenant)
    .bind(employee)
    .bind(alcohol_level)
    .bind(result)
    .bind(is_dev)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("measurements に行を入れられない: {e}"))
}

/// 打刻の行を payload ごと直接入れて id を返す。`at` は `FOLD_BASE_EPOCH` からの秒差
async fn insert_punch_with_payload(
    pool: &sqlx::PgPool,
    tenant: Uuid,
    seq: i64,
    at: i64,
    payload: Value,
) -> String {
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO hub_measurements
               (tenant_id, device_id, kind, payload, seq, recorded_at, is_dev)
           VALUES ($1, 'cores3-m', 'timecard', $2, $3, to_timestamp($4), false)
           RETURNING id"#,
    )
    .bind(tenant)
    .bind(payload)
    .bind(seq)
    .bind((FOLD_BASE_EPOCH + at) as f64)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("hub_measurements に行を入れられない: {e}"));
    id.to_string()
}

#[tokio::test]
async fn test_punches_join_the_measurement_in_payload() {
    test_group!("timecard punches (payload の measurement_id で測定を結ぶ)");

    let state = common::setup_app_state().await;
    let pool = state.pool().clone();
    let base_url = common::spawn_test_server(state).await;
    let tenant = common::create_test_tenant(&pool, "Punch Measurement A").await;
    let other_tenant = common::create_test_tenant(&pool, "Punch Measurement B").await;
    let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
    let other_auth = format!("Bearer {}", common::create_test_jwt(other_tenant, "admin"));
    let client = reqwest::Client::new();

    let emp_id = |v: &Value| Uuid::parse_str(v["id"].as_str().unwrap()).unwrap();
    let emp =
        emp_id(&common::create_test_employee(&client, &base_url, &auth, "測定 太郎", "M001").await);
    let other_emp =
        emp_id(&common::create_test_employee(&client, &base_url, &auth, "測定 次郎", "M002").await);
    let other_tenant_emp = emp_id(
        &common::create_test_employee(&client, &base_url, &other_auth, "別 三郎", "M003").await,
    );

    let m_fail = insert_measurement(&pool, tenant, emp, 0.15, "fail", false).await;
    let m_pass = insert_measurement(&pool, tenant, emp, 0.0, "pass", false).await;
    let m_dev = insert_measurement(&pool, tenant, emp, 0.2, "fail", true).await;
    let m_other_emp = insert_measurement(&pool, tenant, other_emp, 0.1, "fail", false).await;
    let m_other_tenant =
        insert_measurement(&pool, other_tenant, other_tenant_emp, 0.1, "fail", false).await;

    // m_fail だけ点呼として記録された
    sqlx::query(
        "INSERT INTO tenko_sessions (tenant_id, employee_id, tenko_type, status, measurement_id, is_dev)
         VALUES ($1, $2, 'normal', 'completed', $3, false)",
    )
    .bind(tenant)
    .bind(emp)
    .bind(m_fail)
    .execute(&pool)
    .await
    .unwrap();

    // 打刻の社員は payload の employee_id で決まる (CTE の COALESCE の 1 段目)
    let payload = |measurement_id: Option<String>| {
        let mut p = json!({ "employee_id": emp.to_string(), "card_kind": "license" });
        if let Some(m) = measurement_id {
            p["measurement_id"] = json!(m);
        }
        p
    };
    let cases: Vec<(&str, Option<String>)> = vec![
        ("結ばれる (点呼記録あり)", Some(m_fail.to_string())),
        (
            "結ばれる (大文字の UUID)",
            Some(m_pass.to_string().to_uppercase()),
        ),
        ("payload に measurement_id が無い", None),
        ("形の違う値", Some("abc".to_string())),
        ("別テナントの測定", Some(m_other_tenant.to_string())),
        ("is_dev が違う測定", Some(m_dev.to_string())),
        ("別の社員の測定", Some(m_other_emp.to_string())),
    ];
    let mut ids = Vec::new();
    for (i, (_, m)) in cases.iter().enumerate() {
        ids.push(
            insert_punch_with_payload(&pool, tenant, i as i64, i as i64, payload(m.clone())).await,
        );
    }

    let body = list_punches(&client, &base_url, &auth).await;
    let punches = body["punches"].as_array().unwrap();
    assert_eq!(punches.len(), cases.len(), "{body}");
    let by_id = |id: &str| -> &Value {
        punches
            .iter()
            .find(|p| p["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("打刻 {id} が一覧に無い: {body}"))
    };

    test_case!(
        "同じ tenant・is_dev・社員の測定 → 値・判定・点呼記録が出る",
        {
            let p = by_id(&ids[0]);
            assert_eq!(p["measurement_id"], json!(m_fail.to_string()), "{p}");
            assert_eq!(p["alcohol_level"], json!(0.15), "{p}");
            assert_eq!(p["result"], json!("fail"), "{p}");
            assert_eq!(p["tenko_recorded"], json!(true), "{p}");
        }
    );

    test_case!(
        "大文字の UUID でも結ばれる・点呼に記録されていなければ false",
        {
            let p = by_id(&ids[1]);
            assert_eq!(p["measurement_id"], json!(m_pass.to_string()), "{p}");
            assert_eq!(p["alcohol_level"], json!(0.0), "{p}");
            assert_eq!(p["result"], json!("pass"), "{p}");
            assert_eq!(p["tenko_recorded"], json!(false), "{p}");
        }
    );

    for (i, (label, _)) in cases.iter().enumerate().skip(2) {
        test_case!(format!("{label} → 4 欄とも null"), {
            let p = by_id(&ids[i]);
            for key in [
                "measurement_id",
                "alcohol_level",
                "result",
                "tenko_recorded",
            ] {
                assert!(p[key].is_null(), "{label}: {key} が null でない: {p}");
            }
        });
    }

    test_case!("CSV にも同じ 4 列が出る", {
        let res = client
            .get(format!("{base_url}/api/timecard/punches/csv"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let bytes = res.bytes().await.unwrap();
        let csv = std::str::from_utf8(&bytes[3..]).unwrap();
        let line = |id: &str| -> &str {
            csv.lines()
                .find(|l| l.starts_with(id))
                .unwrap_or_else(|| panic!("CSV に {id} が無い: {csv}"))
        };
        assert!(
            line(&ids[0]).ends_with(&format!(",{m_fail},0.15,fail,済")),
            "{csv}"
        );
        assert!(
            line(&ids[1]).ends_with(&format!(",{m_pass},0,pass,未")),
            "{csv}"
        );
        assert!(line(&ids[3]).ends_with(",,,,"), "{csv}");
    });
}

/// 打刻の「端末」列 (device_name) は、hub の ID の行を結び付いた機体の鍵の ID に畳む
/// (Refs ippoan/alc-cam#2)。結び付き = 鍵の ID で入った行の via_device_id。
/// 結び付きの無い hub の ID はそのまま出る。
#[tokio::test]
async fn test_punch_device_name_folds_hub_id_into_key_id() {
    test_group!("timecard punches (machine key id)");

    test_case!(
        "hub の ID の打刻は結び付いた鍵の ID で出る",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Punch Key Id").await;
            let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
            let client = reqwest::Client::new();
            let key = "alarm:0123456789abcdef";

            // 古いファームの打刻 (hub の ID = timecard-dev-1) と、結び付きの無い別の hub の打刻
            assert_eq!(
                post_timecard(&client, &base_url, tenant, 1, "AAAA0000AAAA0000", None)
                    .await
                    .status(),
                201
            );
            let other = json!([{
                "device_id": "timecard-dev-9",
                "kind": "timecard",
                "seq": 1,
                "payload": { "card_id": "BBBB0000BBBB0000", "card_kind": "felica_idm" }
            }]);
            let post = |body: Value| {
                client
                    .post(format!("{base_url}/api/hub/measurements"))
                    .header(
                        "X-Internal-Shared-Secret",
                        common::TEST_INTERNAL_SHARED_SECRET,
                    )
                    .header("X-Tenant-ID", tenant.to_string())
                    .json(&body)
                    .send()
            };
            assert_eq!(post(other).await.unwrap().status(), 201);

            // 新しいファームの打刻: 鍵の ID で記録し、接続の hub の ID を via_device_id に添える
            let keyed = json!([{
                "device_id": key,
                "via_device_id": "timecard-dev-1",
                "kind": "timecard",
                "seq": 2,
                "payload": { "card_id": "CCCC0000CCCC0000", "card_kind": "felica_idm" }
            }]);
            assert_eq!(post(keyed).await.unwrap().status(), 201);

            let body = list_punches(&client, &base_url, &auth).await;
            let name_of = |card: &str| -> Value {
                body["punches"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["card_id"] == card)
                    .unwrap_or_else(|| panic!("{card} の打刻が無い: {body}"))["device_name"]
                    .clone()
            };
            assert_eq!(
                name_of("AAAA0000AAAA0000"),
                json!(key),
                "hub の打刻は鍵の ID に畳む"
            );
            assert_eq!(name_of("CCCC0000CCCC0000"), json!(key));
            assert_eq!(
                name_of("BBBB0000BBBB0000"),
                json!("timecard-dev-9"),
                "結び付きの無い hub はそのまま"
            );
        }
    );
}
