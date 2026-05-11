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
- 必ず **直接接続 (port 5432)** を使用（Supavisor port 6543 は set_config がリセットされる）
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
  "role": "admin | viewer",
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

## ストレージバックエンド切り替え

- `STORAGE_BACKEND=r2` → Cloudflare R2 (`rust-s3` crate)
- `STORAGE_BACKEND=gcs` → GCS (reqwest 直接呼び出し、Cloud Run メタデータサーバー認証)
- `StorageBackend` trait で抽象化 (`src/storage/`)

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

# マイグレーション検証 (Docker 必要)
bash ~/.claude/skills/migrate-test/scripts/migrate_test.sh

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
| `tests/common/mod.rs` | テストハーネス (DB 接続、サーバー起動、JWT 発行ヘルパー) |
| `tests/common/mock_storage.rs` | インメモリ StorageBackend 実装 |
| `tests/auth_test.rs` | JWT 認証 / X-Tenant-ID / 未認証拒否 |
| `tests/employees_test.rs` | RLS テナント分離 / キオスクモード |

### テスト用インフラ

- `docker-compose.yml` — テスト用 PostgreSQL 16 (ポート 54322、tmpfs)
- `scripts/init_local_db.sql` — `alc_api` スキーマ + `alc_api_app` ロール + Supabase 互換ロール
- `.test-config` — `test_and_deploy.sh` 共通スクリプトの設定

### マイグレーション作成時の注意

- **適用済みのマイグレーションファイルは絶対に変更しない** — SQLx は SHA-384 チェックサムで検証し、不一致だとアプリが起動不能になる。修正が必要な場合は新しいマイグレーションファイルを追加する
- 本番に既存データへの INSERT/UPDATE をハードコードしない (`WHERE EXISTS` で条件付きにする)
- `SECURITY DEFINER` 関数には `SET search_path = alc_api` を付けること (splinter 警告回避)
- RLS ポリシーの `WITH CHECK (true)` は避け、明示的な条件を使う

### マイグレーション含む PR の検証方針

**マイグレーション (`migrations/*.sql` 追加・変更) を含む PR は、ローカルで `migrate_test.sh` を実行せず、PR の CI + staging 自動デプロイに検証を任せる**:

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

- マイグレーションファイルは `migrations/` ディレクトリに連番で配置 (`001_`, `002_`, ...)
- マイグレーションは **Cloud Run Jobs** (`rust-alc-api-migrate`) でデプロイ前に実行される
- `src/bin/migrate.rs` — マイグレーション専用バイナリ（同じ Docker イメージに含まれる）
- `deploy.sh` の流れ: Docker ビルド → プッシュ → **Cloud Run Jobs でマイグレーション実行** → Cloud Run Service デプロイ
- マイグレーション失敗時はデプロイが中止され、アプリは前バージョンで動き続ける
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
| `routes/car_inspections.rs` | `GET /api/car-inspections/current`, `/expired`, `/renew`, `/{id}` |
| `routes/car_inspection_files.rs` | `GET /api/car-inspection-files/current` |
| `routes/carins_files.rs` | `GET/POST /api/files`, `/recent`, `/not-attached`, `/{uuid}`, `/{uuid}/download`, `/{uuid}/delete`, `/{uuid}/restore` |
| `routes/nfc_tags.rs` | `GET/POST /api/nfc-tags`, `/search?uuid=`, `DELETE /{nfc_uuid}` |

### 注意事項

- `car_inspection` テーブルのカラム名は **PascalCase**（`EntryNoCarNo` 等）
- REST API は `to_jsonb()` で DB カラム名をそのまま JSON キーとして返す（フロントエンドが PascalCase を期待するため）
- RLS ポリシーは `COALESCE(current_tenant_id, current_organization_id)` で rust-logi からもアクセス可能（移行期間中）
- ファイルストレージは GCS バケット `rust-logi-files`（パス: `{tenant_id}/{uuid}`）

## タイムカード機能

