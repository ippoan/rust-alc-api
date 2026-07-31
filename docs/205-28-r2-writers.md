# R2 (`ohishi-dtako`) の dtako CSV を書く経路の全洗い出し

Refs `ohishi-exp/rust-ichibanboshi#205` / #205-28。**調査のみ。コードは変更していない。**

対象: `DTAKO_R2_BUCKET=ohishi-dtako` の `{tenant_id}/unko/{unko_no}/*.csv`
(`KUDGIVT.csv` / `KUDGURI.csv` / `KUDGFRY.csv` / `KUDGSIR.csv` / `SOKUDODATA.csv` 等)。

目的: **指紋 (etag 相当) を `dtako_operations` に持つ設計 (案 C) が安全か**の判断材料。
判定は「**条件付きで進める**」— 詳細は末尾。

調査時点: 2026-07-31 / alc `origin/main` = `b7d4144` / nuxt-dtako-admin `99a83a7`。

---

## 1. 結論の骨子

`{tenant_id}/unko/**` に **書く**コードは、調べた範囲では **1 本しか無い** —
`crates/alc-dtako/src/dtako_upload.rs` の `split_csv_from_r2()`
([dtako_upload.rs:919](crates/alc-dtako/src/dtako_upload.rs:919) で key を組み、
[:947](crates/alc-dtako/src/dtako_upload.rs:947) で PUT)。
そしてこの関数は同じスコープで `dtako_operations` も更新している
(`update_has_kudgivt`、[:958](crates/alc-dtako/src/dtako_upload.rs:958))。

さらに重要な事実として、**`GET /api/dtako/events/etags` は既にこの DB 更新に依存している** —
DB 側の unko_no 列挙が `has_kudgivt = TRUE` で絞られており
([repo/dtako_y_time_export.rs:124](crates/alc-dtako/src/repo/dtako_y_time_export.rs:124))、
その `has_kudgivt` を立てるのは split だけ。つまり案 C は
「R2 を書いたら DB も更新する」という**既存の依存関係を 1 列ぶん太らせるだけ**で、
新種の依存を持ち込むわけではない。

危険は「経路が複数ある」ことではなく、**指紋の真実が R2 (ソース) から DB (コピー) に
移ることで、`R2 だけ書かれた / R2 は書けたが DB は書けなかった` が黙って通る**ことにある。
現行の LIST 方式はソースを直接見るのでこの穴が構造的に存在しない。
→ §5 の D1 / D2 と緩和策 M2 / M3 が案 C の必須条件。

---

## 2. 書き手の一覧

### W1. `POST /api/upload` → `process_zip` — **ZIP を書く。CSV は書かない**

| | |
|---|---|
| 書く先 | `{tenant_id}/uploads/{upload_id}/{filename}` (application/zip) — [dtako_upload.rs:124-132](crates/alc-dtako/src/dtako_upload.rs:124) |
| いつ | ZIP アップロードのたび (毎回 `upload_id` は新規 UUID) |
| 同じ key を上書きするか | しない (`upload_id` が毎回新規)。`internal/rerun` は同じ `upload_id` の同じ bytes を再 PUT するので内容は不変 |
| `dtako_operations` を更新するか | **する** — `delete_operation` + `insert_operation` で当該 `unko_no` の行を作り直す ([:194-230](crates/alc-dtako/src/dtako_upload.rs:194))。`daily_work_hours` も再計算 |
| `{tenant}/unko/**` を触るか | **触らない** |
| 人手で叩けるか | 叩ける (下記 (c)) |

呼び手は 3 つ:

- **(a) `ohishi-exp/dtako-scraper` (VPS 常駐)** — `src/scraper/upload.rs::upload_zip` が
  auth-worker `/device-data-proxy/api/upload` に device JWT で multipart POST。
  R2 には一切触らない (`Cargo.toml` に s3/aws 系依存なし、`reqwest` のみ)。
- **(b) `nuxt-dtako-admin` の `dtako-scraper-relay` DO (Cloudflare Worker)** —
  cron `0 16 * * *` UTC (= 01:00 JST) で **前日 1 日分**をスクレイプし
  (`workers/dtako-scraper-relay/src/cron.ts:185` `yesterdayJst(now)`、`start_date = end_date`)、
  `alc-internal-upload.ts` が `/alc-internal-proxy/api/upload` に shared-secret + `X-Tenant-ID` で POST。
  `SCRAPER_MODE=http` のときだけ動き、それ以外は VPS 側 cron (= (a)) が担当。
- **(c) 人手** — `nuxt-dtako-admin` の `app/pages/upload.vue` → `uploadZip()` (`app/utils/api.ts:138`)。
  管理画面から任意の ZIP を上げられる。同ページに `rerunUpload` / `splitCsv` もある。

### W2. `split_csv_from_r2` — **`{tenant}/unko/**` を書く唯一の経路**

