---
name: rust-alc-api-map
generated-from: rust-alc-api:37a245edeab0e1e56dd863e40c2992f069769b0c
paths: [crates/, src/, tests/]
description: rust-alc-api (アルコールチェッカー基盤の Rust/Axum Cargo workspace — domain crate 群 + monolith 単一バイナリ、PostgreSQL+RLS、Cloud Run) の構造ナビゲーション。どの crate に何のルートがあるか / monolith (rust-alc-api) 一本化 (gateway + per-domain は #556 で廃止) / RLS・migration・deploy/release 分離の gotcha を 1 枚にまとめる。トリガー:「rust-alc-api」「alc-api」「alc-notify」「alc-tenko」「alc-carins」「alc-dtako」「gateway」「tenko-api」「carins-api」「dtako-api」「trouble-api」(trouble は ippoan/alc-trouble-worker へ移設済み)「RLS テナント」「sqlx migration」「ts-rs」「Release Wave」「Bazel」等。
---

# rust-alc-api-map — rust-alc-api 構造ナビゲーション

アルコールチェッカーシステムの backend。Cargo workspace で **domain crate 群** (`alc-*`)
+ **monolith 単一バイナリ** を持つ。PostgreSQL (`alc_api` スキーマ + RLS) / Cloudflare R2 (or GCS) /
Google OAuth + LINE WORKS。Cloud Run にデプロイ。

> ここは索引。網羅ではない。実ルートの完全列挙や関数シグネチャは repo 側が正。
> frontmatter の `generated-from` が現 tree-sha とズレたら hook が再生成を促す。

## バイナリ構成 (monolith 一本化、Refs #556)

| 系統 | バイナリ | 役割 |
|---|---|---|
| **monolith** | `rust-alc-api` (`src/main.rs`) | 全 domain crate の router を `/api` 下に一括 nest。全 domain (tenko/carins/dtako/notify/misc/auth) を 1 プロセスで提供 (trouble は ippoan/alc-trouble-worker へ移設、Refs #747。camera は削除、Refs #680) |
| **CLI** | `migrate` (`src/bin/migrate.rs`) / `archive` (`src/bin/archive.rs`) | sqlx migration 実行 (staging の起動時とテスト用。本番は alc-migrations 側が流す) / アーカイブ Job (dtako の R2 は `DTAKO_R2_BUCKET`、本番既定は `ohishi-dtako-apac`) |

**gateway (`crates/gateway`) + per-domain API (`tenko-api` / `carins-api` / `dtako-api` /
`trouble-api` / `alc-camera-api`) は #556 で廃止** (本番・staging とも休眠していたため、
attack surface と CI 時間削減で monolith に集約)。`alc-api.ippoan.org` は monolith に
domain-mapping。domain crate (`alc-notify` 等) は monolith が `.with_state` でマウントする
router 実装として存続。旧 per-domain は同じ domain crate を単独 main で立てていたが、
その薄い main (bin crate) だけを削除した。

## 区画 (workspace crate)

| crate | 役割 / 主要ルート群 |
|---|---|
| `alc-core` | 共通基盤: **共有** models / **共有** repository trait / `auth_middleware` / `tenant` / `realtime_bus` / `redact_broadcast` / **`api_error`** (理由付きエラーレスポンス `(StatusCode, Json<Value>)` の共通ヘルパー — `bad_request`/`not_found`/`upstream_error`/`internal_error`/`internal_error_msg`。alc-auth::internal / alc-notify::lineworks_channels / lineworks_config に散っていた同型定義を Refs #668 で統合。**新しいハンドラの理由付きエラーはここを使う。同型定義を crate 内に増やさないこと**)。**`master_data` / `master_handlers` は削除済み** (テナントごとのマスタの generic CRUD。最後の利用者だった整備カテゴリが W3 で ippoan/alc-carins-worker へ移った)。**port 化進行中 (Refs #539)**: `fcm` / `device_pair_client` は **trait (port) だけが core に残り、実装 (adapter) は alc-notify / alc-devices へ移設済み**。`auth_google` は internal OIDC 専用に縮小 (login 用 `verify()`/`extra_client_ids` は削除)。ts-rs 型 export 元。**tenko は ippoan/alc-tenko-worker (W7d)、trouble は ippoan/alc-trouble-worker へ移設済み** (Refs #680 / #747、alc-core への再流入と crate の再作成は `scripts/check_domain_split.sh` が CI で loud fail)。**新機能は原則ドメイン crate (無ければ新 crate) に置く** — alc-core に足してよいのは全ドメイン共有の基盤だけ。alc-core に struct 1 個足すと 12 test shard + 7 Builds が再ビルドされる (コストは CI 時間で可視) **`api_error` / `TenantId` / `AuthUser` / `require_tenant_header` の実体は `alc-core-wasm` (wasm32 でもビルドできる共有部分)。alc-core は元のパス (`alc_core::api_error::*` / `middleware::{TenantId,AuthUser}` / `auth_middleware::require_tenant_header`) のまま re-export するので import は変えない (Refs #681)** |
| `alc-core-wasm` | **この repo には無い。正本は ippoan/alc-worker-kit の crate `alc-core-wasm`** (Refs #736。rust-alc-api d28944e から写した)。`crates/alc-core` の 1 か所が git 依存 (rev 固定、feature `sqlx`) で引く (Bazel は `@crates//:alc-core-wasm`)。**直すときは ippoan/alc-worker-kit に PR → ここの rev を上げる** (`cargo tree -i alc-core-wasm` で出どころが 1 つを確かめる。分割 worker も同じ kit の rev から引く)。中身: `api_error` (理由付きエラー) / `TenantId`・`AuthUser` / `require_tenant_header` (X-Tenant-ID 解釈を monolith と worker で 1 か所に) / `DbError` (`From<sqlx::Error>` は feature `sqlx`) / `device_dev`。alc-core は元のパスのまま re-export する。wasm32 のビルドと行カバレッジ 100% の gate は kit の CI が持つ (この repo の `wasm-build` job・core-wasm の Bazel shard・`coverage_100.toml` の登録は外した) |
| ~~`alc-auth`~~ | **削除済み (W2b、Refs #680)**。認証 JWT の発行・検証とログインの口は auth-worker / alc-auth-worker (#479 PR-3・Refs #680)、最後に残っていた `GET /api/auth/me`・`POST /api/auth/logout`・`POST /api/my-orgs` も本番は auth-worker の `ALC_AUTH_SESSION=1` (v0.2.328) で alc-auth-worker から組んでいるので crate ごと消した。`AuthRepository` (alc-core) は alc-notify 等がまだ使うので残る。crate の再作成は `scripts/check_domain_split.sh` が落とす |
| `alc-misc` | health (`/health` + `/health/secret-fingerprint?name=&expected=` = 任意 env の sha256[0..8] と `expected` 突合、`{match: bool}` のみ返し oracle 防止。cross-store drift を CI で自動検出、Refs ippoan/rust-alc-api#424 / ippoan/ci-workflows#131。health_canary = JWT_SECRET drift 検知は #479 PR-3 の JWT_SECRET 全撤去に伴い削除) / bot_admin (**開発者だけの export `GET /admin/bot/configs/export` だけ**。auth-worker の `handleBotConfigExport` が呼ぶ。一覧・保存・削除は ippoan/alc-lineworks-worker)。**ユーザー・招待・SSO 設定・参加の申請・`tenants/by-slug` (sso_admin / tenant_users / access_requests) は削除済み** (W2c、Refs #680。ippoan/alc-auth-worker が持ち、auth-worker が認可して呼ぶ)。再流入は `scripts/check_domain_split.sh` が落とす |
| ~~`alc-tenko`~~ | **削除済み (W7d、Refs #680)**。点呼の口 (`/api/tenko/*`・`/api/tenko-call/*`。記録・CSV・ダッシュボード・予定・基準値・機器故障・日々の健康・乗務員情報・webhook の設定と配信の記録・点呼の電話 = W7a、セッションの各段・判定と webhook の送信 = W7b) と予定超過の確認 (W7c、Cron Trigger) は全部 ippoan/alc-tenko-worker にあり、本番は auth-worker の振り分け (v0.2.358・v0.2.359) で Worker に回る。通常点呼・IT点呼の行は alc-measurements-worker が書く。`TenkoState`、点呼の webhook 一式 (alc-core の `webhook.rs`・`WebhookRepository`・`AppState.webhook`・`Webhook*` の model、alc-misc の `PgWebhookRepository`、`src/webhook.rs`) は使い手が無くなったので一緒に消した。`tests/mock_tenko`・tenko の mock test・点呼の DB テスト (`normal_tenko_record_test`・`tenko_bp_required_escalate_test`・`driver_info_maintenance_test`、`device_dev_axis_test` の点呼の口を叩く分) も消した (表の制約の検査は worker の実 DB テストが持つ)。crate の再作成と再流入は `scripts/check_domain_split.sh` が落とす。直すときは ippoan/alc-tenko-worker へ |
| ~~`alc-carins`~~ / ~~`alc-maintenance`~~ | **削除済み (W3、Refs #680)**。車検証 (car_inspections・ファイル `/api/files*`・NFC タグ・期限の照会 `POST /api/car-inspections/lookup`) と車両整備 (車両マスタ・区分・整備記録・添付ファイル `/api/maintenance/*`) は全部 ippoan/alc-carins-worker に移り、本番は auth-worker の振り分け (v0.2.331〜) で Worker に回る。smb の取り込みも RPC で worker へ。worker に移さなかった口 (`/api/files/not-attached`・`/api/files/{uuid}` の取得・削除・復元・`/api/car-inspection-files/current`) は呼び手が無く 30 日 0 件だったので一緒に消した。`AppState` の `car_inspections`・`carins_files`・`nfc_tags`・`carins_storage`、`MaintenanceState`、alc-core の carins の trait・`repo/car_inspections.rs` (`lookup_expiry`)・`NfcTag` 型、`tests/mock_carins`・`mock_maintenance` も消した。点呼の乗務員情報 (driver_info) が整備の表を生 SQL で読むのは、点呼の口と一緒に W7d で ippoan/alc-tenko-worker へ移った。crate の再作成と再流入は `scripts/check_domain_split.sh` が落とす。直すときは ippoan/alc-carins-worker へ |
| ~~`alc-dtako`~~ | **削除済み (W5c-5、Refs #680)**。dtako の口は全部 ippoan/alc-dtako-worker (Cloudflare Worker `alc-dtako`、route の crate `crates/alc-dtako-upload/`) に移り、本番は auth-worker の振り分けで Worker に回る。最後に残っていた起票 `dtako_tickets` の 5 口 (`POST /api/dtako/tickets`・`PATCH /api/dtako/tickets/{id}/scraped` = email-receiver の内部用、`GET /api/dtako/tickets`・`GET /api/dtako/tickets/{id}`・`POST /api/dtako/tickets/close` = 画面用) も worker の `tickets.rs` (v0.3.15、auth-worker v0.2.349)。それまでに移したもの: upload・split・再計算・履歴の 10 口 (#725)、機械の取り込み (W5a)、勤怠向けの読み取り (W5b)、画面向けの読み取りと分類の更新 20 口 (W5c-1)、Y時間 export (W5c-2)、拘束時間の帳票 (W5c-3) とその PDF (W5c-4、`alc-pdf` とフォントも worker へ)。車輛設定 dump の口は呼び手が無く消した (W5-0)。`AppState` の `dtako_tickets`・`dtako_storage` (dtako の R2) と `tests/mock_dtako` も消した。alc-core に残る Dtako の型は `DtakologRow` (アーカイブの Job) と `DtakoDailyWorkHours` (点呼の乗務員情報) など。crate の再作成と起票の型の再流入は `scripts/check_domain_split.sh` が落とす。直すときは ippoan/alc-dtako-worker へ |
**機械の取り込み (DVR の動画通知 `/api/dvr/notifications`・`/api/dvr/files/{id}` と車両の現在地のログ `POST /api/dtako-logs/bulk`) は ippoan/alc-dtako-worker へ移り (W5a、v0.3.7。auth-worker v0.2.339 で本番切り替え済み)、ハンドラ・`DtakoLogsRepository::bulk_upsert`・`DtakologInput`・`BulkUpsertResponse` はこの repo から消した** (Refs #680)。表 `dvr_notifications`・`dtakologs` は残り、dtako-logs の読み取りも W5c-1 で worker へ移した。直すときは worker の `crates/alc-dtako-upload/src/{logs,dvr}.rs` へ。
| ~~`alc-trouble`~~ | **削除済み (Refs #747)**。trouble (tickets / files / workflow / schedules / tasks / notifier / lineworks_members 等) は全部 ippoan/alc-trouble-worker に移り、本番は auth-worker の振り分け (`/api/trouble/` prefix と fire) で Worker に回る。rust 側に trouble の route・repo・`TroubleState`・テストは無い。カメラ停止の自動チケットだけは `alc-camera` の `HttpDownTicketSink` が trouble-worker の内部口を HTTP で叩く。crate の再作成は `scripts/check_domain_split.sh` が落とす |
| `alc-notify` | LINE/LINE WORKS 配信のうち Cloud Run に残る口: documents / ingest / line_webhook / email_documents / test_endpoints / extract / redact / background_extract / background_redaction / lineworks_channels (internal の 2 口だけ)。**宛先 (recipients)・グループ (groups)・LINE の設定 (line-config)・LINE WORKS のトークルーム (`/notify/lineworks/channels`)・メンバー (`/notify/lineworks/users`)・ログイン状況 (`/notify/lineworks/login-activity`)・配信 (`/notify/documents/{id}/distribute`・`/notify/test-distribute`) は ippoan/alc-notify-worker、`POST /api/internal/lineworks/send` は alc-lineworks に移り、Cloud Run 側の口と、それだけが使っていた module (`distribute` / `recipients` / `groups` / `line_config` / `lineworks_directory` / `lineworks_login_activity` / `lineworks_config`)・repo (`notify_groups` / `notify_line_config` と、`notify_recipients` / `lineworks_channels` の CRUD・`get_for_send`)・AppState field は消した (Refs #747)**。使い手が無くなった `viewer_register` と、trouble の通知だけが使っていた `clients/lineworks.rs` (`LineworksBotClient`) も消した (Refs #747)。`clients/` に残るのは LINE (`line.rs`) だけ。閲覧の口 (`viewer`・`read_tracker`) と、それだけが使っていた repo の `mark_read`・`get_for_view` (`ReadResult`・`DeliveryViewInfo`) は W8a で消した (閲覧は nuxt-notify の Worker。Refs #680)。**`line_webhook` は #434 lockdown で internal_router 併設**: `/api/internal/notify/line/webhook` (`require_internal_jwt`、auth-worker の public 受け口が OIDC mint で forward)。署名検証 (全テナント channel secret 照合) は rust 側で、**`list_enabled_line_configs()` SECURITY DEFINER 関数経由で RLS バイパス** (migration 117。生クエリだと未認証パスで `app.current_tenant_id=''` → `''::UUID` キャストが 500 する既知罠)。**ただし 072 の `FORCE ROW LEVEL SECURITY` があると所有者=SECURITY DEFINER 実行ロールにも RLS が効いて関数経由でも 500 するため、migration 118 で `NO FORCE` にして所有者バイパスを効かせている** (devices は元から FORCE 無し)。app ロール (非所有者) には RLS 維持。follow イベントで `notify_recipients::upsert_by_line_user_id` を呼ぶ (`NotifyRecipientRepository` に残る唯一のメソッド)。`public_router` (`/notify/line/webhook`) は LINE Console URL 切替 + allUsers 削除までの移行期間 dual-mount。**`lineworks_channels` の internal_router** (`require_internal_jwt` 配下) は auth-worker の LINE WORKS webhook の DO が叩く 2 口: `GET /api/internal/lineworks/bot-secret/{bot_id}` (bot_secret_encrypted を返す。復号は auth-worker) と `POST /api/internal/lineworks/event` (検証済みの join/leave を `upsert_joined` / `mark_left`)。bot_id からの解決は SECURITY DEFINER 関数 `lookup_bot_config_for_webhook` |
| ~~`alc-devices`~~ | **削除済み (W6b、Refs #680)**。端末の登録・管理・報告 (W6b-a)、kiosk の再ペアリング (W6b-b。発行は auth-worker の `DevicePairEntrypoint` を Service Binding で呼ぶ)、FCM で端末に送る口と着信の設定 (W6b-c。FCM の送信専用の既存の鍵を Secrets Store から共有) は全部 ippoan/alc-devices-worker にある。端末の測定 `hub/measurements` は W6a で alc-people-worker へ。`AppState` の `devices`・`fcm`・`device_pair_client`、alc-core の `repository/devices.rs`・`fcm.rs`・`device_pair_client.rs`、alc-notify の `fcm.rs`、`tests/mock_devices`・devices の結合テストも消した。移さずに消した 5 口: 共有の秘密 `FCM_INTERNAL_SECRET` (本番に無い) を照合する `fcm-notify-call`・`test-fcm-all-exclude`・`trigger-update-dev` と、どの経路からも届かない `register/request`・`register/status/{code}`。crate の再作成と再流入は `scripts/check_domain_split.sh` が落とす |
| ~~`alc-camera`~~ | **削除済み (Refs #680)**。監視カメラの死活監視 (#345。拠点タブレットが ONVIF で生死を `camera_health_logs` に書き、連続失敗で trouble に障害を自動起票) は、書き手のチェッカー (ippoan/alc-app#27) が実装されないまま 60 日間 health-logs への書き込みが 0 件だったため、worker に移さず消した。カメラの台帳 `alc_api.cameras` は alc-cam-sd-ingest (ippoan/alc-cam の ingest/) が `/api/cam-clips/cameras` で持つ。死活監視が要るなら alc-cam 側で作り直す |
| `alc-storage` | StorageBackend trait + R2 / GCS / HttpProxy 実装。`list(prefix)` (Refs #205 の 13) はデフォルト未対応 (`StorageError::Config`) の trait メソッドで、R2Backend だけが `rust-s3` の `Bucket::list` (自動ページング、ETag のクォート除去込み) で実装。GCS/HttpProxy は現状未対応のまま |
| `alc-csv-parser` / `alc-compare` | **この repo には無い。正本は ippoan/alc-dtako-worker の `crates/`** (Refs #736)。git 依存 (同じ rev) で引くのは、`alc-csv-parser` が root `Cargo.toml`・`crates/alc-core`、`alc-compare` が root `Cargo.toml`・`crates/alc-dtako` (`crates/alc-dtako` は W5c-2 で Y時間 export が worker へ移り、`alc-csv-parser` を引かなくなった) (Bazel は `@crates//:alc-csv-parser`・`@crates//:alc-compare`)。**直すときは ippoan/alc-dtako-worker に PR → ここの rev を一斉に上げる** (rev が割れると出どころが 2 つになり型が合わない。`cargo tree -i alc-csv-parser` で 1 つを確かめる)。upload・split・再計算の 10 口は ippoan/alc-dtako-worker にあり、`split_csv_entry`・`compute_daily_hours` 等はそこが呼ぶ (この repo の `alc-dtako` は呼ばない。Refs #725)。alc-core は `alc-csv-parser` を `default-features = false` で引く (`OperationMinutes` の再公開)。中身の説明は worker の repo 側に在る。**alc-compare の `lib.rs` は 100% の gate がどこにも無い** (この repo の `coverage_100.toml` から外し、worker の repo には未設定。ippoan/rust-alc-api#736) |
| (vein の route の crate `alc-vein`) | ippoan/alc-vein-worker へ移した (Refs #721)。backend には vein の口は無い |
| (vein の Worker) | ippoan/alc-vein-worker へ移した (2026-10-02、Refs #721)。この repo には無い。配信はその repo の `deploy.yml` (タグ `v*` = 本番) |

## entrypoint / router

- **monolith**: `src/main.rs` — DATABASE_URL で `PgPool`、Storage backend (`STORAGE_BACKEND` = r2/gcs、
  notify は別バケット+別 R2 キー。dtako の R2 は W5c-5、carins の R2 は W3 で外した)、`AppState` (共有 repo 群) を組み立て、
  `.nest("/api", rust_alc_api::routes::router(internal_oidc_trust()))` (`TenkoState` は W7d で消した)。
  各 domain の route 群は router 内で `.with_state(...)` マウント (FromRef 変換は廃止)。trouble の `TroubleState`・
  `TROUBLE_R2_BUCKET`・`SCHEDULE_ALARM_URL` は #747 で消した (`cloudrun/render.sh` の env と trouble の R2 キーの secretKeyRef も)。背景 task は無い (60s ごとの予定超過の確認
  `check_overdue_schedules` は W7c で ippoan/alc-tenko-worker の Cron Trigger (毎分、通知済みの印を先に付ける形) へ移して消した)。
- **router 本体**: `src/routes/mod.rs` — 各 domain crate のルートを re-export し `router()` で結線。
  middleware: `require_tenant_header` (tenant/admin 共通、注入 identity 信頼) / `require_internal_jwt`
  (auth-worker→internal ingest、aud=alc-api-internal。**#479 で HS256 dual-accept を撤去し
  Google OIDC 一本化**: `GoogleTokenVerifier::verify_internal_oidc` が JWKS で RS256 署名検証 +
  iss + aud=alc-api-internal + exp を検証 (Cloud Run IAM に加えた app 層 defense-in-depth)。
  `InternalOidcTrust{verifier}` は `router()` の **DI 引数** — prod は `internal_oidc_trust()`
  (main.rs)、テストは `with_test_claims` verifier を注入。旧 `INTERNAL_AUTH_TRUST_OIDC` env と
  共有 JWT_SECRET での HS256 受理は削除済み)。共有の秘密の経路 (`require_internal_shared_secret`・
  `internal_shared_secret_router`) は**消した** — 最後の利用者だった端末の測定 `/api/hub/measurements` が W6a で
  alc-people-worker へ移った (dvr は W5a、起票 `/api/dtako/tickets` は W5c-5 で alc-dtako-worker へ)。
  `INTERNAL_SHARED_SECRET` を読む rust のコードはもう無い (`device_pair_client` も W6b で消した。render.sh の env は残る)。
  **#434 で monolith のローカル JWT 検証を撤去**: 旧 `require_jwt` / `require_tenant` (bare X-Tenant-ID
  フォールバック) / `TenantProxySecret` gate (#437) / 未配線の `require_tenant_or_device` (#436、device-token)
  を全削除し、tenant/admin 経路を `require_tenant_header` に一本化した。rust-alc-api は JWT を検証せず、
  前段 proxy (CF Worker = alc-app/carins/nuxt-items、実体は auth-worker の `/alc-proxy` 系) が auth-worker
  `/auth/introspect` で検証して注入する `X-Tenant-ID` / `X-User-*` ヘッダーを信頼する dumb backend。
  外部直叩き防止は **Cloud Run IAM 網層ロックダウン** (proxy の OIDC ID token のみ到達可) が担う
  (確定アーキ #4807535677、step 3)。テストは `tests/common/mod.rs` の `test_proxy_inject` が proxy 役で
  Bearer token (base64(JSON) の opaque token) → identity ヘッダーに変換し従来テストを無改修で通す。
  **JWT_SECRET は rust から全撤去済み (#479 完了)**: main.rs の env 読取 / `Extension(JwtSecret)` /
  render.sh の secretKeyRef 注入 / `alc-auth-jwt` の HS256 発行・検証関数 / health_canary を全て削除。
  rust バイナリは HS256 鍵を一切持たず、JWT の発行・検証は auth-worker が単独で担う。
- **RLS が効いているかの内部用の口 `GET /api/internal/rls-check`** (`src/routes/rls_check.rs`、Refs ippoan/auth-worker#605):
  migration を本番に当てた後にオーナーが SQL エディタで手で見ていたものを 1 回で返す。呼び手は auth-worker の
  MCP の tool (既存の内部用の経路 = Google OIDC、`aud=alc-api-internal`)。`internal_protected`
  (`require_internal_jwt` の配下) に merge してあり、無認証の health の並びには置かない。
  root の `src/routes/` に在るのは `alc_migrations` に依存しているのが root crate だけだから
  (`crates/` 側に migration 一式の依存を足さない)。
  - **返すもの** (key は契約で固定。auth-worker の tool がこの形を読む。最上位は 9 個): `ok` /
    `verdicts{invariants, runtime_role, migrations, drift}` (合否の内訳。4 個ちょうど・全部 boolean) /
    `migrations{applied, max_version, binary_count, binary_max_version, matches_binary}`
    (`alc_api.migration_status()` = alc-migrations 160 の SECURITY DEFINER 関数と、binary に埋め込まれた
    `alc_migrations::MIGRATOR` の件数・最大 version) /
    `runtime_role{current_user, is_runtime_role, rolsuper, rolbypassrls, rolinherit, member_of_table_owner}`
    (実行用ロール `alc_api_rt` の `pg_roles` の行。行が無ければ 500) /
    `connections[{usename, count}]` (`pg_stat_activity` の `alc_api` 系のロールごとの接続数) /
    `owner_role_connected` (表の所有者のロールが繋いでいるか) /
    `invariants{violation_count, violations[{check_no, object, detail}], checks[{check_no, title, violations}]}` /
    `state` / `drift` (object か null)。
  - **`ok` の決め方 = `verdicts` の 4 つが全部 true** (`ok` は `verdicts` から計算する = `Verdicts::ok`。
    内訳は純関数 `verdicts`)。**`ok` が false のとき、どれが原因かは `verdicts` で読む**:
    `invariants` = 違反 0 件 / `runtime_role` = 実行用ロールで繋いでいる かつ 属性 4 つが false /
    `migrations` = `matches_binary` / `drift` = `drift` が取れていて `drift.matches_expected`
    (**`state` を取れず `drift` が null なら false** = 検査できていないものを合格にしない)。
    **`owner_role_connected` は合否に入れない** (migration の job の実行中は所有者ロールが一時的に繋ぐ)
  - **`drift` = migration の履歴との食い違い**: 同じ transaction で取った `state` を、
    `alc_migrations::RLS_EXPECTED_STATE` (全 migration を 0 から当てた DB の状態の JSON。正本は alc-migrations の
    `ci/expected_rls_state.json` で、あちらの CI が「実物 = このファイル」を強制している) と比べたもの
    (純関数 `compute_drift(expected, actual)`。**比べるための問い合わせは足さない・期待する状態の写しを置かない**)。
    形は `{matches_expected, tables[{name, expected, actual, expected_policy_names, actual_policy_names}],
    functions[{signature, expected, actual}], views, sequences, unreadable[]}`。
    - **表は「組」ではなく表の単位で比べる**: `tables` の組を `names` で開いて「表の名前 → 表の状態」にする
      (表の状態 = 組から `names`・`count`・`owner` を除いたもの)。組の割れ方が違っても、表ごとの状態が同じなら一致。
      載るのは、状態が違う・`policy_names` が違う・片方にしか無い表だけ (無い側は null)。名前の昇順
    - **`owner` (所有者のロール名) は比べない**: 名前は環境で違う (テスト DB は postgres) ので、期待する状態には
      元から無く、実物の側からも落として比べる。出し分けに効く `runtime_role_can_act_as_owner` (真偽) は比べる。
      これで本番・テスト DB・alc-migrations の CI の 2 軸が、読み替えなしで同じ期待値と比べられる
    - `functions` は `security_definer_functions` を署名で突き合わせ (署名の昇順)、`views` / `sequences` は
      違うときだけ `{expected, actual}` (同じなら null)
    - **黙って一致にしない**: 最上位に知らない key が在る・形が読めない (配列のはずが違う・同じ表が 2 回出る等) ときは
      `unreadable` に `expected.<key>` / `actual.<key>` を入れ、`matches_expected` は false (`unreadable` は
      無ければ空配列で、key は常に出す)。`RLS_EXPECTED_STATE` が JSON として読めなければ 500 にせず `drift: null`
    - 配列 (`policies`・policy の名前・`runtime_role_privileges`) は SQL の側で並びが固定なので、並べ替えずに比べる。
      式と署名の文字列は PostgreSQL の版と接続の search_path に依る — **全部の表・関数が一斉に drift になったら、
      版か search_path が変わったことを疑う**
    - RLS の状態を変える migration を取り込むとき (rev を上げるとき) は、期待する状態も同じ rev で付いてくる
  - **`invariants.checks` は「何を検査したか」を返った値で確かめる材料で、合否には入れない**
    (不変条件として足したい合否は alc-migrations の検査 SQL に検査として足す)。`checks` は
    `alc_migrations::RLS_INVARIANT_CHECKS` (番号と題) を順に並べ、違反の行を番号で数えたもの
    (純関数 `build_checks`。違反が無くても毎回、一覧の全件 = いまは検査 0〜9 の 10 要素。一覧に無い番号の違反は
    題 `(一覧に無い検査)` で番号の昇順に末尾へ足す。数の合計 = `violation_count`)。`state` は `alc_migrations::RLS_STATE_QUERY`
    (`alc_api` schema のカタログの実物を 1 行 1 列の JSON で返す 1 文) の値をそのまま入れる
    (`serde_json::Value`。形の正本は alc-migrations の `ci/rls_state.sql` で、ここでは名指しで写さない)。
    **`state` は取れなければ null で、主の結果を道連れにしない** (200 のまま。失敗はログに 1 行。
    `drift` も null になり、`verdicts.drift` と `ok` は false)。
    そのために**状態の問い合わせは transaction の最後に流す** (PostgreSQL は transaction の中で文が失敗すると
    以後の文が全部失敗する。`fetch_state` より後に問い合わせを足さない)
  - **引数を足さない**: extractor は `State` だけ (`Query` / `Path` / `Json` を書かない)。SQL・表名・テナントを受ける
    入口を作らない。query を付けても読まない
  - **transaction は `begin_read_only(pool)` からだけ取る** (`pool.begin()` → `SET TRANSACTION READ ONLY`。
    最初の問い合わせより前に流す)。最後は rollback。固定の問い合わせ (主の 5 つ + 状態の 1 つ) はどれも bind する値が無く、
    `format!` で SQL を組み立てない
  - **検査 SQL・状態の SQL・期待する状態の写しを置かない**: 不変条件の検査は `alc_migrations::RLS_INVARIANTS_QUERY`、
    状態は `alc_migrations::RLS_STATE_QUERY` をそのまま流し、期待する状態は `alc_migrations::RLS_EXPECTED_STATE` を読む
    (加工・連結しない。正本は alc-migrations の `ci/check_rls_invariants.sql` / `ci/rls_state.sql` /
    `ci/expected_rls_state.json`)
  - **行のデータを返さない・ログに出さない**: 返すのはカタログ由来のロール名・表名・関数名・真偽・件数だけ。
    `pg_stat_activity` から読む列は `usename` と件数だけ。失敗は 500 で、本文は固定の文言
    (`internal_error_msg`。SQL・カタログの中身を載せない)
  - テスト: `verdicts` と `ok`・`compute_drift` (fixture は架空の名前)・`checks` の組み立て・応答の key は同ファイルの unit、
    実 DB は `tests/runtime_role_test.rs` の `rls_check_*` (新しいテストファイルは作らない = bazel の配線を増やさない)。
    陽性対照 2 つ (`ALTER ROLE alc_api_rt BYPASSRLS` = 不変条件とロールの内訳が false /
    `ALTER TABLE … FORCE ROW LEVEL SECURITY` = `drift` の内訳だけが false) は DB 全体に効くので、
    同ファイルの `ROLE_ATTRS` (tokio の RwLock) を write で取り、ほかのテストは `setup()` が read で取る。
    **テスト DB が期待する状態と一致すること (`drift.matches_expected == true`) も実 DB のテストが固定している** —
    テストの初期化が `alc_api` schema に object を足すと、ここが落ちる
- **gateway (廃止済み、Refs #556 PR2)**: 旧 `crates/gateway` は auth + reverse proxy で
  `is_public_route` 判定して per-domain へ振る役だったが、本番・staging とも休眠のため削除。
  introspect 検証 + identity 注入は **auth-worker の `/alc-proxy` / `/alc-internal-proxy` 系**が
  担い、monolith backend へ直接 forward する (gateway を経由しない)。internal ルート
  (`/api/internal/*`) も monolith が直接処理する。前段の introspect 委譲設計 (#479 PR-2) 自体は
  auth-worker 側に存続。

## gotcha (CLAUDE.md / README 由来)

- **保存 secret の暗号鍵は `SSO_ENCRYPTION_KEY` 必須** (Refs #479 PR-1)。LINE channel
  secret / LINE WORKS bot secret / SSO client_secret の AES-256-GCM 鍵素材
  (`SHA-256(SSO_ENCRYPTION_KEY)`、`alc-core::auth_lineworks::{encrypt,decrypt}_secret`)。
  旧 `or_else(JWT_SECRET)` の env-presence fallback は撤去済み — 未設定は loud に 500。
  復号側は auth-worker (同 secret の CF binding) と共有。render.sh は monolith backend に注入
  (LINE WORKS Bot の Private Key を復号して使う trouble の通知は #747 で Worker へ移り、rust は Private Key を読み書きしない)。

- **DB 接続**: `alc_api_app` ロール (NOBYPASSRLS → RLS 有効) で、**直接接続 port 5432** を使う
  (Supavisor 6543 は session スコープの `set_config` (monolith の `set_current_tenant`) をリセットし
  RLS テナント分離が壊れる)。ただし **分割 worker (別 repo。最初は ippoan/alc-vein-worker) は `in_tenant_tx` でトランザクション単位
  (`set_config(..., true)`) の RLS にする場合に限り、プーラーの 6543 を使う** (実装はその repo に在る)。
  `DATABASE_URL` に `?options=-c search_path=alc_api` 必須。
  **接続先の secret (Refs ippoan/alc-app#387)**: 本番の service (`cloudrun/render.sh` の本番枝) は
  実行用ロール (表の非所有者 = FORCE の無い表でも RLS が掛かる) の `alc-app-database-url-rt`。
  archive job は所有者の `alc-app-database-url` のまま。
  戻すときは render.sh の secret 名を戻す。
- **staging は postgres superuser 接続 → RLS 完全 bypass** (`staging/cloudrun-staging.yaml` の
  `postgresql://postgres:...`)。superuser は `FORCE ROW LEVEL SECURITY` でも RLS を無視するため、
  **RLS 頼みで WHERE tenant_id を省いたクエリは staging で全テナント横断に漏れる**。tenant scope は
  `crates/alc-core/src/tenant.rs::TenantConn` が `set_current_tenant` で `app.current_tenant_id` を
  立てて RLS に委ねるが、本番 (alc_api_app) でしか効かない。staging も対象にするクエリは
  **WHERE tenant_id を明示**すること (Refs #434、sso_admin で実害)。
- **呼び出し元に tenant が在る query は `TenantConn` 経由 + `WHERE tenant_id` を明示 (Refs ippoan/alc-app#387)**:
  本番の backend は表の所有者で繋いでいるあいだ、`FORCE ROW LEVEL SECURITY` の無い表に RLS が
  1 行も掛からない (分離は query の `WHERE tenant_id` 頼み)。実行用ロール (所有者でない) へ切り替えると
  ポリシーが掛かり始め、tenant context を立てずに `self.pool` / `state.pool()` で叩く query は
  42704 / 22P02 で落ちる。**どちらの接続でも同じ結果になるよう、tenant を渡せる query は
  `TenantConn::acquire` で接続を取り、id / email で引く UPDATE・DELETE・SELECT にも
  `AND tenant_id = $n` を書く** (RLS 任せにしない)。この形に直した口:
  - `alc-core/src/repo/auth.rs`: `create_user_google` / `create_user_lineworks` / `create_user_line` /
    `register_line_recipient` / `delete_invitation(tenant_id, id)` / `clear_refresh_token(tenant_id, user_id)`
    (後ろ 2 つは署名に tenant_id を足した。呼び出し元は `alc-misc` の `tenant_users.rs` と
    `alc-auth` の `lib.rs` の logout = `auth_user.tenant_id`。ログインの内部口 `internal.rs` は Refs #680 で消したので、
    `create_user_*` / `register_line_recipient` などはテストの準備にだけ使われている)
  - (W6b で消した) `alc-devices/src/repo/devices.rs`: `claim_update_permanent_qr(tenant_id, req_id, …)` (handler は
    `req.tenant_id` が無ければ 400「無効なトークンです」) / `list_tenant_fcm_tokens_except`。
    **`get_fcm_token_bypass_rls` は消した** — 設定変更の通知 (`update_call_settings`) は tenant 付きの
    `get_device_fcm_token(tenant_id, id)` でトークンを引く
  - (W7d で消した) `alc-tenko`: (予定超過の `TenkoOverdueRepository` は W7c で消した) /
    `TenkoCallRepository::delete_number(tenant_id, id)` (handler が `Extension<TenantId>` を取る。
    `tenko_call_numbers.tenant_id` は TEXT なので `&str`)
  - `alc-misc/src/access_requests.rs`: 一覧・承認・却下は helper `tenant_conn` (`auth_user.tenant_id`) の接続で叩く。
    `create_request` (申請先テナントへ INSERT) と slug 検索は認証前・テナント横断の側なのでそのまま
  - `alc-misc/src/repo/tenant_users.rs`: 一覧 2 本・`invite_user`・id / email 指定の DELETE・UPDATE すべて
  - **認証前・テナント横断が本来の動きの query は SECURITY DEFINER 関数経由 (alc-migrations 158)**。
    tenant を立てられないので `TenantConn` には出来ず、pool 直のまま関数を呼ぶ (関数は所有者の資格で動くので、
    所有者接続でも実行用ロールでも同じ結果)。**関数名は `alc_api.` で schema 修飾する**。対応:
    `find_user_by_google_sub` → `alc_api.find_user_by_google_sub($1)` /
    `find_user_by_lineworks_id` → `alc_api.find_user_by_lineworks_id($1)` /
    `save_refresh_token` → `alc_api.save_user_refresh_token($1, $2, $3)` (**引数は user_id, token_hash, expires_at の順**) /
    `find_invitation_by_email` → `alc_api.find_invitation_by_email($1)` /
    `list_fcm_devices` → `alc_api.list_fcm_devices()` / `list_all_callable_devices` → `alc_api.list_all_callable_devices()` /
    `list_dev_device_tenant_ids` → `SELECT t::text FROM alc_api.list_dev_device_tenant_ids() t` (関数は uuid を返す。
    Rust 側は String のまま) / (W7c で消した) `find_overdue_configs` → `alc_api.list_tenko_overdue_webhook_configs()` /
    `create_request` の INSERT → `alc_api.create_access_request($1, $2)` (tenant_id, user_id)。
    **既存の関数に寄せた 2 つ** (W6b で消した `alc-devices/src/repo/devices.rs`): `get_registration_status` は `devices` への JOIN を
    やめ、request を引いた後 `device_id` が在るときだけ `get_device_re_pair_state` (migration 122) で `settings_token` を取る
    (2 往復になる。端末の行が無ければ None = LEFT JOIN のときと同じ) / `get_device_tenant_active` は同じ関数の
    `tenant_id` と `status` を使い、Rust 側で `status == "active"` だけを返す
  - **pool 直のまま残っている query** (どれも実行用ロールで通る形): `tenants` (RLS 無し) /
    `device_registration_requests` の SELECT (`USING (true)`) と INSERT (`WITH CHECK (status = 'pending')`) /
    `tenko_call_numbers`・`tenko_call_drivers` の SELECT (`USING (true)`。全テナントが見える) /
    SECURITY DEFINER 関数の呼び出し。**例外**: 呼び出し元の無い `find_user_by_refresh_token_hash` /
    `find_user_in_tenant` / `find_user_by_username` (auth.rs) と、FORCE の表を tenant 無しで叩く
    `dtako_upload.rs` の 3 本 (消した。upload は ippoan/alc-dtako-worker が持つ)。
    **新しい query を pool 直で書くなら、この 4 種のどれかに当たること**
    (RLS 有効の表を tenant 無しで叩くと、実行用ロールでは 42704 / 22P02 か 0 行になる)。
    `tenko_call` の `ON CONFLICT (phone_number)` と `tenant_users` の `ON CONFLICT (email)` はテナントをまたぐ
    UNIQUE に当たるままで、切り替え後は他テナントと衝突するとエラーになる (未対応)
  - (W6b で消した) **公開 route `POST /devices/test-fcm-all-exclude` は `check_internal_secret` (`X-Internal-Secret` =
    `FCM_INTERNAL_SECRET`) を通す** — 全テナントの端末へ FCM を送る口で、以前は認可が無かった。
    呼び出し元は alc-app の signaling (`cf-alc-signaling` の `room-registry.ts`) で、隣の
    `/devices/fcm-notify-call` と同じ形でヘッダを付ける。secret 未設定は 503 (fail-closed)、不一致は 401
  - テストは `tests/tenant_scoped_queries_test.rs` (実 DB。superuser 接続なので RLS は素通り =
    落ちれば query 側の条件が抜けている) と `tests/runtime_role_test.rs` (実 DB。実行用ロール `alc_api_rt` で
    繋ぐので RLS が掛かる = 落ちれば tenant 無しで RLS の表を叩いている)
- **dev端末 (開発用の鍵) の記録の分離 — `X-Device-Dev` → `app.device_dev` (Refs ippoan/alc-app#387)**:
  dev端末で行った点呼・測定・打刻の行は、その鍵の要求からしか見えない。**INSERT / SELECT の SQL は
  書き換えていない** — backend は検証済みの要求の印を DB の接続の設定に通すだけで、行の出し分けは
  alc-migrations 153 の列の既定値 (`is_dev`) と RLS が担う。対象は 8 表 (`tenko_sessions` /
  `tenko_records` / `measurements` / `hub_measurements` / `webhook_deliveries` / `equipment_failures` /
  `tenko_carrying_item_checks` / `tenko_schedules`)。
  - **ヘッダを読むのは 1 関数**: `alc_core_wasm::device_dev::device_dev_from_headers` (値がちょうど `"1"`
    のときだけ dev。`true` / `0` / 空 / 欄なし / 同名の欄が 2 つ以上は dev でない)。入れ物
    `DeviceDevSlot` (bool 2 個を `Arc` で共有、純 std) も同じ module — alc-core-wasm は tokio を持てないため
  - **鍵の役割 `X-Device-Role` を読むのも 1 関数・書くのは 1 か所**: 同じ module の
    `device_tenko_manager_from_headers` (同名の欄がちょうど 1 つで、値が `device-tenko-manager` に
    バイト列として完全一致のときだけ true。別の role / 大文字違い / 前後の空白 / 複数欄は false)。
    入れ物の 2 個目の bool (`set_tenko_manager` / `is_tenko_manager`) に書くのは
    `alc-core-wasm` (ippoan/alc-worker-kit) の `tenant_header.rs` の dev の印の隣**だけ** (X-Tenant-ID の検証を通した後)。
    読み口は `alc_core::device_dev::is_tenko_manager_key()` = **運行管理者の鍵** のとき true
    (dev かどうかは問わない。スコープの外は false。dev かどうかは `is_device_dev()` で別に見る)。
    使うのは点呼の判定の口 `POST /tenko/sessions/{id}/judgment` (`record_manager_judgment`) 1 か所で、
    認可は 2 段 — (1)「`AuthUser` がある、または `is_tenko_manager_key()`」でなければ 401
    (キオスクの鍵は dev でも 401 のまま)、(2) 記録を読んだ後、`AuthUser` が無く dev でもない
    (= 本番の運行管理者の鍵) のに記録の `tenko_method` が `IT点呼` でなければ 403。
    **本番の運行管理者の鍵が判定できるのは IT点呼 の記録だけ**で、body の `method` では決めない。
    `AuthUser` と dev の運行管理者の鍵は点呼方法で絞らない。この絞りは「既存の記録の `tenko_method` を
    `IT点呼` に書き換える経路が無い」ことに依る (`tenko_method` の UPDATE は `escalate_to_remote` の
    `'遠隔点呼'` 固定 1 本だけ。`IT点呼` は alc-measurements-worker の INSERT でしか入らない) —
    **`tenko_method` を UPDATE する SQL を足すときはここを見直す**。dev の鍵の要求は接続も dev の軸なので、
    本番の軸のセッションは RLS で見えず 404 になる (本番の行は書き換わらない)
  - **認証を通した要求にだけスコープが立つ**: `alc_core::auth_middleware::require_tenant_header` は
    wasm 側の実体を包む薄い関数で、入れ物を request extensions に入れて `alc_core::device_dev::scope`
    (tokio の task-local) を張る。実体は**認証を通した後に**入れ物へ書く (入れ物が無い worker では
    何もしない)。**外側の layer は無い** — `src/routes/mod.rs` の layer 7 か所は無改造で、認証
    middleware を通る = スコープが在る (足し忘れる場所が無い。app は `src/main.rs` と
    `tests/common/mod.rs` の 2 か所で別々に組み立てているので、外側の layer だと片方に書き忘れても
    テストが緑になる)。`require_internal_jwt` (auth-worker の内部口) は印を読まない
  - **`set_current_tenant` (Rust、`alc-core/src/tenant.rs`) のシグネチャは不変**。SQL の
    `set_current_tenant($1)` の**後に**、`device_dev::is_device_dev()` が true のときだけ
    `set_config('app.device_dev', '1', false)` を打つ (SQL 関数が毎回 `''` に戻すので逆順は不可)。
    dev でないときは追加の文を打たない
  - **スコープの外は dev でない**: 公開 route / 認証前 / バッチ (予定超過の確認。W7c で worker の Cron へ移した) /
    **`tokio::spawn` の先**。spawn の先で書いた行は本番の行になるので、dev かどうかは spawn の前に取る
  - (W7d で消した。点呼の webhook の送信は alc-tenko-worker が持つ) **dev の要求では点呼の webhook を発火しない**: `alc-tenko/src/tenko_sessions.rs::spawn_webhook`
    (点呼の 5 か所の発火を 1 つにまとめた関数) が spawn の前に判定し、dev なら配信の行も作らず外部へも
    送らない。**新しい発火を足すときはこの関数を通す** (直接 `tokio::spawn` + `fire_event` を書くと、
    dev の要求の配信が本番の行として残る)
  - **プールへ返すときにも消す**: `alc_core::tenant::reset_tenant_context` (`RESET app.current_tenant_id;
    RESET app.device_dev` を 1 往復) を `src/main.rs` の `after_release` が呼ぶ
  - **`hub_measurements` (端末の測定・打刻) はこの repo に無い** — W6a で ippoan/alc-people-worker の `crates/people/src/hub.rs` へ移った
    (重複判定は軸ごとの 4 列 `ON CONFLICT (tenant_id, device_id, seq, is_dev)`。3 列だと 42P10 で全件落ちる。直すときはそちら)
  - **テスト**: `tests/device_dev_axis_test.rs` (実 DB。dev の運行管理者の鍵の判定もここ。hub の軸のテストは alc-people-worker の `tests/hub_db.rs` へ移った)。ほかの DB テストは superuser で繋ぐので RLS を
    素通りする — ここと `tests/runtime_role_test.rs` だけ `common::setup_app_state_as_app_role` (接続ごとに
    `SET ROLE alc_api_rt` = alc-migrations 158 の実行用ロール。テスト DB の表の所有者は postgres なので非所有者 =
    本番で接続を切り替えた後と同じ立場) で走らせる。**staging も superuser 接続なので、staging では dev の行の読み出しは分かれない**
    (書いた行の `is_dev` は立つ。上の「staging は RLS 完全 bypass」と同じ理由)
- **pre-auth SECURITY DEFINER + FORCE ROW LEVEL SECURITY の罠**: 未認証経路 (LINE webhook /
  LINE login 等) が cross-tenant 検索用の SECURITY DEFINER 関数
  を叩く時、対象テーブルに `FORCE ROW LEVEL SECURITY` が有効かつポリシーが
  `current_setting('app.current_tenant_id', true)::UUID` を `NULLIF` でガードせず直接キャストして
  いると、tenant context 未設定 (空文字) で `invalid input syntax for type uuid: ""` 500 になる
  (テーブル所有者 = SECURITY DEFINER 実行ロールにも FORCE 下では RLS が適用されるため)。
  `devices`/`bot_configs`/carins 系は `NULLIF(..., '')::UUID` でガード済みで安全、
  `notify_line_configs`(#434 migration 118)/`notify_recipients`(119)/`notify_deliveries`+`notify_documents`(120)
  は同じ穴があり `NO FORCE ROW LEVEL SECURITY` で修正済み。新しい pre-auth SECURITY DEFINER 関数を
  追加する時は対象テーブルの RLS ポリシーの cast パターンを確認すること。
  - **裏の非対称にも注意**: recipient 逆引き (`find_recipient_by_line_user_id`, 076/119) は
    SECURITY DEFINER 化されていたが、`users` の LINE 逆引きは repo 層が `SELECT * FROM users` を
    **直接**叩いており (SECURITY DEFINER 無し)、`users` は FORCE 無しでも RLS ENABLE + ポリシーが
    `current_setting('app.current_tenant_id')::UUID` (**missing_ok 無し**) なので pre-auth では
    プール接続の残留 GUC 次第で 500/未検出になる非決定バグがあった。migration 121 で
    `find_user_by_line_user_id(TEXT) RETURNS SETOF users` を SECURITY DEFINER 化して repo を
    関数経由に変更、recipient と対称化した。
- **migration 不変条件**: 適用済み `migrations/*.sql` を絶対に変更しない (sqlx が SHA-384 検証、不一致で起動不能)。
  修正は新ファイル追加。本番の migration は ippoan/alc-migrations の migrate.yml (手動、Actions の runner から直接 DB へ) が流す。
  rust-alc-api の tag release は `deploy.yml` の `migrations-applied` で確かめるだけ (`main.rs` から `sqlx::migrate!()` は削除済み = 起動時自動適用なし)。
- **snapshot hook (plan 整合性)**: `if_flag!(...)` 追加時は先に `ippoan/ippoan-dev-plans` の
  `scope:rust-alc-api` plan Issue を作り `npm run snapshot` で `manifests/production.snapshot.json` 更新。
  pre-commit (`.githooks`) + CI `snapshot-check` job が drift/stale-sha を検出。`SKIP_CLIPPY=1` で commit 時 clippy skip 可。
- **ts-rs**: 各 crate が `#[ts(export)]` で TS 型を生成 (`cargo test export_bindings`)。フロント
  (nuxt-*) が型同期する。型変更時は export_bindings を回す。生成元は alc-core / alc-misc /
  alc-carins の 3 crate だった (alc-carins は W3 で削除) (alc-auth の未使用だった ts-rs 依存は削除済み)。CI での
  `ts-bindings-${sha}` artifact は **test-lib job が生成** する (check job ではない。lib shard は
  #507 で test-matrix から DB なしの test-lib job に分離。Refs #482 / 下記 CI 節)。
- **長時間 compute と Cloud Run**: `tokio::spawn` で background compute → fire-and-forget broadcast は
  やらない (Cloud Run は応答後に CPU を絞る)。`RealtimeBus` / `RedactBroadcaster` で対処。CLAUDE.md 該当節参照。
- **裸の `Err(StatusCode::BAD_REQUEST)` は本文もない 400 になる**: axum の
  `StatusCode` エラー型はレスポンス本文を持たないため、フロントには
  `API エラー (400):` としか出ず原因不明になる (`submit_medical` の 3 箇所で
  2 回本番で誤修正した実害、Refs yhonda-ohishi-alc/alc-app#322/#323、
  ippoan/rust-alc-api#668)。理由を返したいハンドラは戻り値を
  `alc_core::api_error::ApiError` (`(StatusCode, Json<Value>)`) にし、
  `bad_request(error, message)` 等を使う。**ハンドラの戻り型を変えると同じ
  handler 内の他の `StatusCode` エラーも巻き込む**ので、影響範囲をそのハンドラ
  だけに絞ること。
- **血圧必須のボンド判定はヘッダーが正本 (`submit_medical`, Refs #668)**:
  `X-Device-Bp-Bonded: 1`(必須)/`0`(不要)/ヘッダー無し(不明→フェイルクローズ)を
  auth-worker が CoreS3 の署名を検証して転送する。ヘッダーが無いときだけ
  `body.device_id` → `devices.bp_enabled` (登録済み端末向け) にフォールバックする。
  クライアント (body) には血圧要否を申告させない設計 — フィールドを足さないこと。
  **400 の message は根拠ごとに言い分ける (Refs #668 の 2)**: `bp_required` (真偽) だけで
  判定していた頃は 400 の message が「ボンド検出」「端末設定」「不明 (fail-closed)」の
  どの経路でも同じ断定文になっていた (#670 で理由を足した直後にこれが再発)。
  `submit_medical` 内の `BpRequiredReason` (`Bonded` / `DeviceSetting` /
  `Unknown(UnknownBpReason)`) が判定条件そのものは変えずに message だけを分ける。
  `error` コード (`bp_required`) は不変。

## CI / deploy から見た立ち位置

- **Bazel + Cargo の二重ビルド**: `BUILD.bazel` (rust_library `rust_alc_api_lib` + rust_binary 群 + rust_test)
  と Cargo workspace の両方が存在 (`MODULE.bazel` / `.bazelrc`)。CI の merge gate は Cargo
  (`cargo llvm-cov nextest`)。`bazel test //... --build_tests_only` は観測用 job
  (`bazel-test-poc`) + main-push warm で全 unit test を回す (run 跨ぎ result cache、Refs #515)。
  **dev-dependencies を持つ crate (alc-misc / alc-notify) の rust_test には
  `all_crate_deps(normal_dev/proc_macro_dev)` の配線が必須** — 無いと `#[tokio::test]` 等が
  unresolved で FAILED TO BUILD になる。
  **`rust_test` を足したら matrix を 2 箇所に足す** — `ci.yml` の実行 job
  (`bazel-test-poc` / DB 付きは `bazel-test-db`) と、**`cache-warm.yml` の対になる warm job**
  (`cache-warm-bazel-test-poc` / `cache-warm-bazel-test-db`)。`scripts/check_bazel_test_matrix.sh`
  が (a) BUILD の全 rust_test が matrix にあるか (b) **実行 job と warm job の matrix が
  `target`/`name`/`pdfium`/`coverage` まで一致するか を検査し、片方だけだと落ちる。
  このチェックは `bazel-test-poc` の **`auth-jwt` セルでだけ**走るので、
  **失敗ジョブ名 (`Bazel test (auth-jwt)`) は原因と無関係** — ログの
  `matrix target 不一致` を見ること。ローカル再現は `bash scripts/check_bazel_test_matrix.sh`
  (数秒、bazel 不要)。
- **deploy.yml は deploy/release 分離 (Refs #137)**: PR → staging 自動 deploy、tag(v*) push → production。
  **production の tag release は新 revision を 0% (no-traffic) で deploy するだけ**で traffic は旧 revision に残す。
  実際の切替は **Release Wave flip** が行う。`verify-no-traffic` job がこの不変条件を検証 (latest revision が
  0% traffic でなければ FAIL)。
- **単一 Dockerfile**: `Dockerfile` (monolith + migrate + archive + PDFium 同梱。migrate は他 repo の結合テスト用で、本番では起動しない) のみ。monolith が
  単一 Cloud Run service (`rust-alc-api` / staging `rust-alc-api-staging`) として deploy される。
  `cloudrun/render.sh` が YAML 生成 (service は `backend` のみ受理)。gateway + per-domain の
  `Dockerfile.*` / per-service deploy は #556 で廃止。
- **手動 `deploy.sh`** もあり (monolith のみ、AR `cloudsql-sv/alc-app` へ)。通常は CI 経由。
- **coverage 100% ガード**: `coverage_100.toml` 登録ファイルは CI でリグレッション検出。mock テストは
  `tests/mock_misc` だけ (`mock_tenko` は W7d で消した)。
- **その他 workflow**: `release-wave.yml` (Release Wave caller、`repository_dispatch` で cloudrun flip を受ける)。
- **CI 速度の tracking**: [`docs/ci-speed-tracking.md`](../../../docs/ci-speed-tracking.md) が SoT
  (Refs #482)。PR CI は同一ソースに対し「coverage 計装 (test-matrix)」と「Bazel fastbuild
  (build-image)」の 2 profile ビルドが走る (プロファイル統合は原理的に不可)。check job の
  test profile 重複ビルド (TS bindings 生成) は #482 で test-matrix(lib) に統合済み。
  Bazel remote cache は健全 (実測済み) — 遅く見えても cache 設定を疑う前にこの doc を読む。

## 関連 skill

- `coverage-test-patterns` — sqlx 向け DB エラー注入 / SSE テスト / 100% 達成パターン
- `coverage-check` — 未カバー行抽出
- `type-safe-pipeline` — フロント (nuxt-*) が rust-alc-api の ts-rs 型を CI 同期する仕組み
- `migrate-test` — Supabase + sqlx migration の splinter/RLS 検証
- `cross-repo-symbol-index` — この per-repo map の鮮度 hook 運用方針

## CLAUDE.md から移設 (2026-07-06)

# rust-alc-api

Axum + PostgreSQL RLS による ALC (アルコールチェック) API バックエンド
<!-- Bazel disk-cache: tar.zst + actions/cache (ippoan/setup-bazel) -->

## プロジェクト構成

- **バックエンド**: Rust / Axum
- **認証**: Google Sign-In JWT + LINE WORKS OAuth2
- **DB**: Supabase PostgreSQL (`alc_api` スキーマ、`alc_api_app` ロール NOBYPASSRLS)
- **ストレージ**: Cloudflare R2 (`alc-face-photos` バケット) / GCS 切り替え可能
- **デプロイ**: Cloud Run (`deploy.sh`)

## DB 接続の重要事項

- Supabase は rust-logi と同じプロジェクト (`tvbjvhvslgdwwlhpkezh`)、`alc_api` スキーマで分離
- `alc_api_app` ユーザーで接続すること（NOBYPASSRLS → RLS が有効）
- monolith (session スコープの `set_current_tenant`) は必ず **直接接続 (port 5432)** を使用（Supavisor port 6543 は set_config がリセットされる）
- worker (`workers/*`) は `in_tenant_tx` でトランザクション単位の RLS (`set_config(..., true)`) にする場合に限り、プーラーの 6543 を使う
- `DATABASE_URL` に `?options=-c search_path=alc_api` を付けてスキーマ指定

### ローカルから psql で直接クエリ

RLS が有効なため、`set_config` でテナントIDを設定しないと行が見えない。

```bash
source .env && DB_BASE=$(echo "$DATABASE_URL" | cut -d'?' -f1)

# テナント一覧確認
psql "$DB_BASE" -c "SELECT id, name FROM alc_api.tenants;"

# テナント指定してクエリ (set_config は同一セッション内で有効)
psql "$DB_BASE" -c "
  SELECT set_config('app.current_tenant_id', '<tenant_id>', false);
  SELECT count(*) FROM alc_api.dtakologs;"

# archive テーブルは archive_mode も必要
psql "$DB_BASE" -c "
  SELECT set_config('app.archive_mode', 'true', false);
  SELECT count(*) FROM alc_api.dtakologs;"

# テーブルサイズ確認 (RLS 不要)
psql "$DB_BASE" -c "
  SELECT tablename,
         pg_size_pretty(pg_total_relation_size('alc_api.' || tablename)) as size
  FROM pg_tables WHERE schemaname = 'alc_api'
  ORDER BY pg_total_relation_size('alc_api.' || tablename) DESC LIMIT 10;"

# VACUUM FULL (DELETE 後のディスク回収、テーブルロックあり)
psql "$DB_BASE" -c "VACUUM FULL alc_api.<table_name>;"
```

- `?options=` パラメータは psql が解釈できないので `cut -d'?' -f1` で除去する
- `logi` スキーマは `alc_api_app` ユーザーでアクセス不可 → Supabase ダッシュボード SQL Editor を使う

## 認証

### 認証方式

| 方式 | 用途 | エンドポイント |
|------|------|--------------|
| Google OAuth | alc-app 管理画面 | `POST /api/auth/google`, `POST /api/auth/google/code` |
| LINE WORKS OAuth2 | nuxt-pwa-carins (車検証管理) | `GET /api/auth/lineworks/redirect`, `GET /api/auth/lineworks/callback` |
| X-Tenant-ID ヘッダー | キオスクモード (デバイス) | ヘッダーのみ、JWT 不要 |
| Refresh Token | トークン更新 | `POST /api/auth/refresh` |

### JWT クレーム

```json
{
  "sub": "UUID (user_id)",
  "email": "user@example.com",
  "name": "ユーザー名",
  "tenant_id": "UUID (テナントID)",
  "role": "admin | viewer | payroll",
  "iat": 1234567890,
  "exp": 1234571490
}
```

- 有効期限: 1時間 (`ACCESS_TOKEN_EXPIRY_SECS = 3600`)
- Refresh Token: 30日 (`REFRESH_TOKEN_EXPIRY_DAYS = 30`)
- 署名: HS256、Secret Manager `JWT_SECRET`

### LINE WORKS OAuth2 フロー

```
ブラウザ → /api/auth/lineworks/redirect?domain=ohishi&redirect_uri=https://...
  → DB: resolve_sso_config('lineworks', 'ohishi') で SSO 設定取得
  → LINE WORKS authorize URL にリダイレクト
  → ユーザー承認
  → /api/auth/lineworks/callback?code=xxx&state=xxx
  → LINE WORKS token exchange → user profile 取得
  → DB: users テーブルに lineworks_id で upsert
  → JWT 発行 → redirect_uri#token=xxx にリダイレクト
```

- SSO 設定は `alc_api.sso_provider_configs` テーブル（テナントごとに client_id/secret を保持）
- `resolve_sso_config()` は SECURITY DEFINER 関数（認証前アクセス用）
- HMAC-SHA256 state パラメータで CSRF 防止（`OAUTH_STATE_SECRET` 環境変数）
- 実装: `src/auth/lineworks.rs`, `src/routes/auth.rs`
- **新規ユーザーの `role` は `'viewer'`** (`crates/alc-core/src/repo/auth.rs` の `create_user_lineworks`、Refs #599。旧 `'admin'` リテラル)。経路別: Google = 招待 (`tenant_allowed_emails.role`) を bind / LINE WORKS = `'viewer'` / LINE = `'viewer'`。この INSERT は `upsert_lineworks_user` が `find_user_by_lineworks_id` で既存行を引けなかった時だけ走る (= 既存ユーザーの role は動かない)。`users.role` の DB 既定値は `'admin'` (`migrations/003_create_users.sql`) なので退行は静かに管理者が増える形でしか出ず、mock は SQL を通らないため実 DB テスト `tests/auth_user_role_test.rs` (bazel-test-db の `db-auth-user-role` shard) で 3 経路とも縛っている
- **`role` の取りうる値は `'admin'` / `'viewer'` / `'payroll'`** (`migrations/131_add_payroll_role.sql` で `users.role` と `tenant_allowed_emails.role` の CHECK を同時に広げた)。**招待側 (`tenant_allowed_emails`) とログイン本体 (`users`) は必ず同じ集合にする** — 片方だけ広げると「招待は通るのにログインで CHECK 違反」になる。`'payroll'` は給与の閲覧可否を決める独立した軸で、上流 (rust-ichibanboshi) の email allowlist と **AND** で効く (role が allowlist を上書きすることはない)。**テナント管理系の権限判定 (`role != "admin"`: `access_requests.rs` / `bot_admin.rs` / `sso_admin.rs` / `tenant_users.rs`。`api_tokens.rs`・`members.rs` は使われていないので Refs #680 で消した) は従来どおりで、`'payroll'` はそこでは 403 のまま** — 給与専用 role が SSO 設定 / bot 設定 / ユーザー管理 / 参加リクエストに触れないのが正しい状態なので緩めない。受け付ける側は `tenant_users::invite_user` の allowlist (旧 `members::is_allowed_role` は消した)。**`employees.role` (`migrations/024` / `028`、driver / manager / admin) は別テーブル・別ドメイン**で、同名の `'admin'` が出てくるが無関係

### ミドルウェア

| ミドルウェア | 用途 | 認証方法 |
|-------------|------|---------|
| `require_jwt` | 管理者ページ | `Authorization: Bearer <jwt>` 必須 |
| `require_tenant` | テナントスコープ操作 | JWT → フォールバック `X-Tenant-ID` ヘッダー |

### テナント統一モデル

- `alc_api.tenants` — `id`, `name`, `slug` (UNIQUE)
- `alc_api.users` — `google_sub` (nullable) + `lineworks_id` (nullable)、どちらか一方は必須 (CHECK 制約)
- rust-logi の Default Organization (`00000000-...0001`) も `tenants` に登録済み

### nuxt-pwa-carins の認証フロー

ログインは auth-worker (Cloudflare Workers) → rust-logi 経由で JWT 発行（`org` クレーム、rust-logi の JWT_SECRET で署名）。
rust-alc-api の JWT_SECRET とは異なるため、nuxt-pwa-carins のサーバープロキシ (`server/api/proxy/[...path].ts`) が:
1. auth-worker JWT の `org` クレームを抽出
2. `X-Tenant-ID` ヘッダーに変換して rust-alc-api に転送（`require_tenant` ミドルウェアのフォールバック）

rust-alc-api にも LINE WORKS OAuth バックエンドを実装済み (`/api/auth/lineworks/redirect`) だが、
現状は auth-worker 経由で十分に動作しており、両バックエンド共通で使える。
auth-worker が発行する JWT の `org` クレームは rust-logi の `organization_id` = rust-alc-api の `tenant_id` なので互換性あり。

### 環境変数（認証関連）

| 変数 | 用途 | 管理先 |
|------|------|--------|
| `JWT_SECRET` | JWT 署名/検証 | Secret Manager |
| `GOOGLE_CLIENT_ID` | Google OAuth | Secret Manager |
| `GOOGLE_CLIENT_SECRET` | Google OAuth code exchange | Secret Manager |
| `OAUTH_STATE_SECRET` | LINE WORKS OAuth state HMAC 署名 | Secret Manager |
| `API_ORIGIN` | LINE WORKS OAuth callback URL のオリジン | 環境変数 |

### 認証の auth-worker 移管 (#434、完了)

**★ 現状 (Refs #680)**: 下の 5〜6 の相手は rust ではなく **alc-auth-worker** (auth-worker の `ALC_AUTH_LOGIN=1`、
`ALC_AUTH` binding)。rust の `/api/internal/auth/*` と `crates/alc-auth/src/internal.rs` は消した。以下は経緯。

LINE / LINE WORKS の OAuth オーケストレーションを **auth-worker に移管**し、rust は
DB プリミティブを `/api/internal/auth/*` (`require_internal_jwt` 配下) で公開する
だけの dumb backend にした。当時の確定アーキ (Refs #434):

```
1. browser ──▶ auth.ippoan.org/oauth/line/redirect        (ログイン開始)
2. auth-worker ──▶ LINE authorize                          (リダイレクト)
3. user 承認 ──▶ auth-worker/oauth/line/callback?code=…
4. auth-worker が LINE と code 交換 → profile 取得          (auth-worker が直接)
5. auth-worker ──OIDC(aud=alc-api-internal)──▶ rust /api/internal/auth/users/…
                                                           ← ユーザー確認/upsert
6. rust ──▶ auth-worker に user 情報(id/tenant/role/slug)  ← rust→auth-worker ユーザー情報
7. auth-worker が JWT を発行(JWT_SECRET で署名)
8. auth-worker が cookie(logi_auth_token=JWT)をセット + redirect_uri#token=… で戻す ← cookie でログイン保持
```

- **2 つの OIDC の使い分け**: ログイン中の internal call は `aud=alc-api-internal`
  (`/api/internal/auth/*`)、ログイン後のデータ API は `aud=service URL`
  (`require_tenant_header`、tenant/user はヘッダ注入)。`/alc-proxy` は service-URL
  audience でしか mint しないため、consumer が `/alc-proxy` 経由で internal route に
  到達しても `aud` 不一致で弾かれる (confused-deputy 防止、Cloud Run custom audiences)。
- rust は cookie も browser JWT も**一切見ない** (dumb backend)。発行・検証・cookie は
  auth-worker が持つ。
- 実装: 旧 `crates/alc-auth/src/internal.rs` (`internal_router`、Refs #680 で削除)。`require_internal_jwt` は
  rls-check・端末の pairing-tenant・notify の内部口で使い続けている。

## ストレージバックエンド切り替え

- `STORAGE_BACKEND=r2` → Cloudflare R2 (`rust-s3` crate)
- `STORAGE_BACKEND=gcs` → GCS (reqwest 直接呼び出し、Cloud Run メタデータサーバー認証)
- `StorageBackend` trait で抽象化 (`src/storage/`)。`upload` / `download` / `exists` / `delete` /
  `presign_get` に加え `list(prefix) -> Vec<ListedObject>` (Refs #205 の 13、
  `crates/alc-core/src/storage.rs`) — R2 LIST だけで `{key, etag}` を取る用途向けで、
  デフォルトは未対応エラー、`R2Backend` / `MockStorage` (テスト用) だけ実装

## シンボリックリンク（参照用）

プロジェクトルートに関連リポジトリへのシンボリックリンクを配置している。
`.gitignore` に登録済み。VSCode の `git.scanRepositories` で git 操作可能。

| リンク名 | リンク先 | 説明 |
|---|---|---|
| `alc-app` | `/home/yhonda/js/alc-app` | フロントエンド ALC (Nuxt) |
| `front/nuxt-pwa-carins` | `/home/yhonda/js/nuxt-pwa-carins` | フロントエンド 車検証管理 (Nuxt PWA) |
| `workers/auth-worker` | `/home/yhonda/js/auth-worker` | JWT 認証 (Cloudflare Workers) |
| `rust-nfc-bridge` | `/home/yhonda/rust/rust-nfc-bridge` | NFC ブリッジ (Rust) |
| `ble-medical-gateway` | `/home/yhonda/arduino/ble-medical-gateway` | BLE Medical Gateway (Arduino) |

## ユーティリティ

- `git-status-all.sh` — 自身 + シンボリックリンク先の全リポジトリの git status を一括表示

## テスト

### 概要

- **ユニットテスト**: `cargo test --lib` (DB 不要)
- **インテグレーションテスト**: `tests/` ディレクトリ (ローカル PostgreSQL が必要)
- **マイグレーション検証**: ローカル DB + splinter (Supabase Postgres Linter)
- **カバレッジ**: `cargo llvm-cov`
- **統合スクリプト**: `./test_and_deploy.sh` (fmt → clippy → unit → migration → integration → frontend)

### テスト実行

```bash
# ローカル開発 (推奨: Makefile 経由)
make test                     # ユニットテストのみ (DB 不要、高速)
make test-file F=jwt          # 特定モジュールのみ
make db-up                    # テスト DB 起動 (セッション中1回)
make itest-file T=auth_test   # 特定インテグレーションテスト
make itest                    # 全インテグレーションテスト (DB 起動→テスト→停止)
make db-down                  # テスト DB 停止

# カバレッジ検証 (100% ファイルのリグレッション検出)
make cov-check-unit           # unit ファイルのみ (DB 不要)
make cov-check                # 全 100% ファイル (DB 必要)

# CI カバレッジ取得 (artifact からダウンロード、ローカル実行不要)
make cov-not100               # 未達成ファイル一覧
make cov-summary              # 全ファイルサマリ
make cov-file F=devices       # 特定ファイルの未カバー行

# 全テスト一括 (fmt + clippy + unit + migration + integration + frontend)
./test_and_deploy.sh

# テスト + デプロイ
./test_and_deploy.sh --deploy

# オプション
./test_and_deploy.sh --skip-integration   # インテグレーションテストをスキップ
./test_and_deploy.sh --skip-frontend      # フロントエンドテストをスキップ
```

### CI/CD

- **GitHub Actions**: `.github/workflows/ci.yml` (push/PR to main)
  - `check`: fmt + clippy
  - `unit-tests`: `cargo test --lib` + 100% カバレッジ検証 (unit ファイル)
  - `integration-tests`: PostgreSQL サービスコンテナ + 全テスト + 100% カバレッジ検証 + **artifact アップロード**
- **カバレッジ手動実行**: `.github/workflows/coverage.yml` (workflow_dispatch)
  - 3モード: `summary` / `not-100` / `file` → Job Summary にマークダウン出力
  - artifact (`llvm-cov-text`) も同時にアップロード (30日保持)
- **100% ファイル登録簿**: `coverage_100.toml` — CI でリグレッション検出
- **CI artifact 取得**: `make cov-not100` / `make cov-summary` / `make cov-file F=xxx` — `gh run download` で最新 artifact をダウンロードしてローカル解析
- **CI ビルド時間の実測・改善履歴**: [`docs/ci-speed-tracking.md`](./docs/ci-speed-tracking.md) — CI 高速化施策の追加/revert 前に必読 (external cache 等、実測済みの失敗パターンあり)。実施後は実測を追記する

### カバレッジ

```bash
# サマリ (ユニットテストのみ)
cargo llvm-cov --lib --summary-only

# インテグレーション込み (要 docker compose up)
TEST_DATABASE_URL="postgresql://postgres:test@localhost:54322/postgres?options=-c search_path=alc_api" \
  cargo llvm-cov --summary-only

# HTML レポート
TEST_DATABASE_URL="..." cargo llvm-cov --html --open
```

### テスト構成

| ファイル | 内容 |
|---------|------|
| `tests/common/mod.rs` | テストハーネス (DB 接続、サーバー起動、JWT 発行ヘルパー)。migration は `migrate_and_grant` に 1 本化 (`alc_migrations::MIGRATOR` の後に `alc_migrations::LOCAL_APP_GRANTS` を流す。DB を直接開くテストも必ずこれを通す) |
| `tests/app_role_grants_test.rs` | テスト DB の `alc_api_app` の権限が本番と揃っているか (Refs #685) — `alc_api` の表のうち `has_table_privilege('alc_api_app', 表, 'SELECT')` が false のものを列挙し 0 件を assert。新しい表の GRANT 付け忘れ (過去に本番 502) を落とす。直すのは migration 側 (bazel `db-app-role-grants` shard)。**実行用ロール `alc_api_rt` (alc-migrations 158) も同じファイルで検査する**: superuser・BYPASSRLS でなく表を所有していない / schema の USAGE / `_sqlx_migrations` 以外の全表に SELECT・INSERT・UPDATE・DELETE / `_sqlx_migrations` には権限が無い / SECURITY DEFINER 関数を全部呼べる。`alc_api_rt` の権限は `scripts/local_app_grants.sql` に写さない (158 の付け漏れを検出できなくなる) |
| `tests/runtime_role_test.rs` | 実行用ロール `alc_api_rt` で繋いだサーバで、認証前・テナント横断の経路が通ること (alc-migrations 158、Refs ippoan/alc-app#387) — 陰性 (tenant 無しで `users` を読むとエラー・別テナントを立てると 0 行・招待は 0 行) / ログイン 3 経路 (Google は招待 → 作成 → 招待の消費、LINE WORKS、LINE。それぞれ新規と既存、最後に refresh token の保存) / 招待も email_domain も無い Google は 403 / refresh token は渡した user の行だけ / 端末の登録要求 → 承認 → 承認後のポーリングで `settings_token` → `pairing-tenant` と `fcm-dismiss-test` (無効にした端末・未登録は 404) / 内部用 secret 付きの `fcm-notify-call`・`test-fcm-all-exclude`・`trigger-update-dev` / 超過検知の 1 tick (通知済みの印と配信の記録) / 参加の申請の作成 → 一覧 → 承認・却下 / 内部用の口 `GET /api/internal/rls-check` (Refs ippoan/auth-worker#605。実行用ロールで全部合格・応答の key が契約ちょうど・`invariants.checks` が `RLS_INVARIANT_CHECKS` の番号 (0〜9) の順で全部 0・`state` の最上位の key が 6 個で組ごとの表の数の合計 = `table_count`・`drift.matches_expected` が true で `verdicts` の 4 つが true / 認可なし・不正な Bearer は 401 / query を付けても同じ応答 / `begin_read_only` の transaction で INSERT が 25006 / `ALTER ROLE alc_api_rt BYPASSRLS` で `ok=false` と検査 4 の違反・`verdicts.invariants` と `verdicts.runtime_role` が false → 戻す / 表 1 つに `FORCE ROW LEVEL SECURITY` を付けると `drift.tables` にその表だけが載り `verdicts.drift` と `ok` が false、`verdicts.invariants` は true のまま → 戻す)。実行用ロールの属性・表の FORCE を変えるテストは `ROLE_ATTRS` を write で取り、ほかは `setup()` が read で取る。bazel `db-runtime-role` shard |
| `tests/common/mock_storage.rs` | インメモリ StorageBackend 実装 |
| `tests/auth_test.rs` | JWT 認証 / X-Tenant-ID / 未認証拒否 |
| `tests/normal_tenko_record_test.rs` | 通常点呼・IT点呼の行 (worker が書くのと同じ形を `common::insert_normal_flow_tenko` で直接入れる) を rust の点呼の口が扱えること — 執行者 NULL の記録があっても記録の一覧・CSV・セッションの一覧が取れる / スケジュール無し (遠隔点呼) の session は '通常点呼' ではないため、同じ測定を submit_alcohol に付けても unique に当たらない (Refs #655) / IT点呼 の session に運行管理者の判定 (`POST /tenko/sessions/{id}/judgment`) が入り GET に出る / 判定の確認の方法 (`it` / `in_person`) の検査と押し直し / 未完了の IT点呼 の一覧 (filter・ページング) / CSV が対面で確定した IT点呼 を `対面点呼` と読む。行を作る側 (冪等・車検証・本人確認の方法) は alc-measurements-worker の `normal_tenko_db.rs` |
| `tests/tenant_scoped_queries_test.rs` | tenant を渡して叩く query が別テナントの id では 0 行 / 影響なしになること (Refs ippoan/alc-app#387) — 招待の消費・logout の refresh token 消去・LINE 通知先の自動登録 (auth) / ユーザーと招待の一覧・削除・role 変更 (tenant_users) / QR 永久のクレームと FCM トークン (devices) / 超過検知の従業員名・通知済みの印と点呼用電話番号の削除 (tenko) / 参加申請の一覧・承認・却下 (HTTP 経由、別テナントの管理者は 404)。bazel `db-tenant-scoped-queries` shard |
| `tests/tenko_bp_required_escalate_test.rs` | 自動点呼タブの血圧必須化 + 遠隔点呼への切り替え (migration 144、Refs ippoan/alc-app-s3#135) — 自動点呼 (`tenko_method` 既定) は血圧 (最高・最低) なしで 400 / ありで 200 / 通常点呼は血圧なしでも 200 (退行がないこと、`tenko_method` を直接書き換えて分岐だけ確かめる — 実運用の通常点呼作成経路は即 completed になり medical_pending を経由できないため) / `escalate-remote` (`PUT .../escalate-remote`) で `tenko_method` が '遠隔点呼' に変わり、切り替え時刻・理由が応答と DB の両方に残る / 切り替え後は血圧なしで 200 / 切り替えても cancel までの記録は作られず、完了後の `tenko_records` は 1 件だけで `tenko_method` が '自動点呼' に化けていない (144 の `create_tenko_record` 修正の固定) / 理由が空文字は 400 / 200 文字超も 400 (切り詰めない) / 通常点呼のセッションは切り替えられない 400 (別経路の振る舞いを変えない) / 既に遠隔点呼へ切り替え済みのセッションは再度切り替えられない 400 (二重にならない) / **start_session のスケジュール無し経路は `tenko_method='遠隔点呼'` で作られ (Refs #655)、血圧を含まない体温だけの submit_medical が 400 にならず成功する** (症状の固定) / 回帰: スケジュールありの開始は従来どおり `tenko_method='自動点呼'` で、血圧の無い体温提出は従来どおり 400 |
| `tests/device_bp_enabled_test.rs` | `devices.bp_enabled` (migration 143) の既定値・GET/PUT 往復に加え (Refs ippoan/alc-app-s3#135)、`submit_medical` の血圧必須判定が `bp_enabled` を正本にすることを固定 (Refs ippoan/alc-app#322) — `bp_enabled=false` の端末は自動点呼でも血圧なしで 200 (症状の固定) / `bp_enabled=true` の端末は従来どおり血圧なしを 400 で弾く (回帰) / 自動点呼でなければ device_id の有無に関わらず血圧なしで 200 |
| `tests/dtako_operation_changes_test.rs` | 運行の変更記録 (migration 152、Refs ohishi-exp/nuxt-dtako-admin#1133) — 手動削除は crew_role ごとに manual_delete・after=NULL / 読み口は別 tenant を返さない (上げ直しの記録は ippoan/alc-dtako-worker が持ち、前準備の行は SQL で直に入れる) |
| `tests/dtako_driver_dedup_test.rs` | 乗務員マスタ同期 `upsert_by_code` の復活時ガードと migration 150 の部分一意 index (Refs #669 / #673)。取り込み側の乗務員解決は worker 側。BUILD.bazel と ci.yml の `db-dtako-driver-dedup` で走る |

### テスト用インフラ

- `docker-compose.yml` — テスト用 PostgreSQL 16 (ポート 54322、tmpfs)
- **テスト DB の初期化・GRANT はテスト自身が行う** — `tests/common` の `migrate_and_grant` が、空の DB なら (`alc_api` schema が無ければ) `alc_migrations::INIT_LOCAL_DB` (schema + `alc_api_app` + Supabase 互換ロール。冪等でないので存在確認してから) → `MIGRATOR` → `LOCAL_APP_GRANTS` を advisory lock の下で流す。CI もローカルも空の postgres に繋ぐだけ (`psql -f` の段は無い)。SQL の正本は ippoan/alc-migrations の `scripts/init_local_db.sql`・`scripts/local_app_grants.sql` (この repo の写しは消した)
- `LOCAL_APP_GRANTS` — 本番 (Supabase) の `alc_api_app` の GRANT (表・sequence・function) の写し。表は migration が作るので init には書けず、migration の後に流す (2026-09-28 取得、表 85 / 関数 20 / sequence 12。**本番の実態は migration の GRANT より広い** — 全表 ALL 相当で、`dtako_operation_changes`・`_sqlx_migrations` も。本番側を絞るかは別件。冪等の DO ブロックで、ローカルに無い対象は存在確認して飛ばし NOTICE に出す。関数は型だけで引く。`ON ALL TABLES` では書かず本番の一覧をそのまま写す)。新しい表の GRANT はここでなく migration に書く
- `.test-config` — `test_and_deploy.sh` 共通スクリプトの設定

### マイグレーション作成時の注意

- **適用済みのマイグレーションファイルは絶対に変更しない** — SQLx は SHA-384 チェックサムで検証し、不一致だとアプリが起動不能になる。修正が必要な場合は新しいマイグレーションファイルを追加する
- 本番に既存データへの INSERT/UPDATE をハードコードしない (`WHERE EXISTS` で条件付きにする)
- `SECURITY DEFINER` 関数には `SET search_path = alc_api` を付けること (splinter 警告回避)
- RLS ポリシーの `WITH CHECK (true)` は避け、明示的な条件を使う

### マイグレーション含む PR の検証方針

**マイグレーション (`migrations/*.sql` 追加・変更) を含む PR は、ローカルで流さず、PR の CI + staging 自動デプロイに検証を任せる**:

- ローカル実行はリソース消費が大きく、CI と同じ環境ではない
- staging 自動デプロイで実 DB に対する適用結果を確認する方が信頼できる
- PR 内の検証順序:
  1. `cargo fmt` + `cargo check` のみローカル実行
  2. push → CI で `migrate-test` (splinter / RLS) ジョブ実行
  3. CI 通過後、staging 自動デプロイで実マイグレーションが走ることを確認
  4. staging 環境で対象機能の E2E 確認 (export/import 等で再現可能なテストデータを用意)

**ローカル実行を許可する例外**: SQL 構文確認のために手元 docker postgres に対して `psql -f` で叩く程度はOK。
splinter / RLS 検証フルセットは CI に集約する。

## マイグレーションとデプロイ

- **migration の正本は ippoan/alc-migrations に移行済み (rust-alc-api#697)**。`src/bin/migrate.rs` (本番) と `tests/common` (テスト) は `alc_migrations::MIGRATOR` / `LOCAL_APP_GRANTS` を読む (git 依存、`Cargo.toml` の rev 固定。新しい migration は rev を上げて取り込む)。この repo の `migrations/`・`scripts/init_local_db.sql`・`scripts/local_app_grants.sql`・`build.rs`・`migration-safety-check.yml` は消した (152 番で止めた写しが CI と staging の init に残っていて、rev を 174 以上に上げると migration 174 と衝突していた)。足し直しは CI `pr-limit` の Freeze step が止める
- 本番の migration は **ippoan/alc-migrations の migrate.yml (手動)** が、Actions の runner から org secret `ALC_MIGRATE_DATABASE_URL` で直接 DB に繋いで流す (GCP / Cloud Run job は使わない)。rust-alc-api からは流さない
- tag release (`deploy.yml`) の `migrations-applied` job が deploy の前に確かめる: `Cargo.toml` の `alc-migrations` の固定 rev で `alc-migrate` を `cargo install` し、同じ secret で `alc-migrate --check` (読むだけ。その rev の migration が 1 件でも未適用なら exit 1)。rev を上げた PR は、先に alc-migrations 側で本番に流してからでないと tag release が止まる。落ちたら alc-migrations の migrate.yml を main で手動実行してから再実行。`ALC_MIGRATE_DATABASE_URL` は `ci.yml` の deploy job から `deploy.yml` へ渡す
- `src/bin/migrate.rs` は staging の起動時 migrate (`staging/entrypoint.sh`、使い捨て DB) 用に残る。空の DB (`alc_api` schema が無い) なら先に `alc_migrations::INIT_LOCAL_DB` を流す — staging の postgres sidecar (`staging/Dockerfile.postgres`) は空の postgres だけで、SQL を焼き込まない。その image は commit の SHA のタグで push し (`ci.yml` の staging-db job)、`deploy.yml` が `inputs.image_sha` で引く (可変の `latest` は Artifact Registry の remote のキャッシュが古い digest を返し続けたため)。本番 `Dockerfile` も、他 repo の結合テスト (docker-compose が backend image 内の migrate を起動する) のために COPY する。本番では起動しない
- `deploy.sh` の流れ: Docker ビルド → プッシュ → Cloud Run Service デプロイ (migrate は含まない)
- 未適用 migration が在るとき tag release は止まり、アプリは前バージョンで動き続ける
- `main.rs` からは `sqlx::migrate!()` を削除済み（起動時の自動適用はしない）

## 車検証管理 (carins) 機能

rust-logi から移行。nuxt-pwa-carins フロントエンドが使用。

### テーブル（`alc_api` スキーマ、元 `logi` から移動）

- `car_inspection` — 車検証データ（102フィールド、PascalCase カラム名）
- `car_inspection_files` / `_files_a` / `_files_b` — 車検証ファイル紐づけ
- `car_inspection_deregistration` / `_deregistration_files` — 抹消登録
- `car_inspection_nfc_tags` — NFC UUID ↔ 車検証 ID マッピング
- `files` / `files_append` — ファイルメタデータ（実体は GCS `rust-logi-files` バケット）
- `file_access_logs` — アクセス統計
- `pending_car_inspection_pdfs` — PDF 処理キュー

### REST エンドポイント

| ファイル | エンドポイント |
|---------|-------------|
| `routes/car_inspections.rs` | `GET /api/car-inspections/current`, `/expired`, `/renew`, `/{id}` / `POST /api/car-inspections/lookup` |
| `routes/car_inspection_files.rs` | `GET /api/car-inspection-files/current` |
| `routes/carins_files.rs` | `GET/POST /api/files`, `/recent`, `/not-attached`, `/{uuid}`, `/{uuid}/download`, `/{uuid}/delete`, `/{uuid}/restore` |
| `routes/nfc_tags.rs` | `GET/POST /api/nfc-tags`, `/search?uuid=`, `DELETE /{nfc_uuid}` |

### 注意事項

- `car_inspection` テーブルのカラム名は **PascalCase**（`EntryNoCarNo` 等）
- REST API は `to_jsonb()` で DB カラム名をそのまま JSON キーとして返す（フロントエンドが PascalCase を期待するため）
- RLS ポリシーは `COALESCE(current_tenant_id, current_organization_id)` で rust-logi からもアクセス可能（移行期間中）
- ファイルストレージは GCS バケット `rust-logi-files`（パス: `{tenant_id}/{uuid}`）

## タイムカード機能

- **テーブル**: `timecard_cards` (カード:社員 = 多:1)。**`employee_id` は NULL 可** (migration 146、Refs ippoan/rust-alc-api#644) — 社員マスタがまだ同期されていないカードは 持ち主が空で入り、社員番号が `pending_employee_code` に残る (結び付いたら NULL に戻る)。**打刻の行を持つ表は `hub_measurements` だけ** (Refs ippoan/alc-app-s3#134 で一次表をそちらへ寄せた)。旧・打刻表は書き手 (#615 / #618) も読み手 (#616) も無くなったので **#620 で DROP 済み** — 過去の行も残っていない (賃金の監査証跡として残す必要は無い、というオーナー判断)
- **マイグレーション**: `migrations/034_create_timecard.sql` + `migrations/134_timecard_cards_normalize_card_id.sql` (card_id の正規化移行 + CHECK 制約) + `migrations/138_delete_prehistoric_timecard_punches.sql` (1970 年の打刻を削除) + `migrations/139_drop_time_punches.sql` (残置表を DROP) + `migrations/145_timecard_cards_source.sql` (`source` 列の追加 + 初回移行ぶんの backfill。件数を `RAISE NOTICE`、想定超過で `RAISE EXCEPTION` — 138 と同じ形) + `migrations/146_timecard_cards_pending_employee.sql` (`employee_id` の NOT NULL 解除 + `pending_employee_code` 列 + 部分 index `idx_timecard_cards_pending_code`。**`CHECK` 制約は足さない** — 「社員に結び付いていないカード」の拒否は端末 (CoreS3) がその場でやる話で、DB で弾いても呼び出し元には分かりにくい 500 しか返らない)
- **★ `recorded_at` が 1970 になるのは時計未同期の端末が稼働時間を送ったため (Refs ippoan/alc-app-s3#144)**。NFC タイムカード端末は SNTP 起動より前、`recorded_at_ms` に**起動からの稼働時間**を入れていたので 1970 起点になる。送信時に稼働時間の差で補正する仕組み (ippoan/alc-app-s3#124) は「あとで同期したら過去ぶんを補正する」形で、**1 度も同期しなかった端末の行には永久に発火しない**。一覧の絞り込みと並びは `COALESCE(recorded_at, created_at)` なので、これらは 1970 年の打刻として並ぶ。migration 138 が `kind='timecard' AND recorded_at < 2020-01-01 AND created_at < 2026-09-05` の 3 条件で削除した (賃金データなのでオーナー判断で削除)。**device_id / tenant_id はハードコードしない** (public repo に本番の device credential を残さない / device_id は全体一意ではない — 138 の時点の unique は migration 126 の `(tenant_id, device_id, seq)`、いまは alc-migrations 155 の 4 列 index `(tenant_id, device_id, seq, is_dev)` だけで、3 列は 156 で落とした)、**`created_at` の上限が「測定時点で既にあった行だけ」を意味する**、**`recorded_at IS NULL` と他 kind は巻き込まない**。20 件超で `RAISE EXCEPTION` する guard 付き — 発火すると `_sqlx_migrations` に dirty 行が残るので、消してから新 version で作り直す
- **card_id の正規化 (Refs ippoan/alc-app-s3#134)**: (今は ippoan/alc-people-worker の `timecard::normalize_card_id`。以下はこの repo に在った頃の経緯) `normalize_card_id` = `trim` + **小文字** + `':'` 除去。同じ物理カードでも読み取り側で表記が揺れる (NFC タイムカード端末は `%02X` の大文字 IDm、ローカル NFC ブリッジは小文字、`AA:BB:..` と区切る実装もある) のに照合は完全一致なので、**登録・照合・登録照会の 3 経路すべて**を同じ形に揃える: 登録 = `create_card`、照合 = `resolve_employee_by_card` (ブラウザ版 punch と端末中継の共有 choke point)、登録照会 = `get_card_by_card_id` (choke point を通らない 3 本目)。**小文字**なのは `alc-carins` の `normalize_nfc_uuid` (車検証 NFC タグ) と規約を揃えるため — 1 repo に NFC ID の正規化規約を 2 つ並べない。**読み側だけ正規化してはいけない**: `ABC` と `abc` の 2 行が同時に存在し得ると正規化後の値がどちらにも一致して**打刻が別人に着く**。migration 134 が既存行を移行し、CHECK 制約 `timecard_cards_card_id_normalized` が書き忘れを loud fail させる (既存の `idx_timecard_cards_unique (tenant_id, card_id)` がそのまま正規化後の一意性を保証するので index は増やさない)。`employees.nfc_id` フォールバック (免許証 16 桁の数字) に対しては no-op
- **バックエンド**: **タイムカードの口 (`/api/timecard/...`: カード CRUD・`bulk-by-code`・`delete-by-card`・`lookup`・打刻の一覧/CSV) は ippoan/alc-people-worker へ移り、この repo から消した** (W4d-a。auth-worker v0.2.337 の `ALC_BINDING_ROUTES` で画面用・端末用・RPC の 3 経路とも worker に回る。Refs #680)。挙動 (一括取り込みの `ON CONFLICT`・削除の射程 `source`・照会の 404 の分け方・打刻の CTE) は worker の `crates/people/src/timecard.rs` に写してあり、直すときはそちらへ PR を出す。rust の `POST /api/timecard/punch` は使われていなかったので移さず消した (W4d-0)。
  - **カードの照合 (`TimecardRepository`・`resolve_employee_by_card`) も消した** (W6a)。打刻の取り込み (`POST /api/hub/measurements`) が ippoan/alc-people-worker へ移り、照合はそちらの `hub::freeze_employee_ids` (カードの照会と同じ SQL)。この repo にタイムカードのコードは無い
  - 社員マスタ同期 (`PUT /api/employees/bulk-by-code`) が保留カードを結ぶ `link_pending_cards` も W4d-b で worker へ移った (保留行 = `employee_id IS NULL` + `pending_employee_code`)
- **テスト**: alc-people-worker の `tests/timecard_db.rs`・`tests/hub_db.rs`
- **★ `coverage_100.toml` 登録ファイルに `match` を足したら分岐テストが要る。** 式に埋めるだけ (`foo(&normalize(x))` のような呼び出しの追加) なら分岐は増えないので不要 — **この区別を取り違えると gate だけが落ちる**。2026-09-04 に `csv_kind_label` の `match` で実際に踏んだ (直前の PR が「式に埋めただけ」だったのを、そのまま次にも当てはめた)
- **★ 実 DB テストは `BUILD.bazel` に `rust_test` を登録しないと CI で 1 度も走らない。** CI のテストは Bazel のターゲット単位なので、`tests/` にファイルを置いて `cargo test` が通っても**CI では 0 件**。`scripts/check_bazel_test_matrix.sh` は「ターゲットと matrix の対応」を見る検査なので、**ターゲット自体が無い状態は検知できない** (28/28 で整合してしまう)。登録は 3 か所 — `BUILD.bazel` の `rust_test` / `.github/workflows/ci.yml` の `bazel-test-db` matrix / `cache-warm.yml` の同 matrix (後の 2 つは pair 一致が検査される)。**2026-09-04 に `timecard_punches_test` で実際に踏んだ** (「テストが実行されないまま本番に出る」を防ぐために足したテストが、それ自身で起きていた)
- **フロントエンド**:
  - `TimePunchKiosk.vue` — 運行者タブ「タイムカード」(NFCタップ→打刻→当日一覧5秒表示)
  - `TimecardManager.vue` — 管理者ダッシュボード「タイムカード」(カード登録 + 打刻履歴 + CSV出力)
- **NFC**: `useNfcWebSocket()` の `onRead` で取得した card_id を `timecard_cards.card_id` と突合

## デバイス登録機能

Google OAuth 以外の端末登録フローを3種類サポート。

### 登録フロー

| フロー | 流れ | 承認 | 有効期限 |
|---|---|---|---|
| QR一時 | 端末がQR表示 → 管理者スマホでスキャン → 即承認 | 不要 | 10分 |
| QR永久 | 管理者がQR生成(PDF印刷可) → 端末がスキャン/コード入力 → 管理者が承認 | 必要 | なし |
| URL | 管理者がURL生成 → 端末に共有(LINE等) → 端末がデバイス名入力 → 即登録 | 不要 | 24時間 |

### テーブル

- `devices` — 登録済みデバイス (tenant_id, device_name, device_type, phone_number, user_id(任意), status)
- `device_registration_requests` — 登録リクエスト (registration_code, flow_type, tenant_id, status, expires_at)
- RLS: `devices` はテナントスコープ、`device_registration_requests` は SELECT/INSERT パブリック (端末側認証不要)

### マイグレーション

- `migrations/035_create_devices.sql`

### バックエンド (`src/routes/devices.rs`)

- **public_router()** (認証不要):
  - `POST /devices/register/request` — QR一時コード生成 (端末側)
  - `GET /devices/register/status/{code}` — ステータス確認 (ポーリング用)
  - `POST /devices/register/claim` — URL/QR永久の登録申請 (端末側)
- **tenant_router()** (管理者認証):
  - `GET /devices` — デバイス一覧
  - `GET /devices/pending` — 承認待ちリクエスト一覧
  - `POST /devices/register/create-token` — URLフロー用トークン生成
  - `POST /devices/register/create-permanent-qr` — QR永久コード生成
  - `POST /devices/approve/{id}` — 承認 (テナント内)
  - `POST /devices/approve-by-code/{code}` — コードで直接承認 (QR一時用、tenant_id NULL 対応)
  - `POST /devices/reject/{id}`, `POST /devices/disable/{id}`, `POST /devices/enable/{id}`, `DELETE /devices/{id}`

### フロントエンド

- `DeviceRegistration.vue` — 端末側: QR一時コード表示 + ポーリング + Google OAuthフォールバック
- `DeviceRegistrationManager.vue` — 管理者: URL生成 + QR永久生成(PDF) + 承認待ち + デバイス一覧管理
- `pages/device-claim.vue` — URL/QR永久の端末登録ページ (`/device-claim?token=<code>`)
- `pages/device-approve.vue` — QR一時の承認ページ (`/device-approve?code=<code>`)
- `AdminDashboard.vue` + `ManagerDashboard.vue` に「デバイス管理」タブ追加

### 端末側アクティベーション

- `useAuth.ts`: localStorage に `tenant_id` + `device_id` を保存
- `activateDevice(tenantId, deviceId)` / `deactivateDevice()` / `isDeviceActivated`

## 中間点呼 (TenkoCall) 機能

運転者が電話番号で登録し、GPS位置情報付きで中間点呼を実施する機能。

### テーブル

- `tenko_call_numbers` — 電話番号マスタ (call_number UNIQUE, tenant_id, label)
- `tenko_call_drivers` — 登録運転者 (phone_number UNIQUE, driver_name, call_number, employee_code, tenant_id)
- `tenko_call_logs` — 点呼ログ (driver_id FK, phone_number, driver_name, latitude, longitude)
- RLS: `tenko_call_numbers` / `tenko_call_drivers` は SELECT パブリック (認証前の検索用)、write はテナントスコープ

### マイグレーション

- `migrations/030_tenko_call_drivers.sql` — drivers + logs テーブル
- `migrations/031_tenko_call_numbers.sql` — 電話番号マスタ
- `migrations/032_tenko_call_rls.sql` — RLS ポリシー
- `migrations/033_tenko_call_employee_code.sql` — employee_code 追加

### バックエンド (`src/routes/tenko_call.rs`)

- **public_router()** (認証不要):
  - `POST /api/tenko-call/register` — 運転者登録 (call_number でマスタ検証 → phone_number で upsert)
  - `POST /api/tenko-call/tenko` — 点呼実施 (phone_number → driver 特定 → GPS ログ記録 → call_number 返却)
- **tenant_router()** (管理者認証):
  - `GET /api/tenko-call/numbers` — 電話番号マスタ一覧
  - `POST /api/tenko-call/numbers` — 電話番号追加
  - `DELETE /api/tenko-call/numbers/{id}` — 電話番号削除
  - `GET /api/tenko-call/drivers` — 登録運転者一覧

### フロントエンド

- `TenkoCallManager.vue` — 管理者: 電話番号管理 + QRコード生成
- `AdminDashboard.vue` に「中間点呼」タブ追加
- `EmployeeList.vue` — 乗務員一覧に中間点呼登録状況 (電話番号) を表示

## 顔認証

- **ライブラリ**: `@vladmandic/human` (BlazeFace 検出 + FaceRes embedding, 1024次元)
- **入力正規化**: 映像フレームを 640x480 キャンバスにレターボックス描画してから Human.js に渡す（デバイス間の解像度差異を吸収）
- **モデルバージョン管理**: `FACE_MODEL_VERSION` 定数 (`useFaceDetection.ts`) でモデル+正規化パラメータを識別。DB (`employees.face_model_version`) と IndexedDB (`FaceRecord.modelVersion`) に記録
- **バージョン不一致時**: 旧バージョンの embedding は認証時にフィルタされ、再登録が促される
- **閾値**: cosine similarity >= 0.55 (`useFaceAuth.ts`)
- **マイグレーション**: `037_add_face_model_version.sql`
- **関連ファイル**:
  - バックエンド: 顔の登録・承認・`face-data` の口は ippoan/alc-people-worker (`crates/people/src/employees.rs`)。この repo には `alc_core::models::Employee` だけ残る
  - フロント: `useFaceDetection.ts`, `useFaceAuth.ts`, `useFaceSync.ts`, `face-db.ts`, `FaceAuth.vue`

## AlcoholChecker Android アプリ

- パス: `/home/yhonda/android/AlcoholChecker/`
- ビルド: `cd /home/yhonda/android/AlcoholChecker && ./gradlew installDebug`
- **署名不一致エラー**: 端末にリリース署名のAPKがある場合、デバッグビルドを上書きインストールできない。`adb uninstall com.example.alcoholchecker` してから再インストールすること
- 複数 adb 接続時は `-s <device>` を指定（WiFi + ワイヤレスデバッグで2重接続になることがある）
- **バージョニング**: 明示的に指示があるまでパッチバージョン (x.y.Z) で上げること。メジャー・マイナーはユーザー指示時のみ
- **リリース**: `master` ブランチに push + `versionName` 変更で CI が自動ビルド・GitHub Release・GitHub Pages デプロイ

## 既知の RLS / 権限問題

いずれも修正済み。

- **devices テーブル SELECT ポリシー**: `migrations/063_fix_devices_select_policy.sql` で `USING(true)` を DROP し、SECURITY DEFINER 関数 (`lookup_device_tenant`, `get_device_settings_by_id`) に置換済み
- **tenko_call_numbers INSERT/DELETE 権限**: `migrations/064_fix_tenko_call_numbers_grants.sql` で GRANT 追加済み

## LINE 配信 (alc-notify) 仕様とデバッグ

### Channel Access Token v2.1 — JWT assertion の落とし穴

2026-04-22 に LINE push 全体が静かに失敗していた (PR #265/#266)。原因は `alc_notify::clients::line::JwtClaims` の仕様誤解。同じ罠を避けるため:

| claim | 型 | 意味 | 範囲 |
|---|---|---|---|
| `exp` | UNIX 秒 (絶対時刻) | JWT assertion の有効期限 | `now` 以降、最大 30分先 |
| `token_exp` | **秒数 (duration)** | 発行される access token の有効期間 | 30s 〜 30日 (2592000) |

- `token_exp` は **duration** (例: `86400` = 1日、`60*60*24*30` = 30日)。`now + 86400` のような絶対時刻を渡すと LINE が `{"error":"invalid_client","error_description":"Invalid token_exp"}` を返す。
- 両 claim を落とすと `"JWT payload must contain token_exp"`。
- 公式仕様: https://developers.line.biz/ja/docs/messaging-api/generate-json-web-token/

### ローカル検証 (リリース不要)

LINE 送信系のバグはリリース → 本番試験だと 1 サイクル 〜5分かかる。これを避けるため example バイナリがある:

```bash
export LINE_CHANNEL_ID=1234567890
export LINE_KEY_ID=<uuid>
export LINE_PRIVATE_KEY_PATH=/path/to/private.pem  # PKCS#1/PKCS#8 どちらも可
cargo run -p alc-notify --example line_token_issue

# push まで試す場合
cargo run -p alc-notify --example line_token_issue -- --push Uxxxxxx "hello"
```

DB 非依存、本物の LINE API を直接叩く。`invalid_client` 等は即時に stderr に出る。秘密鍵は Secret Manager (`google-ai-studio/line-*` or 本番環境) から `gcloud secrets versions access` で取得して PEM に保存。

### 静かな失敗の検知

LINE push は backend 内で `failed += 1` するだけでテナント管理画面には「失敗 1」としか出ない。根本原因は Cloud Run ログのみに出る:

```bash
gcloud logging read \
  'resource.type="cloud_run_revision" AND resource.labels.service_name="rust-alc-api" AND textPayload:"LINE token"' \
  --project cloudsql-sv --limit 10 --freshness=15m
```

`alc_notify::clients::line` / `alc_notify::line_webhook` から ERROR が出ていたら LINE 側で rejected されている。

### カバレッジガード

`crates/alc-notify/src/clients/line.rs` は 100% に登録済み (`coverage_100.toml`, `unit` 型)。テストは wiremock で token/push エンドポイントをモックし、`build_jwt_assertion` の claim 値 (`exp` は絶対、`token_exp` は `[30, 2592000]` のレンジ) も直接 assertion する。**JWT 仕様回帰はこの CI で捕まる。**

## 外部 API 連携の開発フロー

LINE / LINE WORKS / FCM / e-Gov など外部 API を叩くコードは、バグをリリース → 本番試験で発見すると 1 サイクル数分かかる。今回の LINE 修正 (PR #265/#266/#267) で確立したループを今後も踏襲する:

### 1. ローカルで試す (example バイナリ)

新しい外部 API エンドポイントを呼ぶ時は、まず `crates/<crate>/examples/<api>_*.rs` を作って手元で直接叩く。

- DB 非依存、秘密は env + Secret Manager から手動取得
- **1 イテレーション 10 秒以下**、claim の型ミスや auth 失敗を即時に確認
- 参考: `crates/alc-notify/examples/line_token_issue.rs`

```bash
# 秘密を Secret Manager から .env に書き出す例
gcloud secrets versions access latest --secret=line-private-key > /tmp/pk.pem
export LINE_CHANNEL_ID=... LINE_KEY_ID=... LINE_PRIVATE_KEY_PATH=/tmp/pk.pem
cargo run -p alc-notify --example line_token_issue
```

### 2. モジュール分割 (テスト可能な形に)

example で挙動を確認したら、本体コードを次の粒度で分ける:

- **pure な部分** (JWT claim 生成、署名、URL 組み立て等) は `pub(crate) fn xxx(...) -> Result<...>` で関数に切り出す。引数に `now: u64` 等を受け取り、時刻依存を消す
- **エンドポイント** は `const PROD_URL` を直接使わず、クライアント構造体のフィールドにして `new_with_endpoints(...)` で注入できるようにする → wiremock がローカル HTTP サーバーで差し替え可能になる
- クライアントの `new()` は `with_endpoints(PROD_URL, ...)` を呼ぶシンプルなラッパーに

参考実装: `crates/alc-notify/src/clients/line.rs` の `LineClient::with_endpoints` + `build_jwt_assertion`。

### 3. 単体テストで固める (wiremock)

`#[cfg(test)] mod tests` に以下を入れる:

- **pure 関数テスト** — 生成した JWT / URL / ペイロードをそのまま decode/parse して claim 値やレンジをアサート。仕様書の数値 (e.g. `token_exp` は `[30, 2592000]`) を直接コードに書いて回帰を捕まえる
- **クライアントテスト** — `MockServer::start().await` で仮想 API を立て、`with_endpoints(format!("{}/...", server.uri()), ...)` で注入。**成功 / 4xx / parse error / http error (unreachable) / 上流エラー伝播** の 5 パターンを最低限テストする
- **キャッシュ挙動** — `expires_in: 0` を返して次回呼び出しで再発行されること、`expect(N)` で mock の呼び出し回数を確定させる
- **コンストラクタテスト** — `new()` / `default()` が prod エンドポイントを持っていることを 1 行でチェック (`new_with_endpoints` パス経由の回帰検出)

完成したら `coverage_100.toml` に `type = "unit"` で登録 → `scripts/check_coverage_100.sh --unit-only` で CI が常時ガードする。

### アンチパターン

- **本番 DB / 本番 API に直接 unit test を叩かせない** — ネットワーク依存 + flaky + credential 漏洩リスク
- **「外部 API なのでテスト不可能」で放置しない** — pure 関数切り出し + wiremock で 100% 到達できる (今回の例が証拠)
- **token/URL を const にして直接叩く** — テスト時に差し替えられない。必ず struct フィールド化する

## 長時間 compute と Cloud Run の罠

### 結論: `tokio::spawn` で background compute → fire-and-forget broadcast はやらない

Cloud Run **default で CPU throttling が有効** (`--no-cpu-throttling` 指定なし) のため、
HTTP response 返却後は CPU 割当が瞬時に~0% に落とされる。`tokio::spawn(async move { ... })`
した background task は **HTTP 200/202 直後に CPU 停止 → 完走しない**。

検証経緯 (2026-05-10): Y時間 export を `POST /jobs` + WebSocket 完了通知の async pattern
で実装 (PR #340) → 本番で `job spawned` ログだけ出て `job completed` が来ない →
frontend の WS が 120s timeout で fail。実害発覚後 PR で revert (sync GET + parallel R2
fetch のみに戻す)。

### `tokio::spawn` を使いたい場合の選択肢 (どれもコスト or 実装コスト発生)

1. **`gcloud run services update --no-cpu-throttling`** (instance-based billing)
   - asia-northeast1 で **+$20-60/月** (request 頻度依存、24/7 alive で +$63/月)
   - `deploy.sh` に固定する場合 user 承認必須
   - 注意: `redact_broadcast.rs` (alc-notify の redact pipeline) は同じ pattern で動いている
     はずなので、production で本当に最後まで完走しているか要検証 (Y時間と同様に部分失敗
     している可能性あり)
2. **Cloud Tasks queue + 別 Cloud Run worker サービス**
   - Tasks にタスク push → 別 worker が処理 → broadcast
   - 別サービス追加。月数 cents 〜 $1 程度
3. **Cloudflare DurableObject 内で compute**
   - alc_csv_parser 等を TS/WASM 移植が必要、DB 直接アクセス不可 (rust-alc-api 経由)
   - 1-2 日 rewrite work、ほぼ無料

### sync HTTP で許容できる長さ

- Cloudflare proxy edge timeout: **100s**
- Cloud Run request timeout: default 60min (調整可)
- 5-15s なら sync HTTP で問題なし (Y時間 export は parallel R2 fetch 後 5-15s で済むため
  async 化せず sync で配信)

### `crates/alc-core/src/realtime_bus.rs::RealtimeBus`

汎用 broadcaster client (notify-realtime-bus Worker `/broadcast` への POST)。
`RedactBroadcaster` (`crates/alc-core/src/redact_broadcast.rs`) と同 env vars
(`NOTIFY_REDACT_BROADCAST_URL` / `NOTIFY_REDACT_BROADCAST_SECRET`) を共有。

**現状の使用箇所**: なし (Y時間 export での利用は revert 済。Y時間 export は ippoan/alc-dtako-worker へ移った)。将来 Cloud Tasks や DO 経由
の async pattern を再導入する際に再利用可能なように残してある (テスト 11 件付き)。

新規 async endpoint を増やす際は、上記 3 選択肢のいずれかを採用すること。
**`tokio::spawn` + `bus.broadcast()` を Cloud Run service の HTTP handler 内で直接
呼び出さない** こと (background task が確実には完走しない)。

### 並列 R2 fetch のパターン (Y時間 export 高速化、2026-05-10)

`futures::stream::iter().buffer_unordered(N)` で R2 fetch を並列化すると wall time を大幅に縮められる
(Y時間 export で 13 ヶ月レンジ 41-107s → 5-15s)。結果は順不同で返るので整列して決定的にすること。
Y時間 export 自体 (行の置き方・日をまたぐ入力の形・`POST /api/dtako/y-time-rows` の検証と除外) は
ippoan/alc-dtako-worker の `crates/alc-dtako-upload/src/ytime/` へ移った (W5c-2、Refs #680)。直すときはそちらへ。

**LIST も同じ (2026-07-31、Refs ohishi-exp/rust-ichibanboshi#205-27)**。etags (今は ippoan/alc-dtako-worker) の
LIST は複数 prefix を並列に投げる。
rust-s3 の `Bucket::list` は 1 prefix 内で continuation token を**直列**に辿る
(1000 key/ページ) ので、prefix ループまで直列だと往復が一列に並ぶ。**「往復回数を
減らせば速い」は直列前提でのみ正しい** — 並列前提では「1 prefix が 1 ページで
終わるまで細かく割って同時に投げる」方が速い (etags の LIST 実測 17s の主因がこれ)。

## テスト

- テストインフラ: `docker-compose.yml` (PostgreSQL 16, port 54322) + `tests/common/mod.rs` ヘルパー
- ローカル実行: `make test` (ユニットのみ、DB不要) / `make itest` (全テスト、DB必要)
- カバレッジ: `/coverage-check` スキル使用 (`--full` で サマリ + 未カバー行を1回で取得)
- 100% 達成済みファイル: `coverage_100.toml` で管理 (20ファイル、--text ベース)
- カバレッジリグレッション検証: `bash scripts/check_coverage_100.sh` (`--unit-only` で DB 不要モード)
- CI/CD: `.github/workflows/ci.yml` — push/PR to main で自動実行
- テストは並列実行可能 (`RUST_TEST_THREADS=1` 不要)
- env var 競合は `ENV_LOCK`、email_domain 競合は `GOOGLE_LOGIN_LOCK` (tests/common/mod.rs) で直列化
- DB エラー注入: 認証なしエンドポイントは `pool.close()`、認証ありは trigger (INSERT/UPDATE/DELETE) or RENAME (SELECT) + `DB_RENAME_LOCK` + `db_rename_flock()`
- **coverage gate 対象ファイルで `tracing` マクロを複数行にしない** — フォーマット文字列が独立行になる (手書きでも rustfmt の 100 桁折り返しでも) と、その行は llvm-cov の行カバレッジに乗らず 100% gate が fail する (PR #399/#400 で 2 回発生)。メッセージを短くして必ず 1 行に収める
- カバレッジ計画: `plans/coverage_100.md`

### 100% 未達成ファイル一覧 (2026-03-27 実測)

最新データは `/coverage-check --summary` で取得可能。

| ファイル | Lines | Miss | Cover | 備考 |
|---------|-------|------|-------|------|
| auth/google.rs | 117 | 87 | 25.64% | Google JWT検証 (外部API依存) |
| auth/lineworks.rs | 240 | 63 | 73.75% | LINE WORKS OAuth (外部API依存) |
| compare/mod.rs | 3094 | 184 | 94.05% | 比較ロジック |
| csv_parser/work_segments.rs | 464 | 32 | 93.10% | 作業区間パーサー |
| fcm.rs | 26 | 26 | 0.00% | FCM送信 (外部API依存, trait mock済み) |
| main.rs | 115 | 115 | 0.00% | エントリポイント (テスト対象外) |
| routes/auth.rs | 557 | 176 | 68.40% | 認証ルート (Google/LINE WORKS) |
| routes/bot_admin.rs | 179 | 23 | 87.15% | Bot管理 |
| routes/car_inspection_files.rs | 38 | 3 | 92.11% | 車検証ファイル |
| routes/car_inspections.rs | 173 | 16 | 90.75% | 車検証 |
| routes/carins_files.rs | 284 | 39 | 86.27% | 車検証ファイル管理 |
| routes/devices.rs | 1467 | 260 | 82.28% | デバイス管理 |
| routes/equipment_failures.rs | 290 | 21 | 92.76% | 機器故障 |
| routes/sso_admin.rs | 166 | 35 | 78.92% | SSO管理 |
| routes/tenant_users.rs | 146 | 21 | 85.62% | テナントユーザー |
| routes/tenko_call.rs | 217 | 53 | 75.58% | 中間点呼 |
| routes/tenko_records.rs | 326 | 50 | 84.66% | 点呼記録 |
| routes/tenko_schedules.rs | 325 | 22 | 93.23% | 点呼スケジュール |
| routes/tenko_sessions.rs | 1263 | 149 | 88.20% | 点呼セッション |
| routes/tenko_webhooks.rs | 162 | 33 | 79.63% | Webhook設定 |
| storage/gcs.rs | 42 | 42 | 0.00% | GCS (本番のみ) |
| storage/mod.rs | 11 | 11 | 0.00% | ストレージ抽象 |
| storage/r2.rs | 39 | 39 | 0.00% | R2 (本番のみ) |
| webhook.rs | 164 | 6 | 96.34% | Webhook配信 |
- **DB エラー注入**: `BEGIN → ALTER TABLE RENAME → テスト → ROLLBACK` パターン (PostgreSQL DDL は ROLLBACK 可能)
- **SSE テスト**: コアロジックを `pub async fn xxx_core()` に抽出し、SSE ラッパーとは別にテスト可能にする

## ブランチワークフロー

**main に直接 merge/push してはいけない。** 複数の Claude が並行作業しているため、main を直接変更すると他の worktree や作業に影響する。

### 基本フロー

1. **worktree で作業**: `isolation: "worktree"` の Agent、または手動で worktree を作成
2. **branch 作成・push**: worktree 内で `fix/xxx` ブランチを作成し push
3. **CI 確認**: GitHub Actions の結果を確認
4. **remote で merge**: `gh pr create` → `gh pr merge` で GitHub 上で main にマージ

### Worktree 作成ルール

- **`git checkout main` は禁止** — PreToolUse hook (`branch-switch-guard.sh`) でブロックされる
- **メインワークツリーのソースファイル編集は禁止** — PreToolUse hook (`worktree-edit-guard.sh`) が Write/Edit をブロック
  - `src/`, `tests/`, `migrations/`, `Cargo.toml` 等のコード変更は必ず worktree 内で行う
  - 例外（メインワークツリーで編集可）: `CLAUDE.md`, `.claude/*`, `.gitignore`, `docs/*`, ルート直下 `.md`, `coverage_100.toml`, `.github/*`
- 新しいブランチが必要な場合は **必ず worktree を使う**
- **`origin/main` をベースにすること** — ローカル main は古い可能性がある。hook (`worktree-fetch-guard.sh`) で強制
- **マージ済み worktree の削除**: `bash ~/.claude/hooks/worktree-cleanup.sh` で一括クリーンアップ

```bash
# 正しい方法
git fetch origin main
git worktree add -b fix/xxx .claude/worktrees/xxx origin/main

# NG: ローカル main がベース (hook でブロックされる)
git worktree add -b fix/xxx .claude/worktrees/xxx main
```

### 単一テスト CI (`single-test.yml`)

`fix/test_xxx` ブランチを push すると、`test_xxx` だけ `cargo llvm-cov` で実行される。
**トリガーは `fix/test_*` のみ** — `fix/` 全般ではない（二重 CI 防止）。

```bash
# 例: test_communication_items_crud だけ CI で実行
git push origin fix/test_communication_items_crud
```

カバレッジ修正のイテレーションに使用する。全テスト実行は main CI (`ci.yml`) に任せる。
`fix/` だが `fix/test_` 以外のブランチ（例: `fix/dead_code`）は CI (`ci.yml`) の PR トリガーのみ。

### ローカルテスト不要のワークフロー

テストの作成から検証まで **ローカルで cargo test / cargo llvm-cov を一切実行しない** ワークフロー。
ローカル CPU リソースを節約しつつ、CI 上で全検証を完結させる。

```
1. Agent がテストコードを書く (Read/Write/Edit のみ)
2. 親が cargo fmt → git commit → git push (fix/test_xxx ブランチ)
3. single-test.yml が CI 上でテスト + カバレッジ実行
4. CI 成功 → gh pr create → gh pr merge --squash
5. main merge で ci.yml が全テスト + カバレッジ検証を自動実行
6. CI 失敗 → gh run view でログ確認 → worktree で修正 → 再 push → 3 に戻る
```

- ローカルでは `cargo fmt` のみ (pre-commit hook で強制)
- テスト実行・カバレッジ計測はすべて GitHub Actions 上
- 5つのブランチを並列 push すれば CI も並列実行される

### PR 作成・マージ

```bash
gh pr create --base main --head fix/xxx --title "タイトル" --body "説明"
gh pr merge <PR番号> --squash
```

### 並列 Agent ワークフロー（カバレッジ作業等）

バックグラウンド Agent は **対話的権限取得不可**。Bash・Write・Edit すべて事前許可が必要。
**worktree 作成・git 操作は親が行い、Agent にはコード書き（Read/Write/Edit）だけ任せる**。

#### 前提: settings.json に事前許可を追加

```json
"Write(//home/yhonda/rust/rust-alc-api/.claude/worktrees/**)",
"Edit(//home/yhonda/rust/rust-alc-api/.claude/worktrees/**)"
```

#### 手順

```
1. 親が worktree + ブランチを一括作成
   for name in file_a file_b file_c; do
     git worktree add -b "fix/test_${name}" ".claude/worktrees/${name}" main
   done

2. 親がバックグラウンド Agent を並列起動（run_in_background: true）
   - Bash 不要。Read/Write/Edit/Glob/Grep のみ使用を指示
   - worktree パスを明示: /home/yhonda/rust/rust-alc-api/.claude/worktrees/<name>
   - 未カバー行・DB エラー注入パターン等の必要情報をプロンプトに含める

3. Agent 完了後、親が各 worktree で cargo fmt → git add/commit/push
   cd .claude/worktrees/<name>
   cargo fmt && git add -A && git commit -m "test: ..." && git push -u origin fix/test_<name>

4. push 後の CI 監視 → 失敗時は修正
   - バックグラウンドで gh run list をポーリングして CI 完了を待つ
   - CI 失敗したブランチは gh run view でログ確認 → worktree で修正 → 再 push
   - CI 成功したブランチは gh pr create → gh pr merge --squash
```

#### 注意事項

- `isolation: "worktree"` の Agent は **自動で worktree を作るが Bash 権限がないと何もできない**
- 代わりに通常の Agent（worktree なし）を起動し、worktree パス内のファイルを直接 Read/Write させる
- Agent プロンプトには「Bash は使えません」と明記すること
- **DB エラー注入パターンの使い分けを明示指示すること** — `/coverage-test-patterns` スキルの全内容をプロンプトに含める。特に:
  - `pool.close()` は認証なし (public_router) エンドポイントのみ（認証ありはミドルウェアで先に失敗する）
  - trigger: INSERT/UPDATE/DELETE エラー用
  - RENAME: SELECT エラー用（認証ありエンドポイント）— **`DB_RENAME_LOCK` + `db_rename_flock()` 必須**
- 完了後の worktree クリーンアップ: `git worktree remove .claude/worktrees/<name>` + `git branch -d fix/test_<name>`
- **worktree 削除前に必ず `cd /home/yhonda/rust/rust-alc-api`** すること — シェルの cwd が worktree 内だと削除時に `getcwd` が失敗しセッションが切断される

## ステージング環境

PR を main に向けると CI が自動で staging 環境にデプロイする。本番とは独立したインフラ。

### インフラ構成

| コンポーネント | staging | 本番 |
|---|---|---|
| **API** | Cloud Run `rust-alc-api-staging` (multi-container + PostgreSQL sidecar) | Cloud Run `rust-alc-api` |
| **DB** | CloudSQL Postgres (staging スキーマ) | Supabase PostgreSQL |
| **Frontend (alc-app)** | Cloudflare Workers `alc-app-staging` | Cloudflare Workers `alc-app` |
| **Auth** | Cloudflare Workers `auth-worker-staging` | Cloudflare Workers `auth-worker` |
| **ストレージ** | Cloudflare R2 (staging バケット) | Cloudflare R2 (本番バケット) |

### URL

| サービス | URL |
|---|---|
| API (staging) | `https://rust-alc-api-staging-566bls5vfq-an.a.run.app` |
| Frontend (staging) | `https://alc-app-staging.m-tama-ramu.workers.dev` |
| Auth Worker (staging) | `https://auth-worker-staging.m-tama-ramu.workers.dev` |

### 認証フロー (staging)

staging の alc-app は2つの認証モードをサポート:

1. **Auth バイパスモード** (デフォルト): `NUXT_PUBLIC_STAGING_TENANT_ID` で固定 tenant_id を設定。ログイン不要で X-Tenant-ID ヘッダーで直接アクセス
2. **Google OAuth モード**: ログイン画面の Google ログインボタンで認証。auth-worker-staging → Google OAuth → rust-alc-api-staging

```
[alc-app-staging]
  ├── Auth バイパス: X-Tenant-ID ヘッダーで直接 API アクセス
  └── Google OAuth: accounts.google.com → /auth/callback → rust-alc-api-staging /api/auth/google/code
```

### データ永続性

staging の PostgreSQL は Cloud Run sidecar コンテナ + `emptyDir` ボリューム。**データは揮発性**。
- `minScale: "0"` → アイドル約15分でインスタンス停止 → DB データ消失
- 次のリクエストでコールドスタート → マイグレーションで DB スキーマは再作成されるが、ユーザーデータ（テナント、ユーザー登録等）は消える
- OAuth で登録したユーザーも消える
- テスト環境としてはベスト: 毎回クリーンな DB から始まるので汚れたデータが残らない
- 永続データが必要な場合のみ `minScale: "1"` や Cloud SQL を検討

### Staging Export/Import API (消した)

揮発性 DB のデータ復元用の `GET /api/staging/export`・`POST /api/staging/import` (`STAGING_MODE` と `X-Staging-Key` でガード) は
**消した** (移行計画 W4a、Refs #680)。staging の Cloud Run でも直近 30 日の呼び出しが 0 件で、呼び手は auth-worker の
top ページのボタンと Bot 設定の import だけだった (どちらも同時に外した)。`staging/test-data.json` と staging の
`STAGING_API_KEY` (`ALC_STAGING_API_KEY`) の注入も消した。`STAGING_MODE` は sso_admin が使うので残る。

### Auth バイパスモード (alc-app)

`NUXT_PUBLIC_STAGING_TENANT_ID` が設定されている場合、OAuth なしでキオスクモード (`activateDevice`) を自動有効化。

- `useAuth.ts` の `init()` で stagingTenantId があれば `activateDevice()` を呼ぶ
- `auth.global.ts` ミドルウェアで stagingTenantId 設定時は認証スキップ
- API リクエストは `X-Tenant-ID` ヘッダー経由（JWT 不要）

### StagingFooter 共有コンポーネント

`@yhonda-ohishi-pub-dev/auth-client` パッケージ (`auth-worker/packages/auth-client/`) に `StagingFooter.vue` を追加。
alc-app の `app.vue` で使用。staging 時のみ黄色バーを表示し、Export/Import ボタンを提供。

### Playwright E2E テスト (alc-app)

staging 環境に対する E2E テスト。CLI でローカル実行（CI ジョブなし）。

```bash
cd /home/yhonda/js/alc-app/web
npx playwright test
```

- 設定: `web/playwright.config.ts`
- テスト: `web/tests/e2e/staging-bootstrap.spec.ts`
- ヘルパー: `web/tests/e2e/helpers/staging.ts` (wakeUpStaging, importTestData)
- テストデータ: `web/tests/e2e/fixtures/test-data.json`
- フロー: staging wake up → import → auth バイパスでページ表示確認

### CI 自動デプロイ

- **rust-alc-api**: PR to main → `ci.yml` の `deploy-staging` ジョブ → Cloud Run staging
  - Docker イメージは GHCR に push → Artifact Registry リモートリポジトリ (`asia-northeast1-docker.pkg.dev/cloudsql-sv/ghcr/`) 経由で Cloud Run が pull
- **alc-app**: PR to main → `test.yml` の `deploy-staging` ジョブ → `wrangler deploy --env staging`

### Secrets / 環境変数

**rust-alc-api staging** (Cloud Run → Secret Manager):
- `alc-api-staging-jwt-secret`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`
- `alc-r2-access-key`, `alc-r2-secret-key`, `alc-oauth-state-secret`
- `carins-r2-access-key`, `carins-r2-secret-key`, `dtako-r2-access-key`, `dtako-r2-secret-key`
- SA `747065218280-compute@developer.gserviceaccount.com` に `secretmanager.secretAccessor` 付与済み
- SA `staging-deploy@cloudsql-sv.iam.gserviceaccount.com` に Artifact Registry `artifactregistry.reader` 付与済み

**alc-app staging** (Cloudflare Workers → wrangler.jsonc `env.staging.vars`):
- `NUXT_PUBLIC_API_BASE`: staging Cloud Run URL
- `NUXT_PUBLIC_AUTH_WORKER_URL`: auth-worker-staging URL
- `NUXT_PUBLIC_STAGING_TENANT_ID`: auth バイパス用固定 tenant_id
- `NUXT_PUBLIC_GOOGLE_CLIENT_ID`: Google OAuth Client ID

**auth-worker staging** (Cloudflare Workers → `wrangler secret`):
- `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `OAUTH_STATE_SECRET` (本番と同一値)

**Google OAuth**: 承認済みリダイレクト URI に以下を追加済み:
- `https://auth-worker-staging.m-tama-ramu.workers.dev/oauth/google/callback`
- `https://alc-app-staging.m-tama-ramu.workers.dev/auth/callback`

## デプロイルール

- コードの修正・変更が完了したら、デプロイするかどうかを **AskUserQuestion ツールの選択肢形式** で確認すること
- 選択肢: 「デプロイする」「デプロイしない」の2択で提示
- 確認なしに `deploy.sh` を実行してはいけない
- デプロイコマンド: `./deploy.sh` (Cloud Run へデプロイ)

<!-- migrated from memory/feedback_*.md (2026-05-11) -->

## 追加運用 rule (rust-alc-api / alc-app / nuxt-notify 共通)

### 設定値・Secret 管理

- **`cloudrun/render.sh` や `.github/workflows/*.yml` に値ハードコード禁止** — email/設定値は Secret Manager に格納し `valueFrom: secretKeyRef: {key: latest, name: <name>}` で参照。既存の `JWT_SECRET` / `GOOGLE_CLIENT_ID` / `OAUTH_STATE_SECRET` パターンに揃える。SA `747065218280-compute@developer.gserviceaccount.com` は `secretmanager.secretAccessor` 付与済 (`feedback_no_hardcode_in_render`)
  - 例外: 完全 static な routing 設定 (`STORAGE_BACKEND=r2`) や boolean (`STAGING_MODE=true`) は `value:` 直書き可
- **新規 secret を `secretKeyRef` で参照する前に runtime SA へ per-secret grant が必要** — `747065218280-compute@` への `secretmanager.secretAccessor` は **project レベルではなく per-secret grant** なので、`secret-inject` skill 等で新 secret を投入しただけでは Cloud Run が解決できず、`gcloud run services replace` の cutover が `Permission denied on secret: <NAME> ... must be granted roles/secretmanager.secretAccessor` で fail する。新 secret ごとに 1 回:
  ```bash
  gcloud secrets add-iam-policy-binding <NAME> --project=cloudsql-sv \
    --member="serviceAccount:747065218280-compute@developer.gserviceaccount.com" \
    --role="roles/secretmanager.secretAccessor"
  ```
  (事例: #391 `ALC_STAGING_API_KEY` — grant 漏れで staging cutover が permission denied → enforce が無音で未適用だった。`feedback_cloudrun_new_secret_grant`)
- **Secret Manager 更新後は `gcloud run deploy` で新 revision 強制作成** — 既存インスタンスは起動時にキャッシュした値を使い続けるため、`gcloud run services update` だけでは反映されない (`feedback_cloudrun_secret_cache`)
- **alc-app の `wrangler.jsonc` はトップレベル `vars` が必須** — `env.staging.vars` だけでは本番反映されない。未設定だと `NUXT_PUBLIC_API_BASE` が localhost:3001 にフォールバックして本番 Failed to fetch / OAuth 不能 (`feedback_alc_app_vars`)

### Cloud Run デプロイ

- **rust-alc-api の本番デプロイは `/tag-release patch` でタグを打つだけ** — `./deploy.sh` ローカル実行は使わない。CI ci.yml `deploy-production` ジョブが v* タグで自動実行 (GHCR → AR → Cloud Run migration + deploy) (`feedback_cloudrun_ci_deploy`)

### Gemini API (notify redact / extract)

- **`generationConfig` には必ず `responseMimeType: "application/json"` + `responseSchema` の両方注入** — `responseMimeType` だけでは markdown wrap / 前置テキストで parse error 発生。`responseSchema` (OpenAPI 3.0 subset) が constraint として構造を強制 (`feedback_gemini_response_schema_required`)
  - 型 enum は **大文字** (`STRING / NUMBER / INTEGER / BOOLEAN / ARRAY / OBJECT`)
  - 配列の固定長は `minItems` + `maxItems` で挟む (`box_2d` の `[ymin, xmin, ymax, xmax]` は両方 4)
  - schema を test で pin (`assert_eq!(schema["properties"]["x"]["type"], "STRING")`)
  - parse error 時は raw response 先頭 1KB を `tracing::warn!` で残す
  - 参考実装: `crates/alc-notify/src/redact.rs` の `redact_response_schema() / stage1_response_schema() / stage2_response_schema()` (PR #318)
  - 事故事例: PR #313 で `responseSchema` 忘れ → Stage 1 parse 失敗 → 1-stage fallback → 3164 マスクズレが staging で再現せず、PR #318 で修正
- **bbox / 構造化 output の検証は Python プローブ → 画像オーバーレイで 30 秒イテレーション** — staging deploy ループは 1 イテ 5-10 分かかるので、Rust に書く前に `/tmp/redact_probe.py` + `/tmp/visualize_bbox.py` で prompt を固める。`GEMINI_API_KEY` は `~/js/denchoho-invoice/.env` から拝借 (`feedback_local_gemini_bbox_iteration`、`notify_pdf_redact_design.md` 参照)

### Notify viewer / PDF 配信

- **LINE / LINE WORKS webview の PDF inline 表示は PDF.js (`vue-pdf-embed`) canvas 描画のみ** — `Content-Type: application/pdf` + `Content-Disposition: inline` でも DL ダイアログになる。R2 presign 直 redirect でも改善しない (PR #301 検証済)。canvas 描画必須 (`feedback_webview_pdf_pdfjs`)
  - PDF.js は同一オリジン or CORS 許可済みオリジンから fetch する必要あり → R2 presign 直 access させず同一オリジンのストリーム (`/api/notify/v/{token}/file`) で配信 (PR #303)。**この閲覧の口は今は nuxt-notify の Worker (KV `NOTIFY_VIEW_KV`・R2) に在り、rust の閲覧の口 (`viewer.rs`・`read_tracker.rs`、公開の `/notify/v/*`・`/notify/read/*`、内部用の `/internal/notify/view/*`) は W8a で消した** (Refs #680・#648)。再作成は `scripts/check_domain_split.sh` が落とす。`guess_content_type`・`build_inline_disposition` は `documents.rs` (文書の preview) に移した
  - `nuxt-notify` の `app.vue` で `route.path.startsWith('/v/')` 分岐を入れて認証 gate をバイパス

### テスト関連

- **`cargo test` がキャッシュで新テストを認識しない時は `cargo clean -p <package>`** — Rust incremental compile cache が古いテストバイナリを再利用するケースあり (worktree と main の `target/` 共有時に発生しやすい)。CI はクリーンビルドなので問題なし (`feedback_test_cache`)
- **テストファイル削除は hook がブロック** — `git commit` 時に複数 `*.test.ts` 削除を検出してブロック。**`cat /dev/null > file` + `describe.skip()` placeholder** で modified commit にして通す:
  ```typescript
  import { describe, it } from "vitest";
  describe.skip("xxx (removed — see <replacement>)", () => {
    it("placeholder", () => {});
  });
  ```
  後続 PR で物理削除を少量ずつ行う (`feedback_test_file_deletion_hook`)
