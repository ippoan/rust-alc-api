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
| staging (`--env staging`、**一時**、#695) | Worker → Workers VPC の binding `VEIN_DB_VPC` (VPC Service 型、TCP) → 既存の Cloudflare Tunnel → 運用者の Linux 機の docker (`127.0.0.1:6432` にだけ bind) | 手元で動かす `container/` の image 内の PgBouncer |
| staging (fallback、#691) | Worker → Durable Object `VeinDb` へ TCP (`Stub::connect`) → Container の 6432 へ中継。`VEIN_DB_VPC` を外して deploy するとこちらに戻る | Container 内の PgBouncer (`container/`) |
| 本番 (トップレベル) | secret `DATABASE_URL` の host:port へ Worker の TCP (STARTTLS)。未設定なら 503 | Supabase のプーラー (6543) |
| ローカル | `DATABASE_URL` (`sslmode=disable`) + var `ALLOW_INSECURE_DB=1` のときだけ手元の PgBouncer へ平文 | 手元の PgBouncer |

`src/db.rs` は binding (`VEIN_DB_VPC` → `VEIN_DB`) を secret `DATABASE_URL` より先に見る。どちらの binding も
平文 (trust 認証) なので本番 (トップレベル) に置かないことを `scripts/check-exposure.sh` が検査する。
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
- **staging の手元 DB (#695) に届く口は VPC binding `VEIN_DB_VPC` だけ。** ホストの 6432 は `127.0.0.1` にだけ
  bind し、Tunnel の ingress / public hostname / CIDR route には出さない。VPC Service 型は宛先を 1 host:port に
  固定するので、Worker からホストの他のポートへは届かない (VPC Networks 型は Tunnel の先の網全体に届くので使わない)。

## RLS

**repo のメソッド 1 回 = 1 トランザクション。** `BEGIN` の中で
`set_config('app.current_tenant_id', $1, true)` を打つ (プーラーはトランザクション単位で
コネクションを使い回すので、session スコープの `set_current_tenant` は使えない)。
**repo の DB 操作はすべて `in_tenant_tx` を通し、`Row` / `Statement` をトランザクションの外へ出さない**
(戻り値は印 `TxOutput` の付いた owned 型に限るので、`Row` を返すとコンパイルが通らない)。
`Row` は prepared statement を握っていて、COMMIT 後に drop すると Close がトランザクションの外に出て
別のサーバー接続へ回り、`prepared statement "s1" already exists` (42P05) になる (staging で実測)。
詳細は `src/repo.rs`。

## staging の DB (手元の docker + Workers VPC、一時、#695)

Container 経路は止まった後の cold start (約 3.5 秒)・置き場所が `APAC` までしか絞れない (往復 60〜140ms)・
起動のたびの migration が重いので、**一時的に** staging の DB を運用者の Linux 機の docker に置く。
Hyperdrive は使わない (#680)。

- DB は `container/` の image をそのまま使う (build context は repo のルート)。systemd --user の unit が
  `docker rm -f` → `docker run --rm -p 127.0.0.1:6432:6432` で毎回作り直す (start.sh は既存の PGDATA があると
  落ちるので restart policy・volume は使わない)。**ポートは必ず `127.0.0.1` に bind する** (PgBouncer は
  trust 認証で、届けば全テナントを読める)
- Worker からの到達口は **VPC Service (TCP 型、宛先 = そのホストの `127.0.0.1:6432` だけ)** の binding だけ。
  既存の Tunnel の ingress・public hostname・CIDR route には出さない (VPC Service は ingress 不要)。
  宛先は Service 側で固定なので、`connect()` に渡すアドレスは名目だけ
- wrangler.toml に書くのは VPC Service の ID だけ。Tunnel の ID・ホスト名・account ID・IP は repo に書かない
  (Service は `wrangler vpc service create <名前> --type tcp --tunnel-id … --ipv4 127.0.0.1 --tcp-port 6432` で作る)
- **wrangler は 4.78.0 以上**を使う (TCP 型の VPC Service は 4.78.0 から。手元の 4.58 には `--type tcp` が無い)。
  CI は wrangler を使わない (worker-build だけ) ので CI 側の版は関係ない

```bash
npx wrangler@4.144.0 deploy --env staging   # 4.78.0 以上なら可
```
- **Worker の実行場所は `[env.staging.placement] region` で DB の近く (Tunnel の繋がる関西) に固定する。**
  指定しないとリクエストが入った colo (実測で SIN) で動き、DB の往復ごとに海を越えて一覧の p50 が 944ms になる
  (固定後は入口が SIN でも `cf-placement: remote-KIX` で動き、DB 部分は connect 37ms + db 120ms)
- DB は止まらないので cold start は無い。ディスクは揮発のまま (unit の再起動で空の DB から作り直す)

**終わりの条件** — 次のどれかが来たら、`[[env.staging.vpc_services]]`・`src/db.rs` の `connect_vpc`・VPC Service・
systemd の unit・コンテナ・image を消し、staging を Container 経路 (または本番と同じ経路) に戻す:

- 本番の DB 接続が決まり、staging をそれに揃えるとき
- Workers VPC が有料化されるとき
- この Linux 機を止めるとき

## staging の DB (Cloudflare Containers、fallback)

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

staging (VPC 経路でも Container 経路でも、種は DB の起動時に `container/start.sh` が入れている):

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