| | |
|---|---|
| 書く先 | `{tenant_id}/unko/{unko_no}/{CSV名}` (text/csv、20 並列 PUT) — [dtako_upload.rs:919](crates/alc-dtako/src/dtako_upload.rs:919) / [:947](crates/alc-dtako/src/dtako_upload.rs:947) |
| いつ | ① upload 成功直後 (`try_split_csv`、[:77](crates/alc-dtako/src/dtako_upload.rs:77)) ② `internal/rerun` 成功直後 ([:1084](crates/alc-dtako/src/dtako_upload.rs:1084)) ③ `POST /api/split-csv/{upload_id}` ④ `POST /api/split-csv-all` (SSE、未 split の upload を 5 並列で総なめ) |
| 同じ `unko_no` を上書きしうるか | **しうる**。同じ ZIP の再 split は byte 同一だが、**同一 `unko_no` を含む ZIP が 2 本あると後勝ちで内容が変わる** (手動で日付範囲を重ねて再スクレイプした場合など) |
| `dtako_operations` を更新するか | **する (ただし `has_kudgivt` だけ)** — `UPDATE ... SET has_kudgivt = TRUE WHERE tenant_id = $1 AND unko_no IN (...)` を 100 件チャンクで ([repo/dtako_upload.rs:404](crates/alc-dtako/src/repo/dtako_upload.rs:404))。**指紋列を足すならここが定位置** |
| 人手で叩けるか | **叩ける** — 管理画面の「CSV 分割」(`splitCsv` / `splitCsvAllStream`、`app/utils/api.ts:512,518`) |

補足 (案 C に効く性質):

- `try_split_csv` は失敗しても `tracing::warn!` だけで upload 自体は成功扱い ([:868-872](crates/alc-dtako/src/dtako_upload.rs:868))。
  → split が走らなかった unko_no は `has_kudgivt = FALSE` のままで、**etags の対象から丸ごと外れる** (今日もそう)。
- ZIP → CSV は決定的 (`group_csv_by_unko_no` + ヘッダ付与)。同じ ZIP からは常に同じ bytes。
- 単発 PUT のみ (multipart 無し) なので ETag = content MD5 が成り立つ
  ([alc-core/src/storage.rs:9-18](crates/alc-core/src/storage.rs:9) のコメント通り)。

### W3. `archive` バイナリ (Cloud Run Job `rust-alc-api-archive`) — 同じバケットだが別 prefix

| | |
|---|---|
| 書く先 | `archive/alc_api/dtakologs/**` (`_manifest.json` / `schema_v1.json` / `{tenant}/{YYYY}/{MM}/{DD}.jsonl.gz`) と `archive/logi/**` — [src/archive/dtako.rs:23,38](src/archive/dtako.rs:23) |
| いつ | `archive dtako-archive` / `logi-dump` の手動・ジョブ実行 |
| `{tenant}/unko/**` を触るか | **触らない** (key 空間が完全に分離) |
| DB | `alc_api.dtakologs` の DELETE / UPSERT のみ。**`dtako_operations` には一切触れない** |

`DTAKO_R2_BUCKET` を読むのはこのバイナリ ([src/bin/archive.rs:84](src/bin/archive.rs:84)) と
`src/main.rs` の 2 箇所だけ。

### W4. 鍵を持った人手 (rclone / Cloudflare 側の直接操作) — **DB を一切触らない**

`yhonda-ohishi/claude-skills` の `supabase-r2/supabase-r2/SKILL.md` に、
`DTAKO_R2_ACCESS_KEY` / `DTAKO_R2_SECRET_KEY` から
`rclone` リモート `dtako:ohishi-dtako` を作る手順が載っている。
**文書化されている操作は read のみ** (`rclone ls` / `rclone copy R2→ローカル` / `rclone cat`) だが、
**同じ鍵で書ける**。secrets 側にも `ohishi-dtako-prod-api-202605` という R2 API token 名が
`ippoan/secrets-inventory` のテスト fixture に見える (token 自体の権限・配布先は未確認)。

Cloudflare ダッシュボードからの直接アップロード / 他所に配られた R2 API token /
R2 event notification や Queue consumer の有無は **connector が親セッションにしか無いため未確認**
→ §7 の [質問] 参照。

### W5. dev サンドボックスの HTTP proxy (本番には出ない想定)

`DTAKO_STORAGE_HTTP_PROXY` を設定すると `HttpProxyBackend` 経由で
ホスト側 `wrangler dev --local --port 8788` (R2 binding 付き) に write が飛ぶ
([src/main.rs:176-193](src/main.rs:176))。コメント通り `--local` なら
ローカルエミュレータ止まりで本番 R2 には出ない。`--remote` で起動した場合だけ
「dev の split が本番 R2 を書き、dev の DB を更新する」という形になりうる (要運用上の注意)。

### 読むだけ (書かない) の経路 — 参考

`dtako_csv_proxy` (`GET /operations/{unko_no}/csv/{type}`)、
`dtako_events` (`/events`、`/events/etags` の LIST)、`dtako_y_time_export`、
`recalculate` / `recalculate-driver` / `recalculate-drivers` (R2 から KUDGIVT を **download** するだけ)、
`dtako_logs`。**`dtako_storage` に対する `delete()` 呼び出しはコード上 1 件も無い**
(= R2 側の CSV は消えない)。

---

## 3. 探し方 (「無い」の根拠)

### `ippoan/rust-alc-api` (ローカル clone、`origin/main` = `b7d4144`)

- `StorageBackend` trait の write メソッドは `upload` と `delete` の 2 つだけ
  ([crates/alc-core/src/storage.rs:26-69](crates/alc-core/src/storage.rs:26))。
- `grep -rn "\.upload(\|\.delete(" --include=*.rs crates/ src/` を全件目視 →
  `state.dtako_storage` を使う write は **`dtako_upload.rs:130` (ZIP) と `:947` (CSV) の 2 件のみ**。
  他の `.upload(` は `state.storage` (alc-face-photos) / carins / notify / trouble の別バケット。
- `grep -rn "dtako_storage" --include=*.rs` → 非テストの呼び出し元 12 箇所を全部確認。
  write は上記 2 件、残りは `download` / `list` / `exists`。