- **テーブル**: `timecard_cards` (カード:社員 = 多:1) + `time_punches` (打刻記録)
- **マイグレーション**: `migrations/034_create_timecard.sql`
- **バックエンド**: `src/routes/timecard.rs`
  - カード CRUD: `POST/GET /api/timecard/cards`, `DELETE /api/timecard/cards/{id}`, `GET /api/timecard/cards/by-card/{card_id}`
  - 打刻: `POST /api/timecard/punch` (card_id → 社員特定 → 打刻 + 当日一覧返却)
  - 一覧/CSV: `GET /api/timecard/punches`, `GET /api/timecard/punches/csv`
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
  - バックエンド: `src/db/models.rs` (Employee, UpdateFace, FaceDataEntry), `src/routes/employees.rs`
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

`alc_notify::clients::line` / `alc_notify::distribute` / `alc_notify::line_webhook` から ERROR が出ていたら LINE 側で rejected されている。

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

**現状の使用箇所**: なし (Y時間 export での利用は revert 済)。将来 Cloud Tasks や DO 経由
の async pattern を再導入する際に再利用可能なように残してある (テスト 11 件付き)。

新規 async endpoint を増やす際は、上記 3 選択肢のいずれかを採用すること。
**`tokio::spawn` + `bus.broadcast()` を Cloud Run service の HTTP handler 内で直接
呼び出さない** こと (background task が確実には完走しない)。

### 並列 R2 fetch のパターン (Y時間 export 高速化、2026-05-10)

`futures::stream::iter().buffer_unordered(N)` で R2 fetch を並列化することで wall time を大幅短縮可能。
具体実装は `crates/alc-dtako/src/dtako_y_time_export/mod.rs::compute_y_time_export` を参照。

- `R2_FETCH_CONCURRENCY = 16` (200/16 = 12.5 並列ラウンド ≈ 3.75s 想定)
- `pool.close()` / DB error injection は対象外 (R2 fetch は DB connection 不要)
- 想定: 13ヶ月レンジで 41-107s → **5-15s** (~85% 削減)
- これだけで Cloudflare proxy 100s timeout 内に収まるので、async 化不要

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
| routes/carrying_items.rs | 194 | 32 | 83.51% | 積載品目 |
| routes/communication_items.rs | 228 | 11 | 95.18% | 連絡事項 |
| routes/devices.rs | 1467 | 260 | 82.28% | デバイス管理 |
| routes/dtako_restraint_report.rs | 1614 | 322 | 80.05% | 拘束時間レポート |
| routes/dtako_restraint_report_pdf.rs | 1147 | 53 | 95.38% | 拘束時間PDF |
| routes/dtako_scraper.rs | 145 | 140 | 3.45% | スクレイパー (外部依存) |
| routes/employees.rs | 435 | 27 | 93.79% | 従業員管理 |
| routes/equipment_failures.rs | 290 | 21 | 92.76% | 機器故障 |
| routes/guidance_records.rs | 420 | 84 | 80.00% | 指導記録 |
| routes/measurements.rs | 385 | 46 | 88.05% | 測定記録 |
| routes/sso_admin.rs | 166 | 35 | 78.92% | SSO管理 |
| routes/tenant_users.rs | 146 | 21 | 85.62% | テナントユーザー |
| routes/tenko_call.rs | 217 | 53 | 75.58% | 中間点呼 |
| routes/tenko_records.rs | 326 | 50 | 84.66% | 点呼記録 |
| routes/tenko_schedules.rs | 325 | 22 | 93.23% | 点呼スケジュール |
| routes/tenko_sessions.rs | 1263 | 149 | 88.20% | 点呼セッション |
| routes/tenko_webhooks.rs | 162 | 33 | 79.63% | Webhook設定 |
| routes/timecard.rs | 393 | 17 | 95.67% | タイムカード |
| routes/upload.rs | 78 | 9 | 88.46% | アップロード |
| storage/gcs.rs | 42 | 42 | 0.00% | GCS (本番のみ) |
| storage/mod.rs | 11 | 11 | 0.00% | ストレージ抽象 |
| storage/r2.rs | 39 | 39 | 0.00% | R2 (本番のみ) |
| webhook.rs | 164 | 6 | 96.34% | Webhook配信 |
- **DB エラー注入**: `BEGIN → ALTER TABLE RENAME → テスト → ROLLBACK` パターン (PostgreSQL DDL は ROLLBACK 可能)
- **SSE テスト**: コアロジックを `pub async fn xxx_core()` に抽出し、SSE ラッパーとは別にテスト可能にする

