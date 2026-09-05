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

/// ブラウザ版 punch の応答に付く「当日の打刻」は **JST で切る**。
///
/// `CURRENT_DATE` はサーバ TZ (Cloud Run は UTC) の日付なので、JST 09:00〜24:00 の
/// あいだは閾値が JST 09:00 になり、**00:00〜09:00 JST の打刻が「今日」から落ちる**
/// (早朝の出勤打刻がまさにその時間帯)。逆に JST 00:00〜09:00 は前日ぶんを拾う。
#[tokio::test]
async fn test_today_punches_use_jst_day_boundary() {
    test_group!("timecard punches (当日一覧の日付境界)");

    test_case!("JST の 0 時で切る", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant = common::create_test_tenant(state.pool(), "Punch Today JST").await;
        let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
        let client = reqwest::Client::new();

        let emp =
            common::create_test_employee(&client, &base_url, &auth, "当日 花子", "E010").await;
        let employee_id = emp["id"].as_str().unwrap().to_string();
        let res = client
            .post(format!("{base_url}/api/timecard/cards"))
            .header("Authorization", &auth)
            .json(&json!({ "employee_id": employee_id, "card_id": "CAFEBABECAFEBABE" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        // **2 行仕込むのは、旧実装 (CURRENT_DATE) が時間帯によって過剰にも過少にも
        // 外れるから。** 片方だけだと「いま何時か」でテストが素通りする:
        //   JST 09:00〜24:00 … 閾値が JST 09:00 になり、今日 00:30 の行が落ちる (過少)
        //   JST 00:00〜09:00 … 閾値が前日 09:00 になり、昨日 23:30 の行を拾う (過剰)
        // 両方入れておけば、どちらの時間帯でも件数が 2 からずれて落ちる。
        for (i, (label, expr)) in [
            // JST の今日 00:30 → 含まれるべき
            ("today", "(d + interval '30 minutes')"),
            // JST の昨日 23:30 → 含まれてはいけない
            ("yesterday", "(d - interval '30 minutes')"),
        ]
        .iter()
        .enumerate()
        {
            // **打刻の一次表は hub_measurements。** 一覧はここからだけ導出する
            // (Refs ippoan/alc-app-s3#134)
            sqlx::query(&format!(
                r#"INSERT INTO hub_measurements
                       (tenant_id, device_id, kind, payload, seq, recorded_at)
                   SELECT $1, 'jst-fixture', 'timecard',
                          jsonb_build_object('card_id', 'CAFEBABECAFEBABE',
                                             'employee_id', $2::text),
                          $3, {expr} AT TIME ZONE 'Asia/Tokyo'
                   FROM (SELECT (now() AT TIME ZONE 'Asia/Tokyo')::date::timestamp AS d) t"#
            ))
            .bind(tenant)
            .bind(&employee_id)
            .bind(i as i64)
            .execute(state.pool())
            .await
            .unwrap_or_else(|e| panic!("{label} の打刻を入れられない: {e}"));
        }

        // punch すると応答に当日一覧が付く
        let res = client
            .post(format!("{base_url}/api/timecard/punch"))
            .header("Authorization", &auth)
            .json(&json!({ "card_id": "CAFEBABECAFEBABE" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let body: Value = res.json().await.unwrap();

        // JST 今日 00:30 + いま打った行 = 2 件。昨日 23:30 は入らない。
        // 旧実装だと時間帯に応じて 1 件 (今日 00:30 が落ちる) か
        // 3 件 (昨日 23:30 を拾う) になる
        assert_eq!(
            body["today_punches"].as_array().unwrap().len(),
            2,
            "当日一覧が JST の 0 時で切れていない: {body}"
        );
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

/// ブラウザ版 (キオスク / Android) の打刻も `hub_measurements` へ入る
/// (Refs ippoan/alc-app-s3#134)。
///
/// **書き込み先は `hub_measurements` だけ。** 一次表を 2 つ持つと「時刻がサーバ
/// 時刻になる」「端末 ID が入らない」「重複排除が要る」がそこから生まれる
/// (旧・打刻表は #620 で DROP 済み)。
#[tokio::test]
async fn test_browser_punch_writes_to_hub_measurements() {
    test_group!("timecard punches (ブラウザ版の書き込み先)");

    test_case!(
        "hub_measurements に入り、一覧にも当日一覧にも出る",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Browser Punch").await;
            let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "ブラウザ 太郎", "E020")
                    .await;
            let employee_id = emp["id"].as_str().unwrap().to_string();
            let res = client
                .post(format!("{base_url}/api/timecard/cards"))
                .header("Authorization", &auth)
                .json(&json!({ "employee_id": employee_id, "card_id": "BROWSERCARD01" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);

            // ブラウザ版の打刻 (大文字で投げる = 端末と同じ生値の形)
            let res = client
                .post(format!("{base_url}/api/timecard/punch"))
                .header("Authorization", &auth)
                .json(&json!({ "card_id": "BROWSERCARD01" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["employee_name"], "ブラウザ 太郎");
            // **打った本人の打刻が当日一覧に出る** — 書き込み先と読み出し先が
            // 割れていると空になる
            assert_eq!(
                body["today_punches"].as_array().unwrap().len(),
                1,
                "当日一覧に自分の打刻が出ない: {body}"
            );

            // hub_measurements に 1 行、payload に employee_id が凍結されている
            let (kind, payload): (String, Value) =
                sqlx::query_as("SELECT kind, payload FROM hub_measurements WHERE tenant_id = $1")
                    .bind(tenant)
                    .fetch_one(state.pool())
                    .await
                    .unwrap();
            assert_eq!(kind, "timecard");
            assert_eq!(payload["employee_id"], employee_id);
            assert_eq!(payload["card_id"], "BROWSERCARD01");

            // 一覧にも出る (端末の打刻と同じ経路で読める)
            let list = list_punches(&client, &base_url, &auth).await;
            let punches = list["punches"].as_array().unwrap();
            assert_eq!(punches.len(), 1, "{list}");
            assert_eq!(punches[0]["employee_name"], "ブラウザ 太郎");
            assert_eq!(punches[0]["kind"], "timecard");
        }
    );
}

/// 連続して打っても seq が衝突しない (sequence 採番)。
/// `MAX(seq)+1` だと同時打刻でリトライループが要る
#[tokio::test]
async fn test_browser_punch_seq_does_not_collide() {
    test_group!("timecard punches (seq 採番)");

    test_case!("同じ端末から連続で打てる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant = common::create_test_tenant(state.pool(), "Browser Seq").await;
        let auth = format!("Bearer {}", common::create_test_jwt(tenant, "admin"));
        let client = reqwest::Client::new();

        let emp =
            common::create_test_employee(&client, &base_url, &auth, "連打 次郎", "E021").await;
        let employee_id = emp["id"].as_str().unwrap().to_string();
        client
            .post(format!("{base_url}/api/timecard/cards"))
            .header("Authorization", &auth)
            .json(&json!({ "employee_id": employee_id, "card_id": "SEQCARD01" }))
            .send()
            .await
            .unwrap();

        for i in 0..3 {
            let res = client
                .post(format!("{base_url}/api/timecard/punch"))
                .header("Authorization", &auth)
                .json(&json!({ "card_id": "SEQCARD01" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201, "{i} 回目で失敗");
        }

        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM hub_measurements WHERE tenant_id = $1 AND kind = 'timecard'",
        )
        .bind(tenant)
        .fetch_one(state.pool())
        .await
        .unwrap();
        assert_eq!(n, 3);
    });
}