- `grep -rn "unko/" --include=*.rs crates/ src/` → 7 箇所 (テスト除く)。
  write は `:919` のみ、他は read か doc コメント。
- `grep -rn "DTAKO_R2_BUCKET\|ohishi-dtako"` → `src/main.rs` / `src/bin/archive.rs` /
  `deploy.sh` / `archive.sh` / `cloudrun/render.sh` / `staging/cloudrun-staging.yaml` のみ。
- `find . -name "wrangler*"` → **0 件** (この repo に Worker は無い)。

### `ohishi-exp/nuxt-dtako-admin` (ローカル clone、`99a83a7`)

- `wrangler.toml` と `workers/*/wrangler.toml` の `[[r2_buckets]]` を全部読んだ →
  binding は `DTAKO_R2` = **`dtako-uploads`**、`PROFIT_R2` = `dtako-ichiban-verify(-staging)` のみ。
  top-level / `env.staging` / `env.preview` / `env.dev` すべて同じ。
- `grep -rn "ohishi-dtako" .` (node_modules・.git 除く) → **0 件**。
- R2 write (`.put(` / `.delete(`) の全件確認 → `vehicle-settings/<vehicle_cd>/...`、
  `y-time` テンプレ、`poi/`、`etc`/`etc-errors` prefix、profit スナップショット。
  **`{tenant}/unko/` 形の key を組む箇所は 0 件** (`grep -rn "unko" server/ workers/*/src` は
  クエリパラメータ名と勤怠ロジックのコメントばかり)。
- alc への書き込み系呼び出しは `POST /api/upload`、`POST /api/split-csv/{id}`、
  `POST /api/split-csv-all`、`POST /api/internal/rerun/{id}` — **すべて W1 / W2 に合流**。

### `ohishi-exp/dtako-scraper` (ローカル clone 無し → `gh api` で参照)

- `git/trees/HEAD?recursive=1` でファイル一覧を取得 (34 エントリ、全部目視)。
- `src/scraper/upload.rs` 全文確認 → auth-worker `/device-data-proxy/api/upload` への multipart POST のみ。
- `Cargo.toml` に `s3` / `aws` / `rust-s3` / `r2` 系依存 **なし** (`reqwest` の multipart だけ)。

### GitHub 横断 (`gh search code`)

| 検索語 | 範囲 | 結果 |
|---|---|---|
| `ohishi-dtako` | 全アクセス可能 repo | `ippoan/rust-alc-api` (env/設定 6 ファイル)、`secrets-inventory(-gcp)` (token 名のテスト fixture)、`claude-skills` の `backend-check.md` / `supabase-r2/SKILL.md` のみ。**bind / write するコードは 0 件** |
| `DTAKO_R2_BUCKET` | 全アクセス可能 repo | `ippoan/rust-alc-api` のみ (7 ファイル) |
| `DTAKO_R2_ACCESS_KEY` | 全アクセス可能 repo | `rust-alc-api` + `yhonda-ohishi/claude-skills` (rclone 手順) のみ |
| `r2.cloudflarestorage.com` | `--owner ippoan / ohishi-exp / yhonda-ohishi / ohishi-yhonda-org / yhonda-ohishi-pub-dev / yhonda-ohishi-alc` | `alc-storage/src/r2.rs`、`yhonda-ohishi-pub-dev/rust-logi:src/storage/r2.rs` (別プロダクト・別バケット)、`claude-skills` |
| `r2_buckets` | 同上 5 owner | 18 ファイル。**`ohishi-dtako` を bind しているものは 0 件** |
| `KUDGIVT` | 全アクセス可能 repo | R2 key を組んでいるのは `rust-alc-api` のみ。他は DB スキーマ / CSV パーサ / 旧 logi 系 |

### その他

- `ohishi-exp/smb-watch` — `deploy/smb-watch.env.example` と `src/main.rs` を確認。
  upload 先は `https://carins.ippoan.org/api/device-upload` (車検証)。dtako とは無関係。
- `ohishi-exp/rust-ichibanboshi` (ローカル clone) — `ohishi-dtako` / `DTAKO_R2` / `unko/` の
  grep は `src/kintai_push.rs:9` のコメント 1 行のみ。**R2 を直接触るコードは無い** (alc API 経由の read のみ)。

### 探していない場所 (明示)

- **Cloudflare 側の設定そのもの** — ダッシュボードからの手動アップロード、
  `ohishi-dtako` に対して発行済みの R2 API token の一覧と配布先、
  bucket の event notification / Queue consumer、他アカウントからの binding。
  connector が親セッションにしかないため未確認。
- **`gh search code` のインデックスに載らないもの** — default branch 以外、
  および private repo で index 対象外のもの (`ohishi-exp/net780-wasm`、
  `ohishi-exp/dtako_vid_wasm` は tree を開いていない)。
- **GitHub 外** — VPS (`ohishi-data` 等) に置かれた cron スクリプトや個人 PC の手順書。
  `ohishi-exp/dtako-scraper` と `smb-watch` の repo 内 deploy 定義しか見ていない。
- `ippoan/auth-worker` の proxy 実装 (`/device-data-proxy`, `/alc-internal-proxy`) は
  「誰が alc を叩けるか」の話で R2 直書きではないため、呼び出し規約の確認に留めた。

---

## 4. 経路まとめ (表)

