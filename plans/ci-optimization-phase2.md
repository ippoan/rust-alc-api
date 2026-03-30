# CI 最適化 Phase 2: integration→mock 変換 + CI 簡素化

## 背景

PR #56 で mock テスト統合 (37→1 バイナリ) + integration テスト削減 (559→4 RLS テスト) を実施。
CI: 6.5分 → 2.5分に短縮。

ただし coverage_100.toml に 13 ファイルが `type="integration"` or `type="both"` で残っており、
mock だけでは 100% 未達。これらを mock 化しないと integration テストを完全に削除できない。

PR #57 (integration テスト復元) はクローズする。integration テストを戻すのではなく mock 化する。

## Step 1: PR #57 クローズ + ブランチ削除

```bash
gh pr close 57 --comment "Superseded: mock化で対応"
git branch -D fix/restore-integration-tests
git push origin --delete fix/restore-integration-tests
```

## Step 2: coverage_100.toml から integration/both ファイルを一時除外

13 ファイルを toml から削除（mock 化完了後に再登録）:

```
src/auth/google.rs          (integration) → 除外
src/auth/jwt.rs             (both)        → type="unit" に変更 (unit だけで 100% か検証)
src/csv_parser/kudgivt.rs   (both)        → type="unit" に変更 (同上)
src/csv_parser/kudguri.rs   (both)        → type="unit" に変更 (同上)
src/auth/lineworks.rs       (both)        → 除外
src/compare/mod.rs          (both)        → 除外
src/routes/auth.rs          (integration) → 除外
src/routes/devices.rs       (integration) → 除外
src/routes/dtako_scraper.rs (integration) → 除外
src/routes/dtako_upload.rs  (integration) → 除外
src/routes/dtako_restraint_report.rs (integration) → 除外
src/routes/tenko_sessions.rs (both)       → 除外
src/webhook.rs              (integration) → 除外
```

"both" のうち jwt, kudgivt, kudguri は unit テストだけで 100% の可能性あり → 先に検証。

## Step 3: CI を 3 ジョブに簡素化

```yaml
jobs:
  check:       # fmt + clippy (並列)
  unit-tests:  # cargo test --lib + unit coverage check (並列)
  mock-tests:  # cargo llvm-cov --test mock_tests + mock coverage check + artifact (並列)
```

- full-tests ジョブ削除 (DB 不要)
- RLS テスト 4 個は mock_tests バイナリに移動するか、一旦除外
- mold + debug=false は mock-tests ジョブだけに適用

## Step 4: Wave 2-4 で mock 化 (メイン作業)

memory/mock_test_phase4b.md の計画に従う:

### Wave 2: webhook trait 化
- `webhook.rs` → WebhookRepository trait 新設
- `tenko_sessions.rs` → pool 依存除去
- mock テスト追加 → type="mock" に変更

### Wave 3: 外部 HTTP mock (wiremock)
- `auth/google.rs` — JWKS fetch + code exchange
- `auth/lineworks.rs` — LINE WORKS token exchange + user profile
- `routes/dtako_scraper.rs` — SSE proxy
- mock テスト追加 → type="mock" に変更

### Wave 4: ビジネスロジック充実
- `routes/auth.rs` — Google/LINE WORKS OAuth フロー
- `routes/devices.rs` — FCM/OTA/registration
- `routes/dtako_upload.rs` — ZIP/CSV 処理
- `routes/dtako_restraint_report.rs` — 日付計算・集計
- `compare/mod.rs` — 比較ロジック
- mock テスト追加 → type="mock" に変更

### 各 Wave 完了時
- coverage_100.toml に type="mock" で再登録
- CI の mock coverage check で 100% 検証

## Step 5: 全ファイル mock 化完了後

- CI: check + unit + mock の 3 ジョブ (DB 不要、全並列)
- RLS テスト: 週次 or 手動実行のワークフローに移動
- 目標 CI 時間: ~2分

## 対象ファイル一覧

| ファイル | 現 type | Wave | mock 化の障壁 |
|---------|---------|------|--------------|
| webhook.rs | integration | 2 | pool 引数 → trait 化 |
| tenko_sessions.rs | both | 2 | webhook + pool 依存 |
| auth/google.rs | integration | 3 | 外部 HTTP (JWKS) |
| auth/lineworks.rs | both | 3 | 外部 HTTP (LINE WORKS) |
| dtako_scraper.rs | integration | 3 | 外部 SSE proxy |
| auth.rs | integration | 4 | Google/LW OAuth フロー |
| devices.rs | integration | 4 | FCM + 複雑な状態遷移 |
| dtako_upload.rs | integration | 4 | ZIP/CSV パース |
| dtako_restraint_report.rs | integration | 4 | 集計ロジック |
| compare/mod.rs | both | 4 | 比較ロジック |
| jwt.rs | both | - | unit だけで OK? |
| kudgivt.rs | both | - | unit だけで OK? |
| kudguri.rs | both | - | unit だけで OK? |
