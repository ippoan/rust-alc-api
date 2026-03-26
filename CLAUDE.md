# rust-alc-api

Axum + PostgreSQL RLS による ALC (アルコールチェック) API バックエンド

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
# ユニットテストのみ (DB 不要)
cargo test --lib

# マイグレーション検証 (Docker 必要)
bash ~/.claude/skills/migrate-test/scripts/migrate_test.sh

# インテグレーションテスト (Docker 必要)
docker compose up -d test-db
# DB 起動待ち
until pg_isready -h localhost -p 54322 -q; do sleep 1; done
TEST_DATABASE_URL="postgresql://postgres:test@localhost:54322/postgres?options=-c search_path=alc_api" \
  cargo test --test '*' -- --test-threads=1
docker compose down

# 全テスト一括 (fmt + clippy + unit + migration + integration + frontend)
./test_and_deploy.sh

# テスト + デプロイ
./test_and_deploy.sh --deploy

# オプション
./test_and_deploy.sh --skip-integration   # インテグレーションテストをスキップ
./test_and_deploy.sh --skip-frontend      # フロントエンドテストをスキップ
```

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

- 本番に既存データへの INSERT/UPDATE をハードコードしない (`WHERE EXISTS` で条件付きにする)
- `SECURITY DEFINER` 関数には `SET search_path = alc_api` を付けること (splinter 警告回避)
- RLS ポリシーの `WITH CHECK (true)` は避け、明示的な条件を使う
- 作成・変更後は `bash ~/.claude/skills/migrate-test/scripts/migrate_test.sh` で検証

## マイグレーションとデプロイ

- マイグレーションファイルは `migrations/` ディレクトリに連番で配置 (`001_`, `002_`, ...)
- アプリ起動時に `sqlx::migrate!("./migrations").run(&pool)` で**自動適用**される
- `deploy.sh` は Docker ビルド時に `migrations/` をイメージに含めるため、**デプロイするだけでマイグレーションも適用される**
- 手動で psql を実行する必要はない

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

### devices テーブル: SELECT ポリシーがテナント分離を無効化

- `migrations/040_devices_select_by_id_policy.sql` で `device_select_by_id ON alc_api.devices FOR SELECT USING (true)` を追加
- これにより `tenant_isolation_devices` (`USING (tenant_id = current_setting(...))`) が SELECT で無効化される
- PostgreSQL は同一コマンドの複数ポリシーを OR で評価するため、`true` があると全行が見える
- **影響**: `list_devices` が全テナントのデバイスを返す。UPDATE/DELETE も `device_select_by_id` の USING(true) で行が見つかるため、他テナントのデバイスを変更可能
- **対策案**: `device_select_by_id` を `FOR SELECT USING (id = ANY(...))` に制限するか、`list_devices` クエリに `WHERE tenant_id = $1` を明示追加

### tenko_call_numbers テーブル: INSERT/DELETE 権限なし

- `migrations/031_tenko_call_numbers.sql` で `GRANT SELECT` のみ付与、INSERT/UPDATE/DELETE なし
- `create_number` / `delete_number` エンドポイントが本番で 500 になる
- **対策**: `GRANT INSERT, UPDATE, DELETE ON tenko_call_numbers TO alc_api_app` + SEQUENCE 権限追加

## テスト

- テストインフラ: `docker-compose.yml` (PostgreSQL 16, port 54322) + `tests/common/mod.rs` ヘルパー
- 実行: `source .test-config && cargo test`
- カバレッジ: `source .test-config && cargo llvm-cov --summary-only`
- 現在のカバレッジ: 69.85% line / 73.68% region (361テスト)
- カバレッジ計画: `plans/coverage_100.md`

## デプロイルール

- コードの修正・変更が完了したら、デプロイするかどうかを **AskUserQuestion ツールの選択肢形式** で確認すること
- 選択肢: 「デプロイする」「デプロイしない」の2択で提示
- 確認なしに `deploy.sh` を実行してはいけない
- デプロイコマンド: `./deploy.sh` (Cloud Run へデプロイ)
