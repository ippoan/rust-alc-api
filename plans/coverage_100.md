# バックエンドカバレッジ 100% 計画

## 現状: 82.63% line / 85.43% region (~530テスト)

### 達成済み (28% → 79%)

| Phase | 内容 | テスト数 | カバレッジ |
|-------|------|---------|-----------|
| 1 | auth/employees/measurements/devices/health | +79 | 28% → 41% |
| 2 | tenko sessions/schedules/records + admin | +50 | 41% → 53% |
| 3 | carins/upload/dtako list + OAuth redirect | +40 | 53% → 56% |
| 4 | dtako upload ZIP + PDF生成 + operations CRUD | +30 | 56% → 62% |
| 5 | restraint report JSON + tenko error paths | +25 | 62% → 66% |
| 6-1 | FcmSender trait化 + MockFcmSender | +10 | 66% → 68% |
| 6-2 | GoogleTokenVerifier test_claims + callback成功 | +10 | 68% → 71% |
| 6-3 | lineworks ユニットテスト + csv_parser | +20 | 71% → 72% |
| 7-prev | 大量エッジケース (subagent x3) + dtako report | +50 | 72% → 73% |
| 7 | SSE分離 + リッチZIP + restraint計算検証 | +18 | 73% → 76% |
| 8 | webhook配信 + PDF実データ + check_overdue | +12 | 76% → 79% |
| 9 | safety judgment + restraint ops + rich upload E2E | +8 | 79% → 79% |
| 10 | dtako_upload 100%化 (SSE core抽出 + エラー注入 + dead code削除) | +80 | 79% → 83% |

### 完了済みリファクタ

- **FcmSender trait化**: `src/fcm.rs` に `FcmSenderTrait` 追加、`AppState.fcm: Option<Arc<dyn FcmSenderTrait>>`
- **GoogleTokenVerifier test_claims**: `with_test_claims()` コンストラクタで `"test-valid-token"` / `"test-valid-code"` で固定 claims 返却
- **MockFcmSender**: `tests/common/mod.rs` に送信記録保持モック
- **dtako_storage**: `setup_app_state` に MockStorage 注入済み
- **recalculate_driver_core 抽出**: SSE ラッパーとコア計算を分離、`load_driver_operations` / `process_single_driver_batch` 共通化

### テストファイル一覧

| ファイル | テスト数 | カバー対象 |
|---------|---------|-----------|
| tests/auth_test.rs | ~44 | middleware + refresh/logout/me + OAuth redirect/callback + Google login成功 |
| tests/employees_test.rs | ~27 | CRUD + face flow (pending→approve/reject) + nfc/license |
| tests/measurements_test.rs | ~27 | CRUD + filter + proxy + COALESCE partial update |
| tests/devices_test.rs | ~48 | 3フロー + FCM notify/test/trigger + schedule/exclude |
| tests/tenko_test.rs | ~69 | schedules CRUD + sessions フルフロー + records CSV + webhook配信 + check_overdue |
| tests/admin_test.rs | ~19 | tenant_users + SSO + bot CRUD + viewer forbidden |
| tests/dtako_test.rs | ~52 | upload ZIP(rich) + reupload + recalculate_driver_core + restraint計算検証(8パターン) + PDF実データ |
| tests/carins_test.rs | ~13 | files CRUD + download + upload multipart |
| tests/misc_routes_test.rs | ~58 | timecard/tenko_call/carrying/communication/guidance/daily_health/nfc_tags |
| tests/health_test.rs | 1 | health endpoint |
| unit tests (lib) | ~120 | jwt/compare/csv_parser/lineworks |

### 既知の問題

- **並列テスト env var 競合**: `OAUTH_STATE_SECRET`, `FCM_INTERNAL_SECRET`, `JWT_SECRET` が `set_var/remove_var` で競合。`RUST_TEST_THREADS=1` で全通過
- **RLS 問題**: `device_select_by_id FOR SELECT USING (true)` がテナント分離を無効化
- **テーブル未作成**: `tenko_carrying_item_checks`, `carrying_item_vehicle_conditions`
- **権限不足**: `tenko_call_numbers` に INSERT 権限なし