| # | 経路 | いつ書くか | 同じ `unko_no` を上書き | `dtako_operations` を更新 | 人手で叩けるか |
|---|---|---|---|---|---|
| W1 | `POST /api/upload` → ZIP を `{tenant}/uploads/**` | upload のたび | しない (毎回新 `upload_id`) | **する** (delete+insert で行を作り直す) | 管理画面 `upload.vue` |
| W2 | `split_csv_from_r2` → CSV を **`{tenant}/unko/**`** | upload 直後 / rerun 直後 / split-csv / split-csv-all | **しうる** (同一 `unko_no` を含む別 ZIP は後勝ち) | **する** (`has_kudgivt` のみ) | 管理画面「CSV 分割」 |
| W3 | `archive` バイナリ → `archive/**` | 手動 / Cloud Run Job | 対象外 (別 prefix) | 触らない (`dtakologs` のみ) | CLI / Job 実行 |
| W4 | rclone / CF 直操作 (鍵持ち) | 不定 | しうる | **しない** | 完全に人手 |
| W5 | dev の `wrangler dev` proxy | dev 実行時 | `--remote` なら理論上しうる | dev の DB のみ | 開発者 |

---

## 5. 案 C の risk (具体)

### D1 (必修) — split の PUT 失敗が完全に握り潰されている

```rust
let results = futures::future::join_all(futures).await;
csv_count += results.len();   // Err を一切見ていない
```
[dtako_upload.rs:950-951](crates/alc-dtako/src/dtako_upload.rs:950)。
`Result` を捨てているので、PUT が失敗しても log にすら出ず `csv_count` は成功数として増える。

案 C で「書いたつもりの内容」から指紋を作ると:
R2 は v1 のまま → DB は hash(v2) → gate は「変わった」と見て v1 から再計算し hash(v2) で封 →
**あとで split をやり直して本当に v2 が入っても DB の指紋は hash(v2) のままなので「変わっていない」**
→ v1 由来の結果が最新として固定される。#205 が今日潰してきた failure mode そのもの。

### D2 (必修) — 「R2 は書けたが DB は書けなかった」が黙って通る

現行 LIST 方式は真実 (R2) を直接読むのでこの穴が無い。案 C はコピーを読むので、
PUT 成功 → DB UPDATE 失敗 (transient error / Cloud Run instance 停止) で
**DB が古い指紋を保持したまま = 「変わっていない」と誤判定**する。

**安全側に倒す形**: split の頭で対象 `unko_no` の指紋列を **NULL に落としてから** PUT し、
成功したものだけ最後に値を書く (invalidate-before-write)。
途中で落ちれば NULL (= 不明) が残り、lazy fallback (LIST) が正しく埋める。
逆順 (PUT → UPDATE) にすると失敗が「古い値の据え置き」= 静かな誤判定になる。

### D3 — 同一 `unko_no` の並行 split

`split_csv_all_core` は upload を 5 並列、各 split はさらに 20 並列 PUT
([:1632](crates/alc-dtako/src/dtako_upload.rs:1632) / [:940](crates/alc-dtako/src/dtako_upload.rs:940))。
同じ `unko_no` を含む ZIP が 2 本あると「R2 は A の内容 / DB は B の指紋」の交差がありうる。
D2 の invalidate 方式で大半は吸収できるが完全ではない → M5 (drift 検知) で拾う。

### D4 — 鍵持ちの手作業 (W4) / CF 側の直接操作

DB を一切触らないので、起きたら黙って古い結果に封をする。頻度は低いが構造的に塞げない。
**M5 (定期 drift 検知) だけがこれを拾える。**

### D5 — `crew_role` で 1 `unko_no` = 最大 2 行

`UNIQUE(tenant_id, unko_no, crew_role)` ([migrations/054_dtako_tables.sql:88](migrations/054_dtako_tables.sql:88))。
R2 の CSV は `unko_no` 単位なので、指紋列は `has_kudgivt` と同じく
`WHERE tenant_id = $1 AND unko_no IN (...)` で**全 `crew_role` 行に同じ値**を入れること。
読み側は `DISTINCT ON (driver_id, unko_no)` で 1 行に畳むので問題ないが、
片方だけ更新する書き方をすると壊れる。

### 危険では **ない**と確認できたもの

- W1 (ZIP) は `{tenant}/unko/**` を書かない。しかも `delete_operation` + `insert_operation` で
  行を作り直すので、**再取り込みのたびに指紋列は自然に NULL に戻る** (= 再計算が走る、安全側)。
- W3 (archive) は key 空間が完全に別で `dtako_operations` に触れない。
- R2 側の CSV を `delete` するコードは存在しない。
- nuxt-dtako-admin / dtako-scraper / rust-ichibanboshi は `{tenant}/unko/**` を書かない
  (前者 2 つの書き込みは必ず alc の W1 / W2 を通る)。

---

## 6. 判定と推奨

### 判定: **条件付きで進める**

`{tenant}/unko/**` の書き手は実質 **W2 (split) 一本**で、しかもその関数は既に
`dtako_operations` を更新している。さらに etags の DB 側は既に `has_kudgivt` (= split が書く列)
に依存しているので、**案 C は依存の種類を増やさない**。ここは想定より綺麗だった。

ただし「経路が 1 本」は「安全」を意味しない。**指紋の真実を R2 から DB のコピーに移す**
こと自体が新しい失敗モード (D1 / D2) を作る。以下を C の実装に**含める前提で**進めるべき。

### 必須 (これが無い C には進めない)