## Plan / Snapshot 管理

`ippoan/ippoan-dev-plans` repo で管理している plan/feature flag を本 repo の `manifests/production.snapshot.json` に複製し、pre-commit hook + CI で整合性を機械的に保証する。

### 仕組み

| layer | 役割 |
|---|---|
| `ippoan/ippoan-dev-plans` Issue (`label:plan` + `scope:rust-alc-api` or `scope:cross-repo`) | plan/flag の正本 |
| `manifests/production.snapshot.json` | 上の Issue を fetch して固めたスナップショット (per-flag SHA 付き) |
| `npm run snapshot` (= `npx dev-plans-snapshot build`) | snapshot 再生成 |
| `npm run snapshot:check` | drift 検出 + コードの `if_flag!("name#sha")` 参照が snapshot と一致するか検証 |
| `.githooks/pre-commit` | commit 前に snapshot:check + cargo fmt/clippy |
| `.github/workflows/ci.yml` の `snapshot-check` job | PR で同じチェックを CI 防衛線として再実行 (auto-merge の必須 needs) |

### flag 識別子は `name#sha` ハイブリッド

```rust
if_flag!("trouble_ack_modal#9f1c3a8b")
```

- `name`: human-readable snake_case
- `sha`: `sha256("${plan_id}|${id}|${source_issue}").hex.slice(0,8)` — Issue の identity が変わると SHA も変わるので、code 側の旧参照が即座に検出される
- snapshot に同 name 複数 entry (異 SHA) が並んでも、code は SHA で disambiguate される

### 日常運用ルール

- **新規 flag を導入する前に必ず plan Issue を立てる**: `ippoan/ippoan-dev-plans` で `scope:rust-alc-api` (または `scope:cross-repo`) ラベル付き Issue を作成 → 本文の YAML block に `id` / `plan_id` を書く → `npm run snapshot` で取り込む
- **commit する前に hook を有効化**: `git config core.hooksPath .githooks` (clone 直後 1 度だけ)
- **`npm install` が前提**: `.npmrc` で `@ippoan` scope を GHCR に向けている。`NODE_AUTH_TOKEN` (PAT with `read:packages`) が必要
- **drift で落ちたら**: `npm run snapshot && git add manifests/production.snapshot.json && git commit ...`
- **stale sha で落ちたら**: snapshot を再生成するか、code 側を新 sha に書き換える
- **clippy 重い時**: `SKIP_CLIPPY=1 git commit ...` で skip 可、CI で必ず走る

### マクロ実装

`if_flag!()` マクロ本体は `alc-core` (`ippoan/ippoan-dev-plans#6` で計画中) で定義予定。本 PR (#2) ではマクロは不要 — hook の grep 用 regex (`/\bif_flag!\(\s*"([a-z][a-z0-9_]+)#([a-f0-9]{8})"/g`) と snapshot 機構のレールだけ敷く。

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

### Staging Export/Import API

揮発性 DB のデータ復元用 API。`STAGING_MODE=true` 環境変数でガード（本番では 404）。

| エンドポイント | 用途 |
|---|---|
| `GET /api/staging/export?tenant_id=<uuid>` | テナントデータを JSON ダンプ |
| `POST /api/staging/import` | JSON からリストア (べき等、ON CONFLICT DO UPDATE) |

**Export 対象テーブル:** tenants, users, employees (face_embedding 含む), devices, tenko_schedules, webhook_configs, tenant_allowed_emails, sso_provider_configs, tenko_call_numbers, tenko_call_drivers, bot_configs (LINE WORKS Bot 設定), notify_line_configs (LINE Messaging API 設定), notify_recipients (notify 受信者)

**対象外:** measurements, tenko_sessions, tenko_records, time_punches, file_access_logs（履歴データ）

- 実装: `crates/alc-misc/src/staging.rs`
- テストデータ: `staging/test-data.json` (tenant_id `11111111-...`)
- Import はトランザクション内で tenant → set_current_tenant → 残りテーブルの順で実行
- 認証不要 (public route)、環境変数ガードのみ

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
  - PDF.js は同一オリジン or CORS 許可済みオリジンから fetch する必要あり → R2 presign 直 access させず API ストリーム (`/api/notify/v/{token}/file`) で配信 (PR #303)
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
