# alc-vein Worker

指静脈 (vein) の 4 本の口 (crates/alc-vein) を workers-rs + Hyperdrive で提供する Worker。
rust-alc-api を Cloudflare Workers へ段階移行する最初の 1 本 (Refs #680 / #683)。
口・照合・trait・SQL (`repo::sql`) は alc-vein (`default-features = false`) をそのまま使い、
この Worker が持つのは Hyperdrive 越しの repo 実装 (`src/repo.rs`) と workers-rs への載せ方 (`src/lib.rs`) だけ。

- **到達経路は auth-worker からの Service Binding だけ。** JWT を検証せず `X-Tenant-ID` を
  信頼するので、`workers_dev` / `preview_urls` を false にし、`route` / `routes` を持たない。
  `scripts/check-exposure.sh` が CI で毎回検査する。
- **RLS は repo のメソッド 1 回 = 1 トランザクション。** `BEGIN` の中で
  `set_config('app.current_tenant_id', $1, true)` を打つ (Hyperdrive はトランザクション単位で
  コネクションを使い回すので、session スコープの `set_current_tenant` は使えない)。詳細は `src/repo.rs`。
- monolith の workspace から exclude した独立 workspace (自前の Cargo.lock、Bazel に入れない)。

## ビルド

```bash
cargo install worker-build@0.8.6 --locked
worker-build --release
```

## ローカルのテナント漏れテスト / 測定

接続文字列は repo に書かず環境変数で渡す。worker 側 (`APP_DB_URL`) は **RLS が効く
`alc_api_app` で** 繋ぐ (superuser / BYPASSRLS なら run-local.sh が止まる)。Hyperdrive と同じく
トランザクション単位でコネクションを使い回させるなら、PgBouncer の transaction mode を前に置く。

```bash
PG_ADMIN_URL=... APP_DB_URL=... bash tests/run-local.sh     # tenant-leak.mjs (A/B 各 200 本、同時 20)
PG_ADMIN_URL=... APP_DB_URL=... bash tests/bench-local.sh   # /vein/identify の CPU 時間 (100/500/1000/5000 件)
```

DB は `scripts/init_local_db.sql` + `migrations/` を流したもの。`alc_api_app` は `NOLOGIN` で
作られるので、ローカルでは `ALTER ROLE alc_api_app LOGIN PASSWORD '...'` が要る。