1. **M2 — invalidate-before-write**: split の頭で対象 `unko_no` の指紋列を NULL に落とし、
   PUT が成功したものだけ最後に値を入れる。落ちたら NULL が残る = 安全側。
2. **M3 — PUT の失敗を握り潰さない** ([dtako_upload.rs:950](crates/alc-dtako/src/dtako_upload.rs:950) の修正)。
   失敗した `unko_no` は指紋を NULL のままにし、件数と失敗を log に出す。
   *これは案 C とは独立に今ある bug でもある。*
3. **M4 — lazy fallback**: 指紋が NULL の `unko_no` だけ LIST で埋める。
   過去分 backfill も兼ねる (親の指示どおり一括ジョブは不要)。
4. **M5 — drift 検知**: 既存の LIST 実装をそのまま夜間/週次で回し、DB 指紋と突き合わせて
   ズレたら NULL に落として警告。**W4 (鍵持ちの手作業) を拾える唯一の手段**なので、
   C を採るなら保険としてこれが本命。17s のコストは gate の外なので許容できる。

### 併せて (運用)

5. **M1 — 書き手を 1 本に保つ規範**: 「`{tenant_id}/unko/**` に書いてよいのは
   `split_csv_from_r2` だけ。他所から書かない」を CLAUDE.md / `rust-alc-api-map` skill に明記する。
   今それが守られているのは偶然ではなく設計だが、明文化されていない。
6. **M6 — 移行**: 初回は全量再計算が 1 回走る (親が許容と明言済み)。

### 参考: 進めない判断も合理的

#205-27 で 17s → 2〜4s になるなら、「月ゲートで 1 回」の用途としてはそれで足りる可能性がある。
案 C は 0.1s 未満まで行くが、M2 / M3 / M4 の実装と **M5 の恒久的な運用**が付いてくる。
急がないなら #205-27 で止めて、C は「LIST が再びボトルネックになったら」でも遅くない。
これは速度要件を握っている親の判断。

---

## 追記: etags は R2 に無い運行をどう扱うか (142 行差との関係)

