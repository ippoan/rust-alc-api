# バックエンドカバレッジ 100% 計画

## 現状: 73.09% line / 76.47% region (~430テスト)

### 達成済み (28% → 73%)

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
| 7 | 大量エッジケース (subagent x3) + dtako report | +50 | 72% → 73% |

### 完了済みリファクタ

- **FcmSender trait化**: `src/fcm.rs` に `FcmSenderTrait` 追加、`AppState.fcm: Option<Arc<dyn FcmSenderTrait>>`
- **GoogleTokenVerifier test_claims**: `with_test_claims()` コンストラクタで `"test-valid-token"` / `"test-valid-code"` で固定 claims 返却
- **MockFcmSender**: `tests/common/mod.rs` に送信記録保持モック
- **dtako_storage**: `setup_app_state` に MockStorage 注入済み

### テストファイル一覧

| ファイル | テスト数 | カバー対象 |
|---------|---------|-----------|
| tests/auth_test.rs | ~44 | middleware + refresh/logout/me + OAuth redirect/callback + Google login成功 |
| tests/employees_test.rs | ~27 | CRUD + face flow (pending→approve/reject) + nfc/license |
| tests/measurements_test.rs | ~27 | CRUD + filter + proxy + COALESCE partial update |
| tests/devices_test.rs | ~48 | 3フロー + FCM notify/test/trigger + schedule/exclude |
| tests/tenko_test.rs | ~64 | schedules CRUD + sessions フルフロー + records CSV + edge cases |
| tests/admin_test.rs | ~19 | tenant_users + SSO + bot CRUD + viewer forbidden |
| tests/dtako_test.rs | ~36 | upload ZIP + operations + report JSON/PDF + recalculate |
| tests/carins_test.rs | ~13 | files CRUD + download + upload multipart |
| tests/misc_routes_test.rs | ~50 | timecard/tenko_call/carrying/communication/guidance/daily_health/nfc_tags |
| tests/health_test.rs | 1 | health endpoint |
| unit tests (lib) | ~120 | jwt/compare/csv_parser/lineworks |

### 既知の問題

- **並列テスト env var 競合**: `OAUTH_STATE_SECRET`, `FCM_INTERNAL_SECRET`, `JWT_SECRET` が `set_var/remove_var` で競合。`RUST_TEST_THREADS=1` で全通過
- **RLS 問題**: `device_select_by_id FOR SELECT USING (true)` がテナント分離を無効化
- **テーブル未作成**: `tenko_carrying_item_checks`, `carrying_item_vehicle_conditions`
- **権限不足**: `tenko_call_numbers` に INSERT 権限なし

---

## 残り: 73% → 100% (未カバー ~7200行)

### 未カバー行数トップ

| ファイル | 未カバー行 | 現在% | 難易度 | 必要な対応 |
|---------|-----------|-------|--------|-----------|
| dtako_upload.rs | 1664 | 42% | 高 | SSE内部ロジック分離 |
| dtako_restraint_report.rs | 844 | 50% | 中 | テストデータ直接INSERT |
| dtako_restraint_report_pdf.rs | 649 | 62% | 中 | PDF内部レンダリング関数テスト |
| devices.rs | 530 | 72% | 低 | FCM dismiss/test内部分岐 |
| auth.rs | 321 | 59% | 高 | lineworks callback (resolve_sso_config必要) |
| tenko_sessions.rs | 327 | 80% | 低 | 残りwebhook発火パス |
| compare/mod.rs | 329 | 94% | 低 | エッジケースユニットテスト |
| tenko_records.rs | 125 | 78% | 低 | CSV export内部 |
| webhook.rs | 238 | 8% | 高 | wiremock外部HTTP配信 |
| main.rs | 237 | 0% | 不可 | エントリポイント |
| dtako_scraper.rs | 212 | 4% | 高 | 外部サイトスクレイピングモック |
| auth/google.rs | 141 | 18% | 中 | JWKS fetch/verify (テストモード以外) |
| auth/lineworks.rs | 90 | 81% | 高 | exchange_code/fetch_user_profile (外部API) |
| storage/gcs.rs | 65 | 0% | 中 | GCS API モック |
| storage/r2.rs | 59 | 0% | 中 | R2 API モック |

### Phase 7: SSE内部ロジック分離 (73% → 80%)

#### 7-1. dtako_upload calculate_daily_hours テストデータ拡充
- **対象**: `calculate_daily_hours` (470-884行, ~400行)
- **方法**: テスト用ZIPにもっと多くのKUDGURI/KUDGIVT行を含める（複数運行、複数日、フェリー区間）
- **効果**: ZIP upload テストでより多くの内部分岐を通過
- **見込み**: +300行カバー

