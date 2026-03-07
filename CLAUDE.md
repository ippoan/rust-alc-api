# rust-alc-api

Axum + PostgreSQL RLS による ALC (アルコールチェック) API バックエンド

## プロジェクト構成

- **バックエンド**: Rust / Axum
- **認証**: Google Sign-In JWT
- **DB**: Supabase PostgreSQL (`alc_api` スキーマ、`alc_api_app` ロール NOBYPASSRLS)
- **ストレージ**: Cloudflare R2 (`alc-face-photos` バケット) / GCS 切り替え可能
- **デプロイ**: Cloud Run (`deploy.sh`)

## DB 接続の重要事項

- Supabase は rust-logi と同じプロジェクト (`tvbjvhvslgdwwlhpkezh`)、`alc_api` スキーマで分離
- `alc_api_app` ユーザーで接続すること（NOBYPASSRLS → RLS が有効）
- 必ず **直接接続 (port 5432)** を使用（Supavisor port 6543 は set_config がリセットされる）
- `DATABASE_URL` に `?options=-c search_path=alc_api` を付けてスキーマ指定

## ストレージバックエンド切り替え

- `STORAGE_BACKEND=r2` → Cloudflare R2 (`rust-s3` crate)
- `STORAGE_BACKEND=gcs` → GCS (reqwest 直接呼び出し、Cloud Run メタデータサーバー認証)
- `StorageBackend` trait で抽象化 (`src/storage/`)

## シンボリックリンク（参照用）

プロジェクトルートに関連リポジトリへのシンボリックリンクを配置している。
`.gitignore` に登録済み。VSCode の `git.scanRepositories` で git 操作可能。

| リンク名 | リンク先 | 説明 |
|---|---|---|
| `alc-app` | `/home/yhonda/js/alc-app` | フロントエンド (Nuxt) |
| `rust-nfc-bridge` | `/home/yhonda/rust/rust-nfc-bridge` | NFC ブリッジ (Rust) |
| `ble-medical-gateway` | `/home/yhonda/arduino/ble-medical-gateway` | BLE Medical Gateway (Arduino) |

## ユーティリティ

- `git-status-all.sh` — 自身 + シンボリックリンク先の全リポジトリの git status を一括表示

## マイグレーションとデプロイ

- マイグレーションファイルは `migrations/` ディレクトリに連番で配置 (`001_`, `002_`, ...)
- アプリ起動時に `sqlx::migrate!("./migrations").run(&pool)` で**自動適用**される
- `deploy.sh` は Docker ビルド時に `migrations/` をイメージに含めるため、**デプロイするだけでマイグレーションも適用される**
- 手動で psql を実行する必要はない

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

## デプロイルール

- コードの修正・変更が完了したら、デプロイするかどうかを **AskUserQuestion ツールの選択肢形式** で確認すること
- 選択肢: 「デプロイする」「デプロイしない」の2択で提示
- 確認なしに `deploy.sh` を実行してはいけない
- デプロイコマンド: `./deploy.sh` (Cloud Run へデプロイ)