親からの追加依頼 (#205 の「142 行差」が D1 で説明できるか) への回答。**結論 3 行**:

1. **突合は DB 基準の左外部結合 = (あ)**。R2 に無い運行は `etag: null` の item として残り、脱落しない
   ([dtako_events.rs:513-520](crates/alc-dtako/src/dtako_events.rs:513))。「LIST に在るものだけ返す (い)」ではない。
2. **D1 (PUT 握り潰し) は 142 行差を説明できない**。`has_kudgivt` は ZIP のパース結果から立てており
   PUT の成否を見ないので、**PUT が全滅しても `has_kudgivt = TRUE` になる** → item は残り `etag: null` →
   #205-21 の `no_etag > 0` が鳴る。鳴っていない以上この経路ではない。
3. **ただし別の「静かに減る」経路が在る**: `has_kudgivt = FALSE` の運行は **DB 列挙の時点で消える**。
   items にも `/api/dtako/events` のデータにも現れず、**warning も鳴らない**。
   しかも `has_kudgivt` は**再アップロードのたびに FALSE にリセットされる**ので、
   「前は見えていた運行が、再アップロード + split 失敗で消える」が成立する。**142 行差の症状と一致**。

### 1. 突合の向き — DB 基準 (あ)

```rust
// 4. 突き合わせ: DB にある unko_no だけを、R2 の etag (無ければ null) と組にして返す。
let items = db_unko_nos.into_iter()
    .map(|unko_no| { let etag = r2_etags.get(&unko_no).cloned(); DtakoEventsEtagItem { unko_no, etag } })
    .collect();
```
[crates/alc-dtako/src/dtako_events.rs:513-520](crates/alc-dtako/src/dtako_events.rs:513)。
R2 の LIST 結果 (`r2_etags`) は `HashMap` の**引き当て先**にしかならず、
item の集合を決めるのは `db_unko_nos` 側。したがって
**「DB に `has_kudgivt=TRUE` の行があるのに R2 にオブジェクトが無い」運行は `etag: null` で必ず現れます**。

なお `warnings` はこの endpoint では**常に空**です
([dtako_events.rs:528](crates/alc-dtako/src/dtako_events.rs:528) が `Vec::new()` 固定)。
異常の通知経路は `etag: null` の 1 本だけ、という前提で消費側を作る必要があります。

### 2. PUT 全滅でも `has_kudgivt` は立つ (= D1 は検知される)

`kudgivt_unko_nos` は **アップロード前の準備ループ**で ZIP のパース結果から積まれます
([dtako_upload.rs:931-933](crates/alc-dtako/src/dtako_upload.rs:931))。
その後の PUT ループは `Result` を捨てており ([:950-951](crates/alc-dtako/src/dtako_upload.rs:950))、
`update_has_kudgivt` は**アップロード結果を一切参照せず**無条件に呼ばれます
([:955-965](crates/alc-dtako/src/dtako_upload.rs:955))。

→ **PUT が 1 件も成功しなくても `has_kudgivt = TRUE` になる。**
→ その運行は etags の items に `etag: null` で載る → #205-21 が拾う。
→ **D1 単独では「静かに減る」を作れません。** (D1 は案 C を採る場合のリスクとしては依然として必修)

### 3. 静かに減る本命 — `has_kudgivt = FALSE` で列挙から落ちる

読み取り側の 3 クエリすべてが `has_kudgivt = TRUE` で絞っています
([repo/dtako_y_time_export.rs:61](crates/alc-dtako/src/repo/dtako_y_time_export.rs:61) /
[:90](crates/alc-dtako/src/repo/dtako_y_time_export.rs:90) /
[:124](crates/alc-dtako/src/repo/dtako_y_time_export.rs:124))。
これは etags だけでなく **`GET /api/dtako/events` (データ本体) も同じ repo を使う**ため、
`has_kudgivt = FALSE` の運行は **etags の items からも events の行からも同時に消えます**。
items に現れないので `no_etag` では**原理的に数えられません**。

そして `has_kudgivt` は片道の TRUE ではなく、**再アップロードのたびに FALSE に戻ります**:

- `process_zip` は `delete_operation` + `insert_operation` で行を作り直す
  ([dtako_upload.rs:194-230](crates/alc-dtako/src/dtako_upload.rs:194))
- `insert_operation` の列リストに `has_kudgivt` は**含まれていない**
  ([repo/dtako_upload.rs:343-359](crates/alc-dtako/src/repo/dtako_upload.rs:343)) →
  `DEFAULT FALSE` ([migrations/054_dtako_tables.sql:76](migrations/054_dtako_tables.sql:76)) に戻る
- `has_kudgivt` を TRUE にするのは `update_has_kudgivt` の 1 箇所だけ
  ([repo/dtako_upload.rs:405](crates/alc-dtako/src/repo/dtako_upload.rs:405))

したがって次のシナリオが成立します:

> 1. ある運行は既に split 済みで `has_kudgivt = TRUE`、R2 にも CSV がある (正常に見えている)
> 2. 同じ `unko_no` を含む ZIP が再アップロードされる (cron の再走 / 手動の範囲重ね / rerun)
> 3. `process_zip` が成功 → **行が作り直され `has_kudgivt = FALSE` に戻る**
> 4. その直後の `try_split_csv` が失敗する ([:868-872](crates/alc-dtako/src/dtako_upload.rs:868) で
>    `tracing::warn!` のみ、**upload API は 200 completed を返す**)
> 5. → `has_kudgivt` は FALSE のまま。**R2 には CSV が残っているのに、DB 側の列挙から消える**
> 6. → etags の items にも `/events` の行にも出ない。`no_etag` は 0 のまま。**warning は 1 つも鳴らない**

**症状の一致**: 入力が静かに減る / `no_etag` が鳴らない / 欠けが月の内側なら末尾 gap も動かない /
DB の状態が変わらないので**畳み直しても数字が動かない**。#205 の 142 行差の記述と全部合います。

失敗しうる箇所は PUT だけではありません。`split_csv_from_r2` の前半
(ZIP の `download` / `extract_zip` / `decode_shift_jis`) のどこで落ちても同じ結果になりますし、
`update_has_kudgivt` 自体が失敗しても `tracing::error!` を出すだけで
**`split_csv_from_r2` は `Ok(())` を返します** ([:956-965](crates/alc-dtako/src/dtako_upload.rs:956))。
この最後のケースは文字通り「**R2 は書けたが DB を更新しなかった**」であり、
**現行の LIST 方式でも救えません** (突合が DB 基準なので、DB に居ない運行は最初から見えない)。

### 4. 確認方法 (DB 1 クエリ、コード変更不要)

```sql
SET search_path TO alc_api;
SELECT set_config('app.current_tenant_id', '<tenant_id>', false);
SELECT has_kudgivt, count(*)
  FROM alc_api.dtako_operations
 WHERE tenant_id = '<tenant_id>'
   AND reading_date BETWEEN '<month_start>' AND '<month_end>'
 GROUP BY has_kudgivt;
```
`has_kudgivt = FALSE` が非ゼロなら本命。その `unko_no` について R2 に
`{tenant_id}/unko/{unko_no}/KUDGIVT.csv` が在るかを見れば、
「split が走らなかった」のか「そもそも KUDGIVT が ZIP に無い運行」なのかが切り分けられます。
**前者なら `POST /api/split-csv-all` を叩き直すだけで復活します** (R2 の ZIP は残っているため)。

### 5. この追記から出る示唆

- **`has_kudgivt` は「gate に載せるか」を決める事実上のスイッチなのに、落ちても誰も気づけない。**
  案 C の指紋列を足すかどうかとは別に、**`has_kudgivt = FALSE` の件数を可観測にする**べき
  (etags の `warnings` に「期間内で `has_kudgivt=FALSE` の運行が N 件」を載せるのが最小の手)。
- `try_split_csv` の握り潰し ([:868](crates/alc-dtako/src/dtako_upload.rs:868)) と
  `update_has_kudgivt` の握り潰し ([:961](crates/alc-dtako/src/dtako_upload.rs:961)) は、
  D1 (PUT の握り潰し) と**同じ性質の 3 件目・2 件目**です。まとめて直すのが筋。
- 案 C の評価は変わりません (**条件付きで進める**)。ただし
  「gate の入力集合が DB のフラグ 1 本に依存していて、そのフラグが静かに落ちる」ことが分かったので、
  **drift 検知 (M5) の対象は指紋列だけでなく `has_kudgivt` も含めるべき**です。

---

## 追記 2: `has_kudgivt = FALSE` は本番に残りうるか (機序の可否)

**この節は「機序が成立するか」だけを扱います。142 行差の原因だと結論づけるものではありません** —
突き合わせデータが無いため断定できません。

**結論 3 行**:

1. **残りうる。** 作られ方は `INSERT` の `DEFAULT FALSE` で、TRUE にするのは split の
   `update_has_kudgivt` **1 箇所だけ**。しかも**再アップロードのたびに FALSE に戻る**ので
   「一度 TRUE になれば安全」ではありません。
2. **`POST /api/split-csv-all` を回しても必ず TRUE になるとは限りません。**
   候補列挙が「どの upload が未 split か」を特定しておらず
   ([repo/dtako_upload.rs:194-210](crates/alc-dtako/src/repo/dtako_upload.rs:194))、
   さらに **filename の重複排除**があるため
   ([dtako_upload.rs:1618-1622](crates/alc-dtako/src/dtako_upload.rs:1618))、
   cron が毎日**固定名 `csvdata.zip`** で上げている以上 **同名 upload は 1 本しか split されません**。
3. **突合キーの作り方が 2 系統あり、ずれると「R2 には CSV があるのに DB は FALSE」**になります。
   しかも `UPDATE` の `rows_affected` を見ていないので、**0 行でも成功ログが出ます**
   ([repo/dtako_upload.rs:412](crates/alc-dtako/src/repo/dtako_upload.rs:412) /
   [dtako_upload.rs:963](crates/alc-dtako/src/dtako_upload.rs:963))。

### Q1-a. `has_kudgivt = FALSE` はどう作られるか

- **作る**: `insert_operation` の列リストに `has_kudgivt` が無い
  ([repo/dtako_upload.rs:343-359](crates/alc-dtako/src/repo/dtako_upload.rs:343)) →
  `DEFAULT FALSE` ([migrations/054_dtako_tables.sql:76](migrations/054_dtako_tables.sql:76))。
  `process_zip` は毎回 `delete_operation` + `insert_operation` で行を作り直すので
  ([dtako_upload.rs:194-230](crates/alc-dtako/src/dtako_upload.rs:194))、
  **再アップロードのたびに TRUE → FALSE に戻ります**。
- **TRUE にする**: `update_has_kudgivt` の 1 箇所だけ
  ([repo/dtako_upload.rs:405](crates/alc-dtako/src/repo/dtako_upload.rs:405))。
  repo 全体で他に `has_kudgivt` を書く SQL はありません。

**FALSE のまま残る運行の類型** (機序として成立するもの):

| # | 類型 | 何が起きるか |
|---|---|---|
| (a) | split が走らなかった / 前半 (`download` / `extract_zip` / `decode_shift_jis`) で落ちた upload | その ZIP 由来の**全運行**が FALSE。`try_split_csv` は `warn` のみで upload API は 200 ([:868-872](crates/alc-dtako/src/dtako_upload.rs:868)) |
| (b) | `update_has_kudgivt` 自体が失敗 | 同上。`tracing::error!` を出すだけで `split_csv_from_r2` は `Ok(())` を返す ([:956-965](crates/alc-dtako/src/dtako_upload.rs:956))。**R2 には CSV がある** |
| (c) | 突合キーがずれた (下記 Q2) | `UPDATE` が 0 行。**成功ログが出るので気づけない**。R2 には CSV がある |
| (d) | その ZIP の KUDGIVT.csv に 1 行も出てこない運行 | KUDGURI に居てイベントが 0 件の運行。**別の ZIP に KUDGIVT 行が現れない限り永久に FALSE** |
| (e) | 上記のいずれかの後で再アップロードされた運行 | 一度 TRUE でも FALSE に戻り、再 split が失敗すればそのまま |

なお **「ZIP に KUDGIVT.csv 自体が無い」ケースは存在しません** — `process_zip` が
`KUDGIVT.csv not found in ZIP` で `Err` を返し upload ごと失敗するので
([dtako_upload.rs:157-161](crates/alc-dtako/src/dtako_upload.rs:157))、運行行そのものが作られません。

### Q1-b. 誰が解消するのか

- **確実**: `POST /api/split-csv/{upload_id}` — 対象の upload を名指しできる
  ([dtako_upload.rs:1591-1606](crates/alc-dtako/src/dtako_upload.rs:1591))。
  R2 の ZIP は消えないので、いつでも再実行できます。
- **確実ではない**: `POST /api/split-csv-all`。理由は 2 つ。
  1. **候補列挙が運行と upload を結び付けていない** — `list_uploads_needing_split` の JOIN 条件は
     `uh.tenant_id = o.tenant_id` **だけ**で、`o` と `uh` の対応関係がありません
     ([repo/dtako_upload.rs:194-210](crates/alc-dtako/src/repo/dtako_upload.rs:194))。
     実質「テナント内に `has_kudgivt = FALSE` の運行が 1 件でもあれば、
     そのテナントの completed upload を**全部**候補にする / 0 件なら**空**」という挙動です。
  2. **filename で重複排除している** — `seen_filenames.insert(f.clone())` で
     同名の upload は先頭 1 本だけ残ります
     ([dtako_upload.rs:1618-1622](crates/alc-dtako/src/dtako_upload.rs:1618))。
     cron の自動アップロードは **filename が固定文字列 `"csvdata.zip"`**
     (`nuxt-dtako-admin/workers/dtako-scraper-relay/src/dtako-scraper-relay-do.ts:844`)、
     VPS 側 `dtako-scraper` もサイトが吐く `csvdata.zip` をそのまま送ります。
     → **毎日ぶんの upload が 1 本に潰れ、残りは `split-csv-all` では永久に再 split されません。**
     しかも `ORDER BY uh.filename` は同名同士の順序を決めないため、**どの 1 本が残るかは不定**です。
- **副次的**: あとから同じ `unko_no` の KUDGIVT 行を含む別の ZIP が上がって split が成功すれば TRUE になります。

### Q2. `kudgivt_unko_nos` に何が入るか

**「その ZIP の `KUDGIVT.csv` を `group_csv_by_unko_no` に通した結果のキー集合」**です。
つまり **ZIP 内 KUDGIVT.csv に 1 行以上出てくる運行だけ**が入ります
([dtako_upload.rs:909-934](crates/alc-dtako/src/dtako_upload.rs:909))。
→ **親の理解 (「ZIP 内に KUDGIVT.csv が無い運行は入らない」) で合っています。**
より正確には「**KUDGIVT.csv に自分の行が無い運行は入らない**」で、
ファイル自体の有無ではなく**行の有無**で決まります (ファイルが無ければ upload ごと失敗するため)。

**★ ここに 2 系統の不一致リスクがあります**:

| | R2 key / `kudgivt_unko_nos` 側 | `dtako_operations.unko_no` 側 |
|---|---|---|
| 取り方 | `line.split(',').next()` = **1 列目の生文字列** ([alc-csv-parser/src/lib.rs:43-46](crates/alc-csv-parser/src/lib.rs:43)) | ヘッダー名 `運行NO` で**列位置を引き**、値を `.trim()` ([kudguri.rs:74](crates/alc-csv-parser/src/kudguri.rs:74) / [:126](crates/alc-csv-parser/src/kudguri.rs:126)) |
| 前後空白 | **trim しない** | **trim する** |
| 列位置 | **常に 1 列目**と仮定 (コメント: 「運行NO is always the first column」) | ヘッダー名で解決 |

同梱のテスト fixture では KUDGIVT の 1 列目が `運行NO` なので現状は一致しますが、
**強制されている不変条件ではありません**。ずれた場合:

- R2 には `{tenant}/unko/<ずれたキー>/KUDGIVT.csv` として**書かれてしまう**
- `UPDATE ... WHERE unko_no IN (...)` は **0 行**
- それでも `query.execute(...).await?` は `Ok` を返し
  ([repo/dtako_upload.rs:412](crates/alc-dtako/src/repo/dtako_upload.rs:412) で `rows_affected` を捨てている)、
  呼び出し側は `kudgivt_unko_nos.len()` を使って
  **`has_kudgivt updated: N operations` という成功ログを出します**
  ([dtako_upload.rs:963](crates/alc-dtako/src/dtako_upload.rs:963))
- → **「R2 は書けたが DB を更新しなかった」が、ログ上は成功として残ります**

### Q3 (再掲・確定). PUT が全部失敗しても `has_kudgivt` は立つ

**立ちます。** `kudgivt_unko_nos` はアップロード**前**の準備ループで ZIP のパース結果から積まれ
([dtako_upload.rs:931-933](crates/alc-dtako/src/dtako_upload.rs:931))、
PUT ループは `Result` を捨て ([:950-951](crates/alc-dtako/src/dtako_upload.rs:950))、
`update_has_kudgivt` は**アップロード結果を一切参照せず**無条件に呼ばれます
([:955-965](crates/alc-dtako/src/dtako_upload.rs:955))。
→ PUT が 1 件も成功しなくても `has_kudgivt = TRUE` → items に `etag: null` で載る →
#205-21 が拾える。**D1 単独では「静かに減る」を作れません。**

### この節から出る示唆

- **`has_kudgivt` は gate の入力集合を決める事実上のスイッチなのに、落ちる経路が 5 つあり、
  そのうち (b) (c) は「R2 は書けたが DB は FALSE」で、しかも (c) は成功ログを出します。**
- **最小の可観測化**: `update_has_kudgivt` で `rows_affected` を返し、
  `kudgivt_unko_nos.len()` と一致しなければ `warn` を出す。実装 1 行 + シグネチャ変更で、
  (c) が即座に見えるようになります。
- **`split-csv-all` の filename dedup ([:1618-1622](crates/alc-dtako/src/dtako_upload.rs:1618)) は、
  cron の固定ファイル名と噛み合って「復旧手段が 1 日ぶんしか効かない」状態を作っています。**
  取りこぼしを一括で直したい場合、現状は `upload_id` を列挙して
  `POST /api/split-csv/{upload_id}` を個別に叩くしかありません。
- 案 C の評価は変わりません (**条件付きで進める**)。drift 検知 (M5) の対象に
  `has_kudgivt` を含めるべき、という追記 1 の示唆がさらに強まりました。

---

## 7. 親への [質問] (Cloudflare connector が必要)

1. **`ohishi-dtako` と `dtako-uploads` は別バケットか。**
   nuxt-dtako-admin の `DTAKO_R2` binding は `dtako-uploads`、alc の本番は `ohishi-dtako`。
   コードからは別物に見えるが確証が取れない。**同一だったとしても結論は変わらない**
   (admin の write は `vehicle-settings/` `y-time` `poi/` `etc` prefix のみで `unko/` を書かない)
   が、前提として確認しておきたい。
2. **`ohishi-dtako` に write 権限のある R2 API token / service token が他に無いか。**
   `ohishi-dtako-prod-api-202605` という名前が secrets-inventory のテストに見えるが、
   実際の権限 (read-only か read-write か) と配布先が分からない。
3. **bucket に event notification / Queue consumer が付いていないか。**
   付いていれば「R2 への書き込みを受けて DB を直す」仕組みを後付けできる (M5 の代替)。
4. **Cloudflare ダッシュボードから手動アップロードした運用実績があるか。**
   あるなら M5 (drift 検知) の優先度が上がる。
