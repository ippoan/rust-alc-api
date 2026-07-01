---
name: rust-alc-api-map
generated-from: rust-alc-api:9641c4bd31861a675148afdf72f10a3989b5ae85
paths: [crates/, src/, migrations/]
description: rust-alc-api (アルコールチェッカー基盤の Rust/Axum Cargo workspace — 13 domain crate + gateway/tenko/carins/dtako/trouble の複数バイナリ、PostgreSQL+RLS、Cloud Run) の構造ナビゲーション。どの crate に何のルートがあるか / monolith(rust-alc-api) と per-domain API + gateway の二系統 / RLS・migration・deploy/release 分離の gotcha を 1 枚にまとめる。トリガー:「rust-alc-api」「alc-api」「alc-notify」「alc-tenko」「alc-trouble」「alc-carins」「alc-dtako」「gateway」「tenko-api」「carins-api」「dtako-api」「trouble-api」「RLS テナント」「sqlx migration」「ts-rs」「Release Wave」「Bazel」等。
---

# rust-alc-api-map — rust-alc-api 構造ナビゲーション

アルコールチェッカーシステムの backend。Cargo workspace で **13 の domain crate** (`alc-*`)
+ **複数バイナリ** を持つ。PostgreSQL (`alc_api` スキーマ + RLS) / Cloudflare R2 (or GCS) /
Google OAuth + LINE WORKS。Cloud Run にデプロイ。

> ここは索引。網羅ではない。実ルートの完全列挙や関数シグネチャは repo 側が正。
> frontmatter の `generated-from` が現 tree-sha とズレたら hook が再生成を促す。

## 二系統のバイナリ構成 (重要)

| 系統 | バイナリ | 役割 |
|---|---|---|
| **monolith** | `rust-alc-api` (`src/main.rs`) | 全 domain crate の router を `/api` 下に一括 nest。全 repo を 1 プロセスで提供 |
| **gateway** | `gateway` (`crates/gateway`) | JWT 検証 + reverse proxy。public route 判定 (`routes.rs::is_public_route`) して各 per-domain API へ転送 |
| **per-domain API** | `tenko-api` / `carins-api` / `dtako-api` / `trouble-api` | 各 domain crate の router だけを単独で立てる薄い main。`X-Tenant-ID` header 認証 (`require_tenant_header`) |
| **CLI** | `migrate` (`src/bin/migrate.rs`) / `archive` (`src/bin/archive.rs`) | sqlx migration 実行 / アーカイブ Job |

monolith と per-domain API は同じ domain crate (`alc-tenko` 等) を共有 → ルート実装は 1 箇所。

## 区画 (workspace crate)

