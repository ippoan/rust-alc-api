# rust-alc-api

Axum + PostgreSQL RLS の ALC (アルコールチェック) API。Cloud Run にデプロイ。
詳細は `rust-alc-api-map` skill 参照 (末尾「CLAUDE.md から移設」に旧全文を verbatim 収載)。

## コマンド

`make test` (unit) / `make db-up` + `make itest` (integration) / `./test_and_deploy.sh` (全部) / カバレッジ `/coverage-check`

## 規範 (must / never)

- DB は `alc_api_app` ユーザーで。monolith (session スコープの `set_current_tenant`) は必ず直接接続 port 5432 (6543 は set_config リセットで RLS が壊れる)。worker (`workers/*`) は `in_tenant_tx` でトランザクション単位の RLS (`set_config(..., true)`) にする場合に限り、プーラーの 6543 を使う
- 適用済み migration は絶対に変更しない (checksum で起動不能) — 修正は新規ファイル追加
- migration: `SECURITY DEFINER` に `SET search_path = alc_api` 必須 / `WITH CHECK (true)` は避ける / 既存データへの INSERT/UPDATE ハードコード禁止 (`WHERE EXISTS`)
- migration PR はローカルで流さず CI + staging に任せる
- migration の正本は ippoan/alc-migrations。rust-alc-api の `migrations/` とテスト DB 用 SQL (`scripts/init_local_db.sql`・`scripts/local_app_grants.sql`) は切り替えまで追加・変更禁止 (CI の `pr-limit` で止まる)
- main 直接 merge/push・`git checkout main`・メイン worktree のソース編集は禁止 — コード変更は必ず origin/main ベースの worktree で (hooks 強制)。削除前に必ず repo root へ cd
- 確認なしの `deploy.sh` 実行禁止 (AskUserQuestion 2 択で確認)。本番デプロイは `/tag-release patch` のみ
- Cloud Run handler 内 `tokio::spawn` fire-and-forget 禁止 (CPU throttle で完走しない)
- render.sh / workflows に値ハードコード禁止 — Secret Manager + secretKeyRef。新 secret は per-secret grant、更新後は `gcloud run deploy` で新 revision 必須
- coverage gate 対象ファイルで `tracing` マクロを複数行にしない
- unit test を本番 DB/API に直叩きしない / 外部 API URL は const にせず struct フィールド化 (wiremock)
- Gemini `generationConfig` は必ず `responseMimeType` + `responseSchema` 両方注入
- webview PDF inline は PDF.js canvas 描画必須 / alc-app `wrangler.jsonc` はトップレベル `vars` 必須
- AlcoholChecker: 指示なき限りパッチのみ上げる
