# バックエンドカバレッジ 100% 計画

## 現状: 66.52% line / 70.31% region (327テスト)

### 達成済み (28% → 66%)

| Phase | 内容 | テスト数 | カバレッジ |
|-------|------|---------|-----------|
| 1 | auth/employees/measurements/devices/health | +79 | 28% → 41% |
| 2 | tenko sessions/schedules/records + admin | +50 | 41% → 53% |
| 3 | carins/upload/dtako list + OAuth redirect | +40 | 53% → 56% |
| 4 | dtako upload ZIP + PDF生成 + operations CRUD | +30 | 56% → 62% |
| 5 | restraint report JSON + tenko error paths | +25 | 62% → 66% |

### テストファイル一覧

| ファイル | テスト数 | カバー対象 |
|---------|---------|-----------|
| tests/auth_test.rs | 26 | auth middleware + refresh/logout/me + OAuth redirect |
| tests/employees_test.rs | 19 | CRUD + face/nfc/license |
| tests/measurements_test.rs | 20 | CRUD + filter + proxy |
| tests/devices_test.rs | 38 | 3フロー + FCM + settings |
| tests/tenko_test.rs | 31 | schedules + sessions フルフロー + records |
| tests/admin_test.rs | 10 | tenant_users + SSO + bot |
| tests/dtako_test.rs | 32 | upload ZIP + operations + report + PDF |
| tests/carins_test.rs | 13 | files CRUD + upload multipart |
| tests/misc_routes_test.rs | 32 | timecard/tenko_call/carrying/communication/guidance/daily_health |
| tests/health_test.rs | 1 | health endpoint |

## 残り: 66% → 100% へのリファクタ計画

### Phase 6: trait 抽出 + モック注入 (66% → 80%)

#### 6-1. FcmSender trait 化
- **対象**: `src/fcm.rs` (72行) + `src/routes/devices.rs` FCM 系エンドポイント (~500行)
- **変更**:
  - `FcmSender` を trait に変更 (`send_data_message` メソッド)
  - `AppState.fcm: Option<Arc<dyn FcmSenderTrait>>` に変更
  - テスト用 `MockFcmSender` を作成 (送信記録を保持)
- **テスト**: fcm-notify-call, test-fcm, test-fcm-all, trigger-update が全パス通過
- **見込み**: +500行カバー

#### 6-2. Google OAuth verifier モック
- **対象**: `src/auth/google.rs` (147行) + `src/routes/auth.rs` google_login/code_login (~200行)
- **変更**:
  - `GoogleTokenVerifier` に `verify_id_token` trait メソッド追加
  - テスト用モックが固定の claims を返す
- **テスト**: google_login + google_code_login の成功パス
- **見込み**: +300行カバー

#### 6-3. LINE WORKS OAuth モック
- **対象**: `src/auth/lineworks.rs` (247行) + `src/routes/auth.rs` lineworks callback/WOFF (~200行)
- **変更**:
  - token exchange + profile fetch を trait 化
  - テスト用モックが固定ユーザーを返す
- **テスト**: lineworks callback + WOFF auth の成功パス
- **見込み**: +400行カバー

### Phase 7: SSE + 内部ロジック分離 (80% → 90%)

#### 7-1. dtako_upload recalculate ロジック分離
- **対象**: `src/routes/dtako_upload.rs` recalculate_driver/batch (~700行)
- **変更**:
  - 計算コアロジックを `fn recalculate_core(...)` に分離
  - SSE wrapper は薄いラッパーに
- **テスト**: recalculate_core を直接呼び出すユニットテスト
- **見込み**: +500行カバー

#### 7-2. dtako_restraint_report build_report 詳細テスト
- **対象**: `src/routes/dtako_restraint_report.rs` build_report_with_name_conn (~500行)
- **変更**: テスト用の operations/daily_hours データを DB に直接 INSERT
- **テスト**: 各計算パス (拘束時間, 休息時間, 週次集計) をカバー
- **見込み**: +400行カバー

#### 7-3. webhook 配信テスト (wiremock)
- **対象**: `src/webhook.rs` deliver_webhook (~150行)
- **変更**: `wiremock` crate を dev-dependency に追加、mock HTTP server でテスト
- **テスト**: fire_event → deliver_webhook → HTTP POST → delivery record 作成
- **見込み**: +200行カバー

### Phase 8: 残りモップアップ (90% → 100%)

- `main.rs` (237行): エントリポイント — AppState 構築 + サーバー起動。テスト困難だが lib.rs 分離で一部カバー可能
- `csv_parser/mod.rs` (70行): group_csv_by_unko_no + csv_header のユニットテスト追加
- `dtako_scraper.rs` (212行): 外部サイトスクレイピング — HTTPモック
- `storage/gcs.rs` (65行) + `storage/r2.rs` (59行): 実ストレージテストまたはモック
- `compare/mod.rs` 残り (329行): エッジケースユニットテスト

## カバレッジ計測コマンド

```bash
source .test-config && cargo test
source .test-config && cargo llvm-cov --summary-only
source .test-config && cargo llvm-cov --html --open
```