#### 7-2. dtako_upload recalculate コアロジック分離
- **対象**: `recalculate_driver` (1786-2008行) + `recalculate_drivers_batch` (2009-2245行)
- **変更**:
  - SSE ストリーム生成部分と計算ロジックを分離
  - `async fn recalculate_driver_core(pool, tenant_id, driver_id, year, month) -> Result<RecalcResult>`
- **テスト**: recalculate_driver_core を直接呼ぶインテグレーションテスト
- **見込み**: +400行カバー

#### 7-3. dtako_restraint_report build_report 詳細テスト
- **対象**: `build_report_with_name_conn` (211-660行, ~450行)
- **方法**: DB に直接 `dtako_operations` + `dtako_daily_work_hours` を INSERT して各計算パスを通す
  - 拘束時間計算 (出社～退社)
  - 休息時間計算 (退社～翌出社)
  - 週次小計 (weekly_subtotals)
  - 月次合計 (monthly_total)
  - 時間外/深夜計算
- **見込み**: +400行カバー

### Phase 8: 外部依存モック (80% → 90%)

#### 8-1. webhook 配信テスト
- **対象**: `src/webhook.rs` deliver_webhook (~200行)
- **変更**: `wiremock` crate を dev-dependency に追加
- **テスト**:
  - fire_event で webhook config あり → deliver_webhook → mock HTTP server に POST
  - HMAC 署名検証
  - delivery record DB 保存
  - リトライロジック
- **見込み**: +200行カバー

#### 8-2. lineworks callback 成功パス
- **対象**: `src/routes/auth.rs` lineworks_callback (544-660行)
- **前提**: `resolve_sso_config` SECURITY DEFINER 関数のテストDB版
- **方法**:
  1. テストDBに `sso_provider_configs` を INSERT
  2. `resolve_sso_config` 関数を作成 (マイグレーションに含まれているか確認)
  3. 有効な state + test code → lineworks callback 呼び出し
  4. ただし `exchange_code`/`fetch_user_profile` は外部API → wiremock or trait化
- **変更案**: `lineworks::exchange_code` / `fetch_user_profile` を trait 化するか、テスト用 HTTP server
- **見込み**: +300行カバー

#### 8-3. dtako_scraper モック
- **対象**: `src/routes/dtako_scraper.rs` (221行)
- **変更**: ScraperUrl をテスト用 mock server に向ける
- **テスト**: スクレイピング結果のパース
- **見込み**: +200行カバー

#### 8-4. storage モック拡充
- **対象**: `storage/gcs.rs` (65行) + `storage/r2.rs` (59行)
- **方法**: MockStorage は既に使用中。gcs.rs/r2.rs の本物の実装をカバーするには実ストレージ接続が必要
- **代替**: ユニットテストで URL 生成/キー抽出のみテスト
- **見込み**: +50行カバー

### Phase 9: 残りモップアップ (90% → 100%)

- **main.rs** (237行): エントリポイント — lib.rs にロジック移動で一部カバー可能。残りは `#[cfg(not(test))]` で除外するか許容
- **compare/mod.rs** (329行, 94%): 残りは特殊な日付境界ケース。ユニットテスト追加
- **auth/google.rs** (141行): JWKS fetch/verify の内部ロジック — テストモードでバイパスされるため、JWKS モックが必要
- **dtako_restraint_report_pdf.rs** (649行): PDF 内部レンダリング (`draw_table`, `draw_header` 等) — printpdf のモックは不要、実際に生成させてバイト数チェック

### 並列テスト env var 競合の恒久対策

現在 `RUST_TEST_THREADS=1` で回避中。恒久対策:
1. **テスト専用 env var**: `OAUTH_STATE_SECRET` 等を `.test-config` で統一設定し、テスト内で `set_var/remove_var` しない
2. **spawn_test_server 内で env var 設定**: server 起動時に一度だけ設定
3. **once_cell / lazy_static**: テスト初期化で一度だけ設定

## カバレッジ計測コマンド

```bash
# テスト実行 (シリアル — env var 競合回避)
source .test-config && RUST_TEST_THREADS=1 cargo test

# カバレッジ (シリアル)
source .test-config && RUST_TEST_THREADS=1 cargo llvm-cov --summary-only

# HTML レポート
source .test-config && RUST_TEST_THREADS=1 cargo llvm-cov --html --open
```