| crate | 役割 / 主要ルート群 |
|---|---|
| `alc-core` | 共通基盤: models / repository trait / `auth_middleware` / `realtime_bus` / `redact_broadcast`。ts-rs 型 export 元 |
| `alc-auth` | Google / LINE WORKS OAuth、JWT。`routes/mod.rs` で `auth` として re-export。`issue_tokens_for_google_claims` は招待→ドメイン一致の順で tenant 解決し、どちらも無ければ prod は 403 (#332 ゴミテナント防止)。**`STAGING_MODE=true` のときだけ #332 前の `create_tenant_with_domain` 自動作成を復活** (揮発 DB で毎回新規ユーザーになる staging login の救済、Refs #434)。`internal.rs` (`internal_router`) は **認証 DB プリミティブを `/api/internal/auth/*` で公開** (sso-config 読み / user upsert-line(works) / recipient / refresh-token 保存、`require_internal_jwt` 配下)。OAuth オーケストレーションを auth-worker に移管するための土台で、token は発行せず user + tenant slug を返すのみ (Refs #434 Phase 1/2) |
| `alc-misc` | health (`/health` + `/health/secret-fingerprint?name=&expected=` = 任意 env の sha256[0..8] と `expected` 突合、`{match: bool}` のみ返し oracle 防止。cross-store drift を CI で自動検出、Refs ippoan/rust-alc-api#424 / ippoan/ci-workflows#131) / health_canary / measurements / employees / items / api_tokens / sso_admin / tenant_users / timecard / access_requests / staging / upload / bot_admin / driver_info / members / communication_items / carrying_items / guidance_records |
| `alc-tenko` | 点呼: tenko_call / tenko_records / tenko_schedules / tenko_sessions / tenko_webhooks / daily_health / equipment_failures / health_baselines |
| `alc-carins` | 車検証(carins): car_inspections / car_inspection_files / carins_files / nfc_tags |
| `alc-dtako` | デジタコ: dtako_* (csv_proxy / daily_hours / drivers / logs / operations / restraint_report(_pdf) / scraper / tickets / upload / vehicles / work_times / y_time_export / event_classifications) / vehicle_settings_dumps。`dtako_tickets` は email-receiver Worker から SD カードエラー通知メールを起票し F-VOS3020 設定 ZIP DL → QR で close する pipeline (Refs ippoan/email-receiver#1)。tenant_router (JWT) + internal_router (`INTERNAL_SHARED_SECRET` + `X-Tenant-ID`) + public_close_router (`close_token` のみ) の 3 経路 |
| `alc-trouble` | トラブル管理: tickets / files / workflow / categories / offices / progress_statuses / schedules / tasks / task_types / task_statuses / notifications / notifier / cloud_tasks / lineworks_members。**schedule fire は #434 lockdown で internal 化**: `schedules::internal_fire_router` (`/api/internal/trouble/schedules/{id}/fire`, `require_internal_jwt`) を monolith の internal_protected に集約。旧 bare public `fire_router` は撤去 (現状 `cloud_tasks: None` で未配線)。trouble-api サブサービスは fire を mount しない (gateway が `/api/internal/*` を backend へ振るため) |
| `alc-notify` | LINE/LINE WORKS 配信: recipients / groups / documents / distribute / ingest / line_config / line_webhook / lineworks_* / read_tracker / viewer / email_documents / extract / redact / background_extract / background_redaction。**`line_webhook` は #434 lockdown で internal_router 併設**: `/api/internal/notify/line/webhook` (`require_internal_jwt`、auth-worker の public 受け口が OIDC mint で forward)。署名検証 (全テナント channel secret 照合) は rust 側で、**`list_enabled_line_configs()` SECURITY DEFINER 関数経由で RLS バイパス** (migration 117。生クエリだと未認証パスで `app.current_tenant_id=''` → `''::UUID` キャストが 500 する既知罠)。**ただし 072 の `FORCE ROW LEVEL SECURITY` があると所有者=SECURITY DEFINER 実行ロールにも RLS が効いて関数経由でも 500 するため、migration 118 で `NO FORCE` にして所有者バイパスを効かせている** (devices は元から FORCE 無し)。app ロール (非所有者) には RLS 維持。`public_router` (`/notify/line/webhook`) は LINE Console URL 切替 + allUsers 削除までの移行期間 dual-mount |
| `alc-devices` | デバイス登録 (`devices`) |
| `alc-storage` | StorageBackend trait + R2 / GCS / HttpProxy 実装 |
| `alc-csv-parser` / `alc-compare` | CSV パース / 比較ロジック |
| `alc-pdf` | PDF 生成 (assets/fonts 同梱) |

## entrypoint / router

- **monolith**: `src/main.rs` — DATABASE_URL で `PgPool`、Storage backend (`STORAGE_BACKEND` = r2/gcs、
  carins/dtako/notify/trouble は別バケット+別 R2 キー)、巨大な `AppState` に全 Pg*Repository を組み立て、
  `.nest("/api", rust_alc_api::routes::router())`。背景 task: 60s ごと `check_overdue_schedules`。
- **router 本体**: `src/routes/mod.rs` — 各 domain crate のルートを re-export し `router()` で結線。
  middleware: `require_tenant_header` (tenant/admin 共通、注入 identity 信頼) / `require_internal_jwt`
  (auth-worker→internal ingest、aud=alc-api-internal。**#434 Phase D で HS256 / Google OIDC の
  dual-accept 化**: `InternalOidcTrust{enabled,verifier}` Extension (env `INTERNAL_AUTH_TRUST_OIDC=1`
  で enabled) の時のみ `GoogleTokenVerifier::verify_internal_oidc` が JWKS で RS256 署名検証 +
  iss + aud=alc-api-internal + exp して OIDC を受理 (Cloud Run IAM に加えた app 層 defense-in-depth)。
  flag off は HS256 のみ = 非破壊) / `require_internal_shared_secret`
  (email-receiver→`/api/dtako/tickets`)。
  **#434 で monolith のローカル JWT 検証を撤去**: 旧 `require_jwt` / `require_tenant` (bare X-Tenant-ID
  フォールバック) / `TenantProxySecret` gate (#437) / 未配線の `require_tenant_or_device` (#436、device-token)
  を全削除し、tenant/admin 経路を `require_tenant_header` に一本化した。rust-alc-api は JWT を検証せず、
  前段 proxy (CF Worker = alc-app/carins/nuxt-items、または per-domain gateway) が auth-worker
  `/auth/introspect` で検証して注入する `X-Tenant-ID` / `X-User-*` ヘッダーを信頼する dumb backend。
  外部直叩き防止は **Cloud Run IAM 網層ロックダウン** (proxy の OIDC ID token のみ到達可) が担う
  (確定アーキ #4807535677、step 3)。テストは `tests/common/mod.rs` の `test_proxy_inject` が proxy 役で
  Bearer JWT → identity ヘッダーに変換し従来テストを無改修で通す。
- **gateway**: `crates/gateway/src/{main,routes,proxy,auth,config}.rs`。`is_public_route` に
  列挙された path (health / auth/* / tenko-call register / devices register / staging /
  `/notify/line/webhook` / notify read / access-requests 等) は JWT skip でそのまま proxy。
  **#434 lockdown**: trouble schedule fire は public 列挙から外し internal 化、LINE webhook の
  判定 path も `/notify/line-webhook` (誤) → `/notify/line/webhook` (実パス) に修正。
  `resolve_backend` は `/api/internal/*` を **dtako_url (fallback backend)** へ振る = internal
  ルートは monolith backend が処理する。

## gotcha (CLAUDE.md / README 由来)

- **DB 接続**: `alc_api_app` ロール (NOBYPASSRLS → RLS 有効) で、**直接接続 port 5432** を使う
  (Supavisor 6543 は `set_config` がリセットされ RLS テナント分離が壊れる)。
  `DATABASE_URL` に `?options=-c search_path=alc_api` 必須。
- **staging は postgres superuser 接続 → RLS 完全 bypass** (`staging/cloudrun-staging.yaml` の
  `postgresql://postgres:...`)。superuser は `FORCE ROW LEVEL SECURITY` でも RLS を無視するため、
  **RLS 頼みで WHERE tenant_id を省いたクエリは staging で全テナント横断に漏れる**。tenant scope は
  `crates/alc-core/src/tenant.rs::TenantConn` が `set_current_tenant` で `app.current_tenant_id` を
  立てて RLS に委ねるが、本番 (alc_api_app) でしか効かない。staging も対象にするクエリは
  **WHERE tenant_id を明示**すること (Refs #434、sso_admin で実害)。
- **pre-auth SECURITY DEFINER + FORCE ROW LEVEL SECURITY の罠**: 未認証経路 (LINE webhook /
  LINE login / notify viewer `/v/{token}` 等) が cross-tenant 検索用の SECURITY DEFINER 関数
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
  修正は新ファイル追加。migration は **Cloud Run Jobs** (`rust-alc-api-migrate`) で deploy 前に実行
  (`main.rs` から `sqlx::migrate!()` は削除済み = 起動時自動適用なし)。
- **snapshot hook (plan 整合性)**: `if_flag!(...)` 追加時は先に `ippoan/ippoan-dev-plans` の
  `scope:rust-alc-api` plan Issue を作り `npm run snapshot` で `manifests/production.snapshot.json` 更新。
  pre-commit (`.githooks`) + CI `snapshot-check` job が drift/stale-sha を検出。`SKIP_CLIPPY=1` で commit 時 clippy skip 可。
- **ts-rs**: 各 crate が `#[ts(export)]` で TS 型を生成 (`cargo test export_bindings`)。フロント
  (nuxt-*) が型同期する。型変更時は export_bindings を回す。生成元は alc-core / alc-misc /
  alc-carins の 3 crate のみ (alc-auth は ts-rs 依存だけあって derive 未使用)。CI での
  `ts-bindings-${sha}` artifact は **test-matrix(lib) shard が生成** する (check job ではない、
  Refs #482 / 下記 CI 節)。
- **長時間 compute と Cloud Run**: `tokio::spawn` で background compute → fire-and-forget broadcast は
  やらない (Cloud Run は応答後に CPU を絞る)。`RealtimeBus` / `RedactBroadcaster` で対処。CLAUDE.md 該当節参照。

## CI / deploy から見た立ち位置

- **Bazel + Cargo の二重ビルド**: `BUILD.bazel` (rust_library `rust_alc_api_lib` + rust_binary 群 + rust_test)
  と Cargo workspace の両方が存在 (`MODULE.bazel` / `.bazelrc`)。CI は主に Cargo (`cargo llvm-cov nextest`)。
- **deploy.yml は deploy/release 分離 (Refs #137)**: PR → staging 自動 deploy、tag(v*) push → production。
  **production の tag release は新 revision を 0% (no-traffic) で deploy するだけ**で traffic は旧 revision に残す。
  実際の切替は **Release Wave flip** が行う。`verify-no-traffic` job がこの不変条件を検証 (latest revision が
  0% traffic でなければ FAIL)。
- **複数 Dockerfile**: `Dockerfile` (monolith + migrate + archive + PDFium 同梱) / `Dockerfile.gateway` /
  `.tenko` / `.carins` / `.dtako` / `.trouble`。各 service が個別 Cloud Run service (`rust-alc-api`,
  `rust-alc-api-gateway`, `rust-alc-api-tenko` ...) として deploy される。`cloudrun/render.sh` が YAML 生成。
- **手動 `deploy.sh`** もあり (monolith のみ、AR `cloudsql-sv/alc-app` へ)。通常は CI 経由。
- **coverage 100% ガード**: `coverage_100.toml` 登録ファイルは CI でリグレッション検出。mock テストは
  domain 別 (`tests/mock_tenko` `mock_dtako` `mock_carins` `mock_trouble` `mock_devices` `mock_misc`)。
- **その他 workflow**: `migration-safety-check.yml` (適用済み migration の破壊的変更検査) /
  `release-wave.yml` (Release Wave caller、`repository_dispatch` で cloudrun flip を受ける)。
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
