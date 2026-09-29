# alc-vein Worker

指静脈 (vein) の 4 本の口 (crates/alc-vein) を workers-rs + tokio-postgres で提供する Worker。
rust-alc-api を Cloudflare Workers へ段階移行する最初の 1 本 (Refs #680 / #683 / #691)。
口・照合・trait・SQL (`repo::sql`) は alc-vein (`default-features = false`) をそのまま使い、
この Worker が持つのは repo 実装 (`src/repo.rs`)・DB への経路 (`src/db.rs`)・staging の DB を抱える
Durable Object (`src/vein_db.rs`) と workers-rs への載せ方
(`src/lib.rs`) だけ。

## DB への経路 (`src/db.rs` の 1 か所で出し分ける)

どの経路も間に **transaction mode のプーラー**が入る (Cloudflare の DB 接続プールは使わない — staging で同じ経路を
通せないため。#691)。

| env | 経路 | プーラー |
|---|---|---|
| staging (`--env staging`) | Worker → Durable Object `VeinDb` へ TCP (`Stub::connect`) → Container の 6432 へ中継 | Container 内の PgBouncer (`container/`) |
| 本番 (トップレベル) | secret `DATABASE_URL` の host:port へ Worker の TCP (STARTTLS)。未設定なら 503 | Supabase のプーラー (6543) |
| ローカル | `DATABASE_URL` (`sslmode=disable`) + var `ALLOW_INSECURE_DB=1` のときだけ手元の PgBouncer へ平文 | 手元の PgBouncer |

本番の接続文字列の設定とデプロイは別タスク。`ALLOW_INSECURE_DB` はローカル専用 (`wrangler dev --var` /
`.dev.vars`) で、無ければ `sslmode=disable` でも TLS を強制する。wrangler.toml の vars に書かないことを
`scripts/check-exposure.sh` が検査する。

## 到達面

- **本番の到達経路は auth-worker からの Service Binding だけ。** JWT を検証せず `X-Tenant-ID` を
  信頼するので、トップレベルは `workers_dev` / `preview_urls` を false にし、`route` / `routes` を持たない。
- **staging はテストから叩くため `workers_dev = true`**
  (URL は `wrangler deploy --env staging` の出力を見る)。**この workers.dev は Cloudflare Access で保護する前提**
  (アプリ名・ポリシー・service token は親タスク / 運用側が Access に設定する。repo には持たない)。
  Access を通らないリクエストは Worker に届かず、Access がログインへの 302 か 403 を返す。
  テストは service token を `CF-Access-Client-Id` / `CF-Access-Client-Secret` で付ける。
- `workers_dev = true` を許すのは `env.staging` だけ。`scripts/check-exposure.sh` が CI で毎回これを検査し、
  `scripts/check-exposure-test.sh` が陰性対照 (wrangler.toml を崩すと exit 1) を回す。
- **`VeinDb` の `connect` ハンドラは Worker の `Stub::connect` (DO binding `VEIN_DB`) からしか呼べない。**
  DO も Container も外部から直接届く口は無い (Container の 6432 は `getTcpPort()` 経由だけ)。

## RLS

**repo のメソッド 1 回 = 1 トランザクション。** `BEGIN` の中で
`set_config('app.current_tenant_id', $1, true)` を打つ (プーラーはトランザクション単位で
コネクションを使い回すので、session スコープの `set_current_tenant` は使えない)。
**repo の DB 操作はすべて `in_tenant_tx` を通し、`Row` / `Statement` をトランザクションの外へ出さない**
(戻り値は印 `TxOutput` の付いた owned 型に限るので、`Row` を返すとコンパイルが通らない)。
`Row` は prepared statement を握っていて、COMMIT 後に drop すると Close がトランザクションの外に出て
別のサーバー接続へ回り、`prepared statement "s1" already exists` (42P05) になる (staging で実測)。
詳細は `src/repo.rs`。

## staging の DB (Cloudflare Containers)

`container/`: postgres 16 + PgBouncer (`pool_mode = transaction`、サーバー側 4 本、
`max_prepared_statements = 0`)。**ディスクは揮発**なので、起動のたびに空の DB から
`scripts/init_local_db.sql` → `migrations/` → `scripts/local_app_grants.sql` (本番の GRANT の写し) →
テナント漏れテストの種 (`tests/seed.sql`) を流し、最後に PgBouncer を起動する。

- `VeinDb` は Container が止まっていれば起動し、PgBouncer が応答するまで StartupMessage を送り直して待つ
  (`getTcpPort().connect()` は listen 前でも「開く」ので、応答が返ったかで判定する。上限 90 秒)
- **常時起動にしない**: 最後の接続が閉じて 10 分で alarm が Container を止める。次のリクエストは
  cold start (Container 起動 + migration) になる
- Cloudflare Containers には `/var/run/postgresql` も `/dev/shm` も無い (unix socket は `/tmp` に置く)
- 既定だと北米に置かれて往復ごとに太平洋を越えるので `constraints.regions = ["APAC"]`

```bash
wrangler deploy --env staging    # docker で container/ を build して push する
```

## ビルド

```bash
cargo install worker-build@0.8.6 --locked
worker-build --release
```

## テナント漏れテスト / 測定

`tests/tenant-leak.mjs` は 2 テナント (A / B) のリクエストを並列に交互に投げ、全レスポンスが
自テナントの件数・ID と完全一致することを数える。worker は **RLS が効く `alc_api_app` で**繋ぐこと
(superuser だと RLS を素通りして全部通ってしまう)。

staging (Container の PgBouncer 経由。種は Container の起動時に入っている):

```bash
VEIN_URL="<wrangler deploy --env staging が出す URL>" \
  CF_ACCESS_CLIENT_ID=... CF_ACCESS_CLIENT_SECRET=... \
  TENANT_A=0a000000-0000-4000-8000-00000000000a TENANT_B=0b000000-0000-4000-8000-00000000000b \
  N_A=7 N_B=13 node tests/tenant-leak.mjs     # Access のヘッダー無し・値違いが 302/403 になることも数える
```

ローカル (接続文字列は repo に書かず環境変数で渡す。`container/` の image をそのまま DB に使える):

```bash
PG_ADMIN_URL=... APP_DB_URL=... bash tests/run-local.sh     # tenant-leak.mjs (A/B 各 200 本、同時 20)
PG_ADMIN_URL=... APP_DB_URL=... bash tests/bench-local.sh   # /vein/identify の CPU 時間 (100/500/1000/5000 件)
```

DB は `scripts/init_local_db.sql` + `migrations/` を流したもの。`alc_api_app` は `NOLOGIN` で
作られるので、ローカルでは `ALTER ROLE alc_api_app LOGIN PASSWORD '...'` が要る。