---

## 残り: 79% → 100% (未カバー ~5740行)

### 未カバー行数トップ (2026-03-26 時点)

| ファイル | 未カバー行 | 現在% | 難易度 | 必要な対応 |
|---------|-----------|-------|--------|-----------|
| dtako_upload.rs | 1197 | 57% | 高 | 残りSSE内部 (internal_upload/split_csv/load_kudgivt) |
| dtako_restraint_report.rs | 612 | 64% | 中 | 残り比較CSV + 内部分岐 |
| devices.rs | 514 | 73% | 低 | FCM dismiss/test内部分岐 |
| tenko_sessions.rs | 327 | 80% | 低 | 残りwebhook発火パス |
| compare/mod.rs | 327 | 94% | 低 | エッジケースユニットテスト |
| auth.rs | 321 | 59% | 高 | lineworks callback (resolve_sso_config必要) |
| main.rs | 237 | 0% | 不可 | エントリポイント |
| dtako_scraper.rs | 212 | 4% | 高 | 外部サイトスクレイピングモック |
| auth/google.rs | 141 | 18% | 中 | JWKS fetch/verify (テストモード以外) |
| tenko_records.rs | 125 | 78% | 低 | CSV export内部 |
| guidance_records.rs | 143 | 72% | 低 | 残り CRUD パス |
| auth/lineworks.rs | 90 | 81% | 高 | exchange_code/fetch_user_profile (外部API) |
| storage/gcs.rs | 65 | 0% | 中 | GCS API モック |
| storage/r2.rs | 59 | 0% | 中 | R2 API モック |
| dtako_restraint_report_pdf.rs | 64 | 96% | 低 | ほぼ完了 |
| webhook.rs | 24 | 91% | 低 | ほぼ完了 |

### Phase 9: 低難易度モップアップ (79% → 85%)

#### 9-1. devices.rs 残り分岐 (+300行)
- FCM dismiss/test/trigger の内部エラーパス
- device login/logout の追加ケース

#### 9-2. tenko_sessions.rs 残りパス (+200行)
- webhook発火パス (alcohol_detected → fire_event)
- interrupt/resume の追加エッジケース

#### 9-3. guidance_records/tenko_records/carrying_items (+200行)
- CSV export の detail 内部
- 子レコード・フィルターの組み合わせ

### Phase 10: 外部依存モック (85% → 92%)

#### 10-1. auth.rs lineworks callback (+300行)
- `resolve_sso_config` テストDB版
- exchange_code/fetch_user_profile をテスト用 HTTP server でモック
- state HMAC 検証成功パス

#### 10-2. dtako_scraper モック (+200行)
- ScraperUrl をテスト用 mock server に向ける
- スクレイピング結果のパース

#### 10-3. storage ユニットテスト (+50行)
- gcs.rs/r2.rs の URL 生成/キー抽出のみユニットテスト

#### 10-4. auth/google.rs JWKS モック (+100行)
- JWKS fetch/verify の内部ロジック

### Phase 11: 残り (92% → 100%)

- **main.rs** (237行): lib.rs にロジック移動で一部カバー可能。残りは許容
- **compare/mod.rs** (327行, 94%): 特殊な日付境界ケース。ユニットテスト追加
- **dtako_upload.rs** (1197行): internal_upload_zip/split_csv は dead code の可能性あり → 未使用関数削除で行数削減

## カバレッジ計測コマンド

```bash
# テスト実行 (シリアル — env var 競合回避)
source .test-config && RUST_TEST_THREADS=1 cargo test

# カバレッジ (シリアル)
source .test-config && RUST_TEST_THREADS=1 cargo llvm-cov --summary-only

# HTML レポート
source .test-config && RUST_TEST_THREADS=1 cargo llvm-cov --html --open
```
