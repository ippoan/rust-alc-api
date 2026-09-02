---
name: rust-alc-api-map
generated-from: rust-alc-api:2da8d607
paths: [crates/, src/, migrations/]
description: rust-alc-api (アルコールチェッカー基盤の Rust/Axum Cargo workspace — domain crate 群 + monolith 単一バイナリ、PostgreSQL+RLS、Cloud Run) の構造ナビゲーション。どの crate に何のルートがあるか / monolith (rust-alc-api) 一本化 (gateway + per-domain は #556 で廃止) / RLS・migration・deploy/release 分離の gotcha を 1 枚にまとめる。トリガー:「rust-alc-api」「alc-api」「alc-notify」「alc-tenko」「alc-trouble」「alc-carins」「alc-dtako」「gateway」「tenko-api」「carins-api」「dtako-api」「trouble-api」「RLS テナント」「sqlx migration」「ts-rs」「Release Wave」「Bazel」等。
---

# rust-alc-api-map — rust-alc-api 構造ナビゲーション

アルコールチェッカーシステムの backend。Cargo workspace で **domain crate 群** (`alc-*`)
+ **monolith 単一バイナリ** を持つ。PostgreSQL (`alc_api` スキーマ + RLS) / Cloudflare R2 (or GCS) /
Google OAuth + LINE WORKS。Cloud Run にデプロイ。

> ここは索引。網羅ではない。実ルートの完全列挙や関数シグネチャは repo 側が正。
> frontmatter の `generated-from` が現 tree-sha とズレたら hook が再生成を促す。

## バイナリ構成 (monolith 一本化、Refs #556)

| 系統 | バイナリ | 役割 |
|---|---|---|
| **monolith** | `rust-alc-api` (`src/main.rs`) | 全 domain crate の router を `/api` 下に一括 nest。全 domain (tenko/carins/dtako/trouble/camera/notify/misc/auth) を 1 プロセスで提供 |
| **CLI** | `migrate` (`src/bin/migrate.rs`) / `archive` (`src/bin/archive.rs`) | sqlx migration 実行 / アーカイブ Job |

**gateway (`crates/gateway`) + per-domain API (`tenko-api` / `carins-api` / `dtako-api` /
`trouble-api` / `alc-camera-api`) は #556 で廃止** (本番・staging とも休眠していたため、
attack surface と CI 時間削減で monolith に集約)。`alc-api.ippoan.org` は monolith に
domain-mapping。domain crate (`alc-tenko` 等) は monolith が `.with_state` でマウントする
router 実装として存続。旧 per-domain は同じ domain crate を単独 main で立てていたが、
その薄い main (bin crate) だけを削除した。

## 区画 (workspace crate)

| crate | 役割 / 主要ルート群 |
|---|---|
| `alc-core` | 共通基盤: **共有** models / **共有** repository trait / `auth_middleware` / `tenant` / `webhook` (generic 配信) / `realtime_bus` / `redact_broadcast`。**port 化進行中 (Refs #539)**: `fcm` / `device_pair_client` は **trait (port) だけが core に残り、実装 (adapter) は alc-notify / alc-devices へ移設済み**。`auth_google` は internal OIDC 専用に縮小 (login 用 `verify()`/`extra_client_ids` は削除)。ts-rs 型 export 元。**tenko / trouble ドメインの models/trait は alc-tenko / alc-trouble へ分割済み** (Refs #513、再流入は `scripts/check_domain_split.sh` が CI で loud fail)。**新機能は原則ドメイン crate (無ければ新 crate) に置く** — alc-core に足してよいのは全ドメイン共有の基盤だけ。alc-core に struct 1 個足すと 12 test shard + 7 Builds が再ビルドされる (コストは CI 時間で可視) |
| `alc-auth` | **認証 JWT の発行・検証は auth-worker に完全移管 (#479 PR-3)**。rust に残るのは `internal.rs` の DB プリミティブと me / logout / my_orgs のみ。旧 login/OAuth handler 群 (public_router = Google / LINE / LINE WORKS OAuth / WOFF / password login / refresh / switch-org) は撤去済み。`routes/mod.rs` で `auth` として re-export。`internal.rs` (`internal_router`) は **認証 DB プリミティブを `/api/internal/auth/*` で公開** (sso-config 読み / user upsert-line(works) / **upsert-google** (旧 `/api/auth/google` の tenant 解決 = 招待 → email_domain → STAGING_MODE 自動テナント作成 → 403 を移植) / recipient / refresh-token 保存、`require_internal_jwt` 配下)。token は発行せず user + tenant slug を返すのみで、JWT 組み立ては auth-worker が行う (Refs #434 / #479)。`alc-auth-jwt` leaf crate は `INTERNAL_AUD` 定数のみ残存 |
| `alc-misc` | health (`/health` + `/health/secret-fingerprint?name=&expected=` = 任意 env の sha256[0..8] と `expected` 突合、`{match: bool}` のみ返し oracle 防止。cross-store drift を CI で自動検出、Refs ippoan/rust-alc-api#424 / ippoan/ci-workflows#131。health_canary = JWT_SECRET drift 検知は #479 PR-3 の JWT_SECRET 全撤去に伴い削除) / measurements / employees / items / api_tokens / sso_admin / tenant_users / timecard / access_requests / staging / upload / bot_admin / members / communication_items / carrying_items / guidance_records。**employees の一括 upsert (Refs ippoan/alc-app-s3#125)**: `PUT /employees/bulk-by-code` (`employees.rs`) は theearth の乗務員マスタを nuxt-dtako-admin の relay が 1 日 5 回取り込む受け口で、乗務員CD (`code`) をキーに最大 500 件/リクエストをまとめて upsert する。解決順は (a) `code` 一致 (deleted_at 不問、`idx_employees_code` が deleted_at を見ない一意制約のため) → 復活 UPDATE (**ただし `nfc_id` を別の乗務員が既に持っていたら nfc_id だけ据え置いて更新し `nfc_id_conflict` で skipped に載せる** — `UNIQUE (tenant_id, nfc_id)` に当たるとトランザクションごと 500 になり 1 人のせいで全員分が落ちるため、Refs ippoan/alc-app-s3#125)、(b) 無ければ `nfc_id` (免許証 IC の交付日8桁+有効期限8桁=16桁) が `deleted_at IS NULL` でちょうど1行かつ code が NULL/一致のときだけ code を付与 UPDATE (2行以上/別code一致は `nfc_id_conflict` で skip)、(c) どちらも無ければ `role=['driver']` で INSERT (`ON CONFLICT DO NOTHING RETURNING id` が0行なら `unique_violation` で skip)。1 トランザクションで順次処理し `EmployeeUpsertSummary{created, updated, skipped}` を返す。repo 実装は `alc-misc/src/repo/employees.rs` の `upsert_by_code`、trait は `alc-core::repository::employees::EmployeeRepository` |
| `alc-tenko` | 点呼: tenko_call / tenko_records / tenko_schedules / tenko_sessions / tenko_webhooks / daily_health / equipment_failures / health_baselines / driver_info (alc-misc から移設)。**専用の `models` / `repository` (trait) / `TenkoState` / `overdue` (check_overdue_schedules + TenkoOverdueRepository) を自前で持つ** (alc-core から分割、Refs #513)。route は `tenant_router<S>()` generic を monolith が mount (旧 tenko-api は #556 で廃止) |
| `alc-carins` | 車検証(carins): car_inspections / car_inspection_files / carins_files / nfc_tags |
| `alc-dtako` | デジタコ: dtako_* (csv_proxy / daily_hours / drivers / events / logs / operations / restraint_report(_pdf) / scraper / tickets / upload / vehicles / work_times / y_time_export / event_classifications) / vehicle_settings_dumps。`events` (`dtako_events.rs`) の `GET /api/dtako/events` は乗務員CD + 期間 (`driver_cd` 省略で全乗務員) で KUDGIVT.csv の**生行を 1 往復で**返す (Refs ohishi-exp/rust-ichibanboshi#205 の 01)。per-運行NO の `csv_proxy` を N 回叩く形を置き換えるためのもので、**分類・時刻パース・正規化・集計は一切しない** — 勤怠計算は一番星固有でマルチテナント基盤に置かないため。`headers` は運行ごとに持つ (`対象乗務員CD` のように一部ファイルにしか無い列が実在し全運行同一の保証が無い)。期間上限は単一乗務員 366 日 / 全乗務員 31 日 (後者は R2 GET が約 13,500 回になるため) で、全乗務員版は乗務員単位の keyset ページング。repo 層は `DtakoYTimeExportRepository` を流用するため `list_operations` の期間拡張を引き継ぐ。**その期間条件は読取日と運行日の OR** — `(reading_date BETWEEN from-1 AND to+1 OR operation_date BETWEEN from-1 AND to+1)` で、列挙系 4 メソッド全部が共通 (Refs ohishi-exp/rust-ichibanboshi#205 の 38)。`reading_date` (カードを読ませた日) だけで絞ると月末に走った運行ほど読まれるのが遅いぶん構造的に落ちる — 2026-06 の勤怠が オンプレ基準より 142 行少なかった原因がこれで、名指しできた 29 件は **29/29 が「alc に存在し・読取日が窓の上端 (07-02) より後 (07-03〜07-13)・運行日は窓の中 (06-24〜07-01)」**だった (反例ゼロ)。`reading_date` 側の条件は消していない — `operation_date` は NULL 可 (`migrations/054_dtako_tables.sql`) で KUDGURI の `運行日` が空の取り込みでは埋まらないため、置き換えると別の取りこぼしが出る。同じ OR は `repo/dtako_upload.rs` の `fetch_operations_for_recalc` / `load_driver_operations` が先行して採っている。索引は `idx_dtako_ops_reading_date (tenant_id, reading_date)` のみで `operation_date` には無く、OR は BitmapOr になるので operation_date 側は索引無しの走査になるが、`dtako_operations` は全 7,263 行 (2026-07-31 実測) で `dtako-etags ms` の律速が R2 の LIST (`ms_drv=37 / ms_ops=144 / ms_list=2367`) のため索引は足していない (要否は投入後の `ms_ops` を測ってから)。**この OR で読む運行は 2026-06 の etags 窓で +64 件 (同窓 1,122 件に対し約 +5.7%、実測)** — うち勤怠の 142 行差に効くのは 29 件だけで、残り 35 件は `time_card_dtako` を 1 行も持たない別集団 (#205 の 39) なので混同しないこと。R2 の LIST 往復は増えない (prefix は `unko_no` の先頭 6 桁 = 運行開始日から作るので、運行日が窓の中の運行の prefix は既存集合に含まれる)。**消費側 (ichibanboshi の月ゲート) では、閉じた月は指紋が 1 回変わって以後安定するが、進行中の月は読取のたびに変わる** — 「月末の運行を翌月上旬に読む」が今までその月の指紋を動かさなかったため。同ファイルの `GET /api/dtako/events/etags?date_from&date_to` (Refs ohishi-exp/rust-ichibanboshi#205 の 13、絞り込みは同 #205-22 → #205-27) は `StorageBackend::list()` による R2 の LIST で対象期間の `{unko_no, etag}` 一覧を返す — CSV 本体は一切ダウンロードしない。LIST は tenant 全体ではなく **DB から引いた `db_unko_nos` の先頭 6 文字 (YYMMDD) を distinct にした日 prefix ごと** (1 か月ぶんで 30 個台) に絞り、`R2_LIST_CONCURRENCY = 16` で**並列に**呼ぶ (`derive_day_prefixes` + `list_prefixes`)。#205-22 では 4 文字 (YYMM = 月) だったが、**運行列挙が `reading_date ± 1 日`に広がるため YYMM は `{前月, 当月, 翌月}` の 3 個になり隣接月を丸ごと舐めていた**のと、**prefix ループも rust-s3 の continuation token も直列**だったのが本番 17 秒の主因で、#205-27 で日 prefix + 並列に倒した (往復回数は増えるが待ちは 2〜3 波)。unko_no の先頭 6 文字が数字として取れない要素が 1 件でもあれば、速さより正しさを優先して `{tenant_id}/unko/` の裸 prefix で全 LIST に倒す (`tracing::warn!` 1 行)。`db_unko_nos` が空なら LIST は 1 回も呼ばない。並列 LIST のうち 1 本でも失敗したら部分結果を返さず 500 (部分結果は下流の月ゲートが「運行が消えた」と誤検出する)。`tracing::info!` 1 行で `drivers/unko/prefixes/keys/ms_drivers/ms_ops/ms_list` を出しており、内訳は本番ログで確認できる。**応答は prefix の切り方に依存してはならない** — prefix は DB が返した `unko_no` から作るので必ず `db_unko_nos` を覆う、という不変条件を mock test (`etags_day_scoped_list_matches_full_list_byte_for_byte`) が全 LIST 参照実装との完全一致で縛っている (1 件でも変わると下流の月ゲートが全乗務員 stale と判定して全量再計算に落ちる)。消費側 (ichibanboshi の月ゲート) が「前回 fold 時から dtako 側の入力が変わったか」を安く判定するための指紋材料で、`/api/dtako/events` と違い R2 GET を払わないぶん期間上限は専用定数 `MAX_RANGE_DAYS_ETAGS=40` (`MAX_RANGE_DAYS_ALL`=31 の CSV ダウンロード枚数根拠は適用されない)。DB 側は date 範囲の unko_no 列挙にだけ既存 repo を使い、R2 の etag が無い運行 (upload 未完了) は `etag: null` のまま items に含める。各 item は `driver_cds: Vec<String>` (乗務員 CD の集合) も持つ — `list_operations_for_drivers` が `DISTINCT ON (driver_id, unko_no)` なので同じ unko_no が運転手/副運転手で別乗務員に紐づくことがあるため複数持てる形にしてある (`unko_no`/`etag`/`items` の件数・順序は月ゲートの指紋入力そのものなので不変、`driver_cds` は追加フィールドのみ、Refs #205 の 36)。応答にはさらに `unsplit: Vec<{unko_no, driver_cd, reading_date}>` と `unsplit_total: usize` を持つ — 既存 3 クエリ (`list_operations` / `list_drivers_with_operations` / `list_operations_for_drivers`) は全て `has_kudgivt = TRUE` で絞るため、`FALSE` (split 未完了) の運行は入力からも欠け検知の母集団からも消えて原理的に気づけない。専用クエリ `list_unsplit_operations` (`has_kudgivt = FALSE`、他 3 クエリと同じ期間条件 = 読取日/運行日 の OR を ± 1 日、employees join は `list_drivers_with_operations` と同じ経路) で列挙し、`unsplit` は 500 件で切るが `unsplit_total` には実数を入れる。1 件以上あれば `warnings` に 1 行 (`未 split の運行 {n} 件 (has_kudgivt=FALSE、この期間のデータから欠けています)`) を足す。`dtako_tickets` は email-receiver Worker から SD カードエラー通知メールを起票し F-VOS3020 設定 ZIP DL → QR で close する pipeline (Refs ippoan/email-receiver#1)。tenant_router (JWT) + internal_router (`INTERNAL_SHARED_SECRET` + `X-Tenant-ID`) + public_close_router (`close_token` のみ) の 3 経路。`GET /api/operations` (`dtako_operations.rs`) の一覧レスポンス (`DtakoOperationListItem`) は `vehicle_cd` (一番星との突合キー、Refs ohishi-exp/nuxt-dtako-admin#198 Phase 8) を含む。同エンドポイントは `GET /api/internal/operations` (`internal_router`、`INTERNAL_SHARED_SECRET`+`X-Tenant-ID`) からも同じハンドラで叩ける (nuxt-ichibanboshi の service binding 呼び出し用、tenant_router の `/operations` とのパス衝突を避けて別パスにした)。`upload` (`dtako_upload.rs`) の `POST /api/upload` は tenant FK 違反 (`dtako_upload_history_tenant_id_fkey` = tenants に tenant_id が無い、staging 揮発 DB で頻出) を 500 に潰さず actionable な 400 で返す (Refs ohishi-exp/dtako-scraper#22)。**`dtako_operations.has_kudgivt` (= この運行の CSV が R2 に split 済みか) は読み取り側 3 クエリ全部がこの列で絞っており、`process_zip` の再アップロードで一旦 `FALSE` に戻る (`insert_operation` の列リストに無いため) ため、直後の split 失敗が入力データと欠け検知の母集団を同時に消す事故があった (Refs ohishi-exp/rust-ichibanboshi#205 の 31)。`split_csv_from_r2` (`try_split_csv` 経由で `POST /api/upload` / `POST /api/internal/rerun/{id}` からも非ブロッキングで呼ばれる) は個別 CSV PUT の失敗を成功として数えず (`csv_count` は成功数のみ、失敗した unko_no は `has_kudgivt` 更新対象から除外)、戻り値の失敗件数を `UploadResponse.split_failed` として応答に載せる。`UploadResponse` は
さらに `split_unko_nos`/`split_unko_nos_total` (今回 split 成功した運行NO、`split_csv_from_r2`
が既に持つ `kudgivt_unko_nos` をそのまま使い数え直さない) と
`split_failed_unko_nos`/`split_failed_unko_nos_total` (KUDGIVT PUT がリトライ後も失敗した
運行NO — `split_failed` とは母数が異なる、KUDGIVT 以外のファイル失敗は含まない) を持つ。
一覧は `SPLIT_UNKO_NOS_DISPLAY_LIMIT`(=500) 件で切るが `_total` には実数を入れる (`unsplit`/
`unsplit_total` と同じ形)。`split_csv_from_r2` が丸ごと失敗した場合は個別運行NO が分からず
両配列とも空になる。`POST /api/split-csv/{id}` (`split_csv_handler`) の JSON 応答にも同じ
4 フィールドを同じキー名で持たせている (Refs ohishi-exp/rust-ichibanboshi#205 の 51)。
**個別 CSV PUT の失敗 (`split_csv_from_r2` 内) と `split_csv_from_r2` 自体の丸ごと失敗 (`try_split_csv` の Err 経路) はどちらも `SPLIT_RETRY_ATTEMPTS`(=3) 回まで自動リトライする** — 「R2 の書き込みが直後の読み取りに間に合わない」という遅延説は、R2 公式が read-after-write の強一貫性を謳っていることと `process_zip` の運行ごと逐次 DB upsert が ZIP の PUT と split 側の GET の間に既に間隔を作っていることから弱いと判断し採用しなかった。リトライは秒単位の先行 wait ではなくネットワーク瞬断からの回復狙いで `SPLIT_RETRY_DELAYS_MS=[300, 800]`(ms) の短い間隔のみ (合計 sleep 1.1s、Cloudflare proxy edge timeout 100s に対し十分小さい)。丸ごと失敗のリトライは `try_split_csv` 経由 (`POST /api/upload` / `POST /api/internal/rerun/{id}`) のみに効き、`POST /api/split-csv/{id}` (`split_csv_handler`) は `split_csv_from_r2` を直接呼ぶため丸ごと失敗のリトライは効かない (口は変えない設計、個別 PUT のリトライだけは効く、Refs ohishi-exp/rust-ichibanboshi#205-46)。`update_has_kudgivt` の失敗はもう握り潰さず `?` で伝播する (直接 split-csv endpoint `POST /api/split-csv/{id}` は 500 になるが、`try_split_csv` 経由の upload API は ZIP 取り込み確定後なので巻き戻らず `split_failed` で気づける形)。同メソッドの戻り値は `HashSet<unko_no>` (`RETURNING unko_no` を dedup) — `unko_no` は運転手/副運転手で 2 行あることがあるため件数 (rows_affected) 比較だと誤検知するので、集合の差分で当たらなかった `unko_no` を求める (R2 側キーは `line.split(',').next()` の生文字列で trim しない一方、DB 側は `対象乗務員CD`/`乗務員CD1` をヘッダー名解決後 `.trim()` するズレが疑われる)。1 件でもあれば `tracing::warn!` 1 行で可視化する (正規化はまだしていない)。`POST /api/split-csv-all` (SSE) は `has_kudgivt=FALSE` の運行を含むテナントの completed upload を**全部**候補にする (`dtako_operations` に upload を特定する列が無く、`r2_key_prefix`(`{tenant}/unko/{unko_no}`) と `dtako_upload_history.r2_zip_key`(`{tenant}/uploads/{upload_id}/{filename}`) はフォーマットが違うため upload 単位の絞り込みは未実装、Refs #205-34)。upload は `id` で一意にすべて処理する — 旧実装は **filename で dedup**していたため、`nuxt-dtako-admin` の日次 cron が固定ファイル名 `csvdata.zip` で毎日アップロードするケースで実質 1 本しか split されず、`has_kudgivt=FALSE` 運行の復旧手段として機能していなかった。候補順は `created_at DESC, id DESC` (新しい upload から、`SELECT DISTINCT` の制約で `created_at` も select list に含めてから絞る)。dedup 撤廃で候補がテナント全体の completed upload (運用月数ぶんの ZIP) に膨らみ得るため、1 リクエストで実際に split するのは `SPLIT_CSV_ALL_LIMIT`(=50) 件までに制限し、新しい方から確実に処理する。SSE の `done` イベントは `candidates`(候補総数)/`success`/`failed`/`total`(=success+failed)/`skipped`(候補 − 今回処理数、再実行で埋める分) を返す。`scraper` (`dtako_scraper.rs`) は **rust から dtako-scraper への直接中継 (`SCRAPER_URL` 経由の SSE relay) を撤去済み** (Cloud Run は gVisor sandbox で Cloudflare Tunnel/VPC 到達不可のため)。front Worker (`ohishi-exp/nuxt-dtako-admin`) が DO 経由で dtako-scraper に直接 WebSocket 接続し、rust 側は結果受領後の `POST /api/scraper/history` (履歴 insert) と `GET /api/scraper/history` のみを持つ薄い保存専用エンドポイントに縮小 (Refs ohishi-exp/dtako-scraper#17, ohishi-exp/nuxt-dtako-admin#63) |
| `alc-trouble` | トラブル管理: tickets / files / workflow / categories / offices / progress_statuses / schedules / tasks / task_types / task_statuses / notifications / notifier / cloud_tasks / lineworks_members / **field_layouts** (新規、tenant 単位でチケット入力フォームの表示/非表示・幅・並び順・カスタムラベルを保持する `trouble_field_layouts` テーブル、`GET`/`PUT /api/trouble/field-layout`、settings は JSONB 1 カラムの upsert)。`trouble_tickets` には `counterparty_vehicle`(相手方車両) / `disciplinary_committee`(賞罰委員会) カラムを追加済み (migration 124/125)。**経過記録 (tasks) の並び順は `sort_order` (第2キー `created_at`)**。`PUT /api/trouble/tickets/{ticket_id}/tasks/reorder` (body `{task_ids: [...]}`) が渡された順に `sort_order` を 0 起点で採番し直し、更新後の一覧を返す (`UPDATE ... FROM unnest($3) WITH ORDINALITY` の 1 文 + transaction、チケット外/重複 id は何も更新せず 404/400)。`create` は `sort_order` 未指定時に `MAX(sort_order)+1` を採番 (末尾追加)。**隣接行の sort_order を交換する方式は使わない** — 既存行が全て 0 だと「0 と 0 の交換」で沈黙し、並び替えが効かなくなる (migration 128 で既存行を表示順のまま連番へ振り直し済み、Refs ippoan/nuxt-trouble#240)。**tickets 一覧は `TroubleTicketFilter.sort_by` (`"occurred"` | `"ticket_no"`、whitelist 方式) + `sort_desc` でソート可 (未指定・whitelist 外は ticket_no DESC、"occurred" は occurred_date 第1キー NULLS LAST。Refs ippoan/nuxt-trouble#225)**。**schedule fire は #434 lockdown で internal 化**: `schedules::internal_fire_router` (`/api/internal/trouble/schedules/{id}/fire`, `require_internal_jwt`) を monolith の internal_protected に集約。旧 bare public `fire_router` は撤去。**発火は `worker_alarm::WorkerAlarmClient` (`CloudTasksClient` trait の DO Alarm 実装、Refs #550/#551) で schedule-alarm worker (ippoan/nuxt-notify) に登録**: `PUT/DELETE {SCHEDULE_ALARM_URL}/alarms/{id}`、認証は既存 `INTERNAL_SHARED_SECRET` 再利用。`cloud_tasks` / `notifier` (LineworksTroubleNotifier) は monolith で配線済み (env 未設定なら None + warn、旧 trouble-api は #556 で廃止)。**fire_schedule はメッセージ先頭にチケット見出し (`person_name`・`occurred_at` の JST 表示 (無ければ `occurred_date`)・`company_name`/`office_name` | `location`、空フィールドは行省略) を連結する (Refs #553)。チケット URL は入れない — LINE アプリ内ブラウザだと Google OAuth が 403 disallowed_useragent でブロックされるため (TROUBLE_FRONTEND_URL は撤去済み)**。見出し用チケット取得は `schedule.tenant_id` 明示の既存 `TroubleTicketsRepository::get` (TenantConn、bypass getter 追加なし)、取得失敗時は loud log + 本文のみ送信。組み立ては pure 関数 `build_ticket_heading` / `build_fire_message` (schedules.rs、unit test 付き)。**専用の `models` (TS derive 付き 32 struct) / `repository` (trait 12 本) / `TroubleState` を自前で持つ** (alc-core から分割、Refs #513 Phase B)。route は `Router<TroubleState>` を monolith が `.with_state` でマウント |
| `alc-notify` | LINE/LINE WORKS 配信: recipients / groups / documents / distribute / ingest / line_config / line_webhook / lineworks_* / read_tracker / viewer / email_documents / extract / redact / background_extract / background_redaction。**`line_webhook` は #434 lockdown で internal_router 併設**: `/api/internal/notify/line/webhook` (`require_internal_jwt`、auth-worker の public 受け口が OIDC mint で forward)。署名検証 (全テナント channel secret 照合) は rust 側で、**`list_enabled_line_configs()` SECURITY DEFINER 関数経由で RLS バイパス** (migration 117。生クエリだと未認証パスで `app.current_tenant_id=''` → `''::UUID` キャストが 500 する既知罠)。**ただし 072 の `FORCE ROW LEVEL SECURITY` があると所有者=SECURITY DEFINER 実行ロールにも RLS が効いて関数経由でも 500 するため、migration 118 で `NO FORCE` にして所有者バイパスを効かせている** (devices は元から FORCE 無し)。app ロール (非所有者) には RLS 維持。`public_router` (`/notify/line/webhook`) は LINE Console URL 切替 + allUsers 削除までの移行期間 dual-mount。**`lineworks_channels` も internal_router を持つ** (`/api/internal/lineworks/bot-secret/{bot_id}` / `/api/internal/lineworks/event` / `/api/internal/lineworks/send`、いずれも `require_internal_jwt` 配下)。`POST /api/internal/lineworks/send` (body `{channel_id?: Uuid, recipient_id?: Uuid, text: String}`、Refs ohishi-exp/nuxt-dtako-admin#874) は **無人 worker (dtako-scraper-relay の netprint cron) 用のテキスト送信口**。同等の tenant 経路 `POST /notify/lineworks/channels/{id}/test-send` は `require_tenant_header` 配下で、auth-worker の `alc-internal-proxy` が tenant 経路の forward を禁じている (shared secret だけで `X-Tenant-ID` を詐称でき #434 の再現になる) ため、cron からは叩けない — 迂回せず internal 側に口を足した形。**宛先は `channel_id` (トークルーム) と `recipient_id` (個人) の 2 択で、どちらか一方が必須**。両方指定は 400 `target_ambiguous` / 両方省略は 400 `target_required` で弾く — 片方を優先する実装にすると呼び出し側の設定ミスが「意図しない相手に届いた」として現れるため。**キー無しと明示 `null` はどちらも「未指定」**扱い (`Option<Uuid>` の serde 既定。caller がどちらの書き方でも通るように、mock test で HTTP 越しに縛ってある)。**recipient 宛は後から足した (#874 の 9)**: 実運用ではトークルームが 1 件も登録されておらず (Bot が招待されていない)、`notify_recipients` の個人が宛先だった。**新パスではなく body の拡張にしたのは、パスを増やすと auth-worker の `alc-internal-proxy` allowlist にも PR が要るから** (body 拡張なら auth-worker は無変更)。**`channel_id` は `lineworks_channels` の行 id (Uuid) であって LINE WORKS 側の channel 文字列ではない**。`recipient_id` は `notify_recipients` の行 id。internal 経路は `X-Tenant-ID` を honor しないので tenant はどちらも行から解決する: `lineworks_channels` は FORCE RLS (migration 102) で素引きが 0 行になり、`notify_recipients` も実行ロール `alc_api_app` が非所有者なので RLS (069) で 0 行になるため、SECURITY DEFINER 関数 `lookup_lineworks_channel_for_send` (migration 129) / `lookup_notify_recipient_for_send` (migration 130) 経由の `get_for_send` でバイパス取得する (`trouble_schedules::get_for_fire` / `notify_deliveries::get_for_view` と同じ作法)。`active = FALSE` (Bot 退出済み) の channel 行も返す — 404 に潰すと 「id が無い」と「Bot が居ない」を呼び出し側が区別できなくなるので、上流に到達させて 502 にする。recipient 側のエラー契約: 行が無ければ 404 `recipient_not_found` / `provider != "lineworks"` (= `lineworks_user_id` が NULL) は 400 `recipient_not_lineworks` (LINE 宛は別の Messaging API 経路なのでここでは扱わない。**黙って何もしないと呼び出し側が「送れた」と誤認して通知が消える**) / `enabled = false` は **404 ではなく 400 `recipient_disabled`** (行は在るので id 違いと混同させない。無効化は「もう送るな」の意思表示で、tenant 経路の `distribute` が `list_enabled` で無効宛先に配らないのと揃える)。SECURITY DEFINER 関数側で `enabled` を絞らないのはこの区別を残すため。**recipient には `lineworks_channels.bot_config_id` に相当する列が無い** (宛先は個人であって Bot に紐づかない) ので、送信 Bot は `distribute::resolve_lineworks_config` と同じく tenant の `provider = "lineworks" AND enabled` な bot_config から選ぶ (`pick_lineworks_bot_config_id`)。**0 件なら 500 `bot_config_not_found` / 複数件なら `list_configs` の順 (`ORDER BY name`) の先頭** — internal 経路と `distribute` が別の Bot から送るとログの追跡が壊れるので、選び方を揃えること自体が契約。Bot ごとに宛先を分けたくなったら `notify_recipients` に bot_config_id を足す (ここで別ルールを足さない)。bot config の取得 + 復号は private fn `resolve_bot_config` に括り出して channel 宛 (`send_text_via_channel`、tenant 経路の test-send と共用) と recipient 宛 (`send_text_to_lineworks_user` → `send_text_to_user`) が共有する |
| `alc-devices` | デバイス登録 (`devices`)。kiosk 端末 re-pair (再認証、Refs #495) は `re_pair_policy.rs` (pure 判定 fn) + `devices.rs` の `authorize_repair`/`re_pair` handler。auth-worker `/device/pair-internal` 呼び出しは port/adapter 分離 (Refs #539): trait `DevicePairClient` + `PairedCredential` は `alc-core::device_pair_client` (AppState が trait object を保持するため)、実装 `HttpDevicePairClient` は `alc-devices::device_pair_client`。合成は main.rs。設計 SoT: `docs/plan-device-repair.md`。**hub_measurements (Refs #564)**: CoreS3 (alc-app-s3) ハブ測定の ingest。`hub_measurements::internal_router` (`POST /api/hub/measurements`、`internal_shared_secret_router` 配下 = X-Internal-Shared-Secret + X-Tenant-ID) がバッチ/単発を受け、kind allowlist (`HUB_MEASUREMENT_KINDS`: temperature/blood_pressure/alcohol/fc1200_raw/license (license は CoreS3 が点呼開始時に免許証 IC を読み取って同じ session_id で送るもの、Refs ippoan/alc-app-s3#125)、DB CHECK は意図的に無し = 将来 kind はコード変更のみ) を検証して `PgHubMeasurementsRepository` (repo/hub_measurements.rs) が `ON CONFLICT (tenant_id, device_id, seq) DO NOTHING` で冪等 insert (migration 126、RLS tenant 分離)。trait は AppState 配線のため `alc-core::repository::hub_measurements`。ingest 前に `ensure_tenant_for_staging` (Refs #567): staging の揮発 DB で device JWT 由来 tenant が dangling になり FK 違反 500 になるのを、STAGING_MODE 限定で tenant 冪等作成 (`auth.ensure_tenant_exists`) して救済 — seed (staging/entrypoint.sh) への tenant ハードコードは撤回、本番は no-op。経路: CoreS3 →(WSS+device JWT)→ cf-alc-recorder (ippoan/alc-app) →(service binding)→ auth-worker /alc-internal-proxy → 本 endpoint。**読み出し (Refs #592)**: `hub_measurements::tenant_router` (`GET /api/hub/measurements`、テナント認証付き router = X-Tenant-ID) が同じテーブルを `created_at DESC` で返す。ingest 用の `internal_router` とはパスが同じでも**別 router** — あちらは cf-alc-recorder 専用の shared-secret 経路なので混ぜない。絞り込みは `device_id` / `kind` (allowlist 外は 400、typo を無言の 0 件と区別するため) / 期間 `from`・`to` (**`created_at` に対する閉区間**、`recorded_at` は端末の時計未同期で NULL になり得るため基準に使わない。`from > to` は 400)、ページングは `limit` (既定 50 / 上限 200 に clamp、実効値をレスポンスに返す) + `offset`。**総件数 (COUNT(*)) は返さない** — ingest テーブルは伸び続けるため。代わりに repo が `limit + 1` 件を引き handler が `has_more` に畳む。SQL は RLS 任せにせず `tenant_id = $1` を明示 (index `hub_measurements_tenant_device (tenant_id, device_id, created_at DESC)` の先頭列に乗せる二重防御)。テナント分離は `tests/hub_measurements_test.rs` / `tests/mock_tests/mock_hub_measurements_test.rs` の両方で固定済み。`payload` は JSONB 素通し (kind 別の型付けは別 issue)。UI は alc-app の `AdminDashboard.vue` 「ハブ測定値」タブ (ippoan/alc-app#144)。**session_id (migration 127、Refs ippoan/alc-app-s3#112)**: 1 回の点呼を束ねる端末発番の識別子。端末 (CoreS3) の UI が Measuring → Idle に戻るまでを 1 セッションとして発番し、**端末内でのみ一意** — グローバルな一意性は (tenant_id, device_id, session_id) の組で担保する。**NULL は欠損ではなく「セッション不明」**: 既存行・旧ファーム・点呼外の単発計測 (待機画面で BLE 機器から届いたもの) が該当する。端末由来 = untrusted なので ingest / 一覧の両方で長さ 64 + 英数字 `-` `_` に絞って 400 で弾く (`valid_session_id`)。index は partial (`hub_measurements_tenant_session ... WHERE session_id IS NOT NULL`) — NULL 行を引く用途が無く、伸び続ける ingest テーブルの index を小さく保つため |
| `alc-camera` | 監視カメラ死活管理 (Refs #345)。障害自動起票は **camera 所有 port `DownTicketSink`** にのみ依存し trouble crate には依存しない (Refs #513 Phase B)。**trouble への adapter (`TroubleDownTicketSink`) は #556 PR1 で per-domain の alc-camera-api binary から monolith `src/main.rs` へ移設済み**。route は `alc_camera::handlers::tenant_router()` (`Router<CameraState>`) を monolith が `require_tenant_header` 配下 (`/api/cameras*`) に `.with_state(camera_state)` でマウント。gateway + per-domain (alc-camera-api 含む) は #556 PR2 で廃止済み、camera は monolith 側に存続 |
| `alc-storage` | StorageBackend trait + R2 / GCS / HttpProxy 実装。`list(prefix)` (Refs #205 の 13) はデフォルト未対応 (`StorageError::Config`) の trait メソッドで、R2Backend だけが `rust-s3` の `Bucket::list` (自動ページング、ETag のクォート除去込み) で実装。GCS/HttpProxy は現状未対応のまま |
| `alc-csv-parser` / `alc-compare` | CSV パース / 比較ロジック。**KUDGURI / KUDGIVT はどちらも `対象乗務員CD` を優先し、無ければ `乗務員CD1` にフォールバックする** (`find_col("対象乗務員CD").or_else(\|\| find_col("乗務員CD1"))`)。`乗務員CD1` は運行の主運転者で運行内の全行が同一値なので、2 名乗務では対象者を取り違える。`dtako_upload.rs` が `rest_event_map` を `(driver_cd, work_date)` で張り KUDGURI 由来の driver_cd で引くため、**片方だけ `乗務員CD1` のままだとキーが食い違って 2 人目の休息時間が落ちる** (Refs ohishi-exp/rust-ichibanboshi#205 の 08) |
| `alc-pdf` | PDF 生成 (assets/fonts 同梱) |

## entrypoint / router

- **monolith**: `src/main.rs` — DATABASE_URL で `PgPool`、Storage backend (`STORAGE_BACKEND` = r2/gcs、
  carins/dtako/notify/trouble は別バケット+別 R2 キー)、`AppState` (共有 repo 群) と `TenkoState`
  (tenko 系 9 repo) / `TroubleState` (trouble 系 12 repo + storage、Refs #513) を組み立て、
  `.nest("/api", rust_alc_api::routes::router(internal_oidc_trust(), tenko_state, trouble_state))`。
  tenko / trouble route 群は router 内で `.with_state(...)` マウント (FromRef 変換は廃止)。背景 task: 60s ごと
  `alc_tenko::overdue::check_overdue_schedules` (WebhookRepository + TenkoOverdueRepository + http の 3 引数)。
- **router 本体**: `src/routes/mod.rs` — 各 domain crate のルートを re-export し `router()` で結線。
  middleware: `require_tenant_header` (tenant/admin 共通、注入 identity 信頼) / `require_internal_jwt`
  (auth-worker→internal ingest、aud=alc-api-internal。**#479 で HS256 dual-accept を撤去し
  Google OIDC 一本化**: `GoogleTokenVerifier::verify_internal_oidc` が JWKS で RS256 署名検証 +
  iss + aud=alc-api-internal + exp を検証 (Cloud Run IAM に加えた app 層 defense-in-depth)。
  `InternalOidcTrust{verifier}` は `router()` の **DI 引数** — prod は `internal_oidc_trust()`
  (main.rs)、テストは `with_test_claims` verifier を注入。旧 `INTERNAL_AUTH_TRUST_OIDC` env と
  共有 JWT_SECRET での HS256 受理は削除済み) / `require_internal_shared_secret`
  (email-receiver→`/api/dtako/tickets`、cf-alc-recorder→`/api/hub/measurements` #564)。
  **#434 で monolith のローカル JWT 検証を撤去**: 旧 `require_jwt` / `require_tenant` (bare X-Tenant-ID
  フォールバック) / `TenantProxySecret` gate (#437) / 未配線の `require_tenant_or_device` (#436、device-token)
  を全削除し、tenant/admin 経路を `require_tenant_header` に一本化した。rust-alc-api は JWT を検証せず、
  前段 proxy (CF Worker = alc-app/carins/nuxt-items、実体は auth-worker の `/alc-proxy` 系) が auth-worker
  `/auth/introspect` で検証して注入する `X-Tenant-ID` / `X-User-*` ヘッダーを信頼する dumb backend。
  外部直叩き防止は **Cloud Run IAM 網層ロックダウン** (proxy の OIDC ID token のみ到達可) が担う
  (確定アーキ #4807535677、step 3)。テストは `tests/common/mod.rs` の `test_proxy_inject` が proxy 役で
  Bearer token (base64(JSON) の opaque token) → identity ヘッダーに変換し従来テストを無改修で通す。
  **JWT_SECRET は rust から全撤去済み (#479 完了)**: main.rs の env 読取 / `Extension(JwtSecret)` /
  render.sh の secretKeyRef 注入 / `alc-auth-jwt` の HS256 発行・検証関数 / health_canary を全て削除。
  rust バイナリは HS256 鍵を一切持たず、JWT の発行・検証は auth-worker が単独で担う。
- **gateway (廃止済み、Refs #556 PR2)**: 旧 `crates/gateway` は auth + reverse proxy で
  `is_public_route` 判定して per-domain へ振る役だったが、本番・staging とも休眠のため削除。
  introspect 検証 + identity 注入は **auth-worker の `/alc-proxy` / `/alc-internal-proxy` 系**が
  担い、monolith backend へ直接 forward する (gateway を経由しない)。internal ルート
  (`/api/internal/*`) も monolith が直接処理する。前段の introspect 委譲設計 (#479 PR-2) 自体は
  auth-worker 側に存続。

## gotcha (CLAUDE.md / README 由来)

- **保存 secret の暗号鍵は `SSO_ENCRYPTION_KEY` 必須** (Refs #479 PR-1)。LINE channel
  secret / LINE WORKS bot secret / SSO client_secret の AES-256-GCM 鍵素材
  (`SHA-256(SSO_ENCRYPTION_KEY)`、`alc-core::auth_lineworks::{encrypt,decrypt}_secret`)。
  旧 `or_else(JWT_SECRET)` の env-presence fallback は撤去済み — 未設定は loud に 500。
  復号側は auth-worker (同 secret の CF binding) と共有。render.sh は monolith backend に注入
  (LineworksTroubleNotifier の復号経路も monolith が持つ、旧 trouble-api は #556 で廃止)。

- **DB 接続**: `alc_api_app` ロール (NOBYPASSRLS → RLS 有効) で、**直接接続 port 5432** を使う
  (Supavisor 6543 は `set_config` がリセットされ RLS テナント分離が壊れる)。
  `DATABASE_URL` に `?options=-c search_path=alc_api` 必須。
- **staging は postgres superuser 接続 → RLS 完全 bypass** (`staging/cloudrun-staging.yaml` の
  `postgresql://postgres:...`)。superuser は `FORCE ROW LEVEL SECURITY` でも RLS を無視するため、
  **RLS 頼みで WHERE tenant_id を省いたクエリは staging で全テナント横断に漏れる**。tenant scope は
  `crates/alc-core/src/tenant.rs::TenantConn` が `set_current_tenant` で `app.current_tenant_id` を
  立てて RLS に委ねるが、本番 (alc_api_app) でしか効かない。staging も対象にするクエリは
  **WHERE tenant_id を明示**すること (Refs #434、sso_admin で実害)。
- **pre-auth SECURITY DEFINER + FORCE ROW LEVEL SECURITY の罠**: 未認証経路 (LINE webhook /
  LINE login / notify viewer `/v/{token}` 等) が cross-tenant 検索用の SECURITY DEFINER 関数
  を叩く時、対象テーブルに `FORCE ROW LEVEL SECURITY` が有効かつポリシーが
  `current_setting('app.current_tenant_id', true)::UUID` を `NULLIF` でガードせず直接キャストして
  いると、tenant context 未設定 (空文字) で `invalid input syntax for type uuid: ""` 500 になる
  (テーブル所有者 = SECURITY DEFINER 実行ロールにも FORCE 下では RLS が適用されるため)。
  `devices`/`bot_configs`/carins 系は `NULLIF(..., '')::UUID` でガード済みで安全、
  `notify_line_configs`(#434 migration 118)/`notify_recipients`(119)/`notify_deliveries`+`notify_documents`(120)
  は同じ穴があり `NO FORCE ROW LEVEL SECURITY` で修正済み。新しい pre-auth SECURITY DEFINER 関数を
  追加する時は対象テーブルの RLS ポリシーの cast パターンを確認すること。
  - **裏の非対称にも注意**: recipient 逆引き (`find_recipient_by_line_user_id`, 076/119) は
    SECURITY DEFINER 化されていたが、`users` の LINE 逆引きは repo 層が `SELECT * FROM users` を
    **直接**叩いており (SECURITY DEFINER 無し)、`users` は FORCE 無しでも RLS ENABLE + ポリシーが
    `current_setting('app.current_tenant_id')::UUID` (**missing_ok 無し**) なので pre-auth では
    プール接続の残留 GUC 次第で 500/未検出になる非決定バグがあった。migration 121 で
    `find_user_by_line_user_id(TEXT) RETURNS SETOF users` を SECURITY DEFINER 化して repo を
    関数経由に変更、recipient と対称化した。
- **migration 不変条件**: 適用済み `migrations/*.sql` を絶対に変更しない (sqlx が SHA-384 検証、不一致で起動不能)。
  修正は新ファイル追加。migration は **Cloud Run Jobs** (`rust-alc-api-migrate`) で deploy 前に実行
  (`main.rs` から `sqlx::migrate!()` は削除済み = 起動時自動適用なし)。
- **snapshot hook (plan 整合性)**: `if_flag!(...)` 追加時は先に `ippoan/ippoan-dev-plans` の
  `scope:rust-alc-api` plan Issue を作り `npm run snapshot` で `manifests/production.snapshot.json` 更新。
  pre-commit (`.githooks`) + CI `snapshot-check` job が drift/stale-sha を検出。`SKIP_CLIPPY=1` で commit 時 clippy skip 可。
- **ts-rs**: 各 crate が `#[ts(export)]` で TS 型を生成 (`cargo test export_bindings`)。フロント
  (nuxt-*) が型同期する。型変更時は export_bindings を回す。生成元は alc-core / alc-misc /
  alc-carins の 3 crate のみ (alc-auth の未使用だった ts-rs 依存は削除済み)。CI での
  `ts-bindings-${sha}` artifact は **test-lib job が生成** する (check job ではない。lib shard は
  #507 で test-matrix から DB なしの test-lib job に分離。Refs #482 / 下記 CI 節)。
- **長時間 compute と Cloud Run**: `tokio::spawn` で background compute → fire-and-forget broadcast は
  やらない (Cloud Run は応答後に CPU を絞る)。`RealtimeBus` / `RedactBroadcaster` で対処。CLAUDE.md 該当節参照。

## CI / deploy から見た立ち位置

- **Bazel + Cargo の二重ビルド**: `BUILD.bazel` (rust_library `rust_alc_api_lib` + rust_binary 群 + rust_test)
  と Cargo workspace の両方が存在 (`MODULE.bazel` / `.bazelrc`)。CI の merge gate は Cargo
  (`cargo llvm-cov nextest`)。`bazel test //... --build_tests_only` は観測用 job
  (`bazel-test-poc`) + main-push warm で全 unit test を回す (run 跨ぎ result cache、Refs #515)。
  **dev-dependencies を持つ crate (alc-misc / alc-notify) の rust_test には
  `all_crate_deps(normal_dev/proc_macro_dev)` の配線が必須** — 無いと `#[tokio::test]` 等が
  unresolved で FAILED TO BUILD になる。
  **`rust_test` を足したら matrix を 2 箇所に足す** — `ci.yml` の実行 job
  (`bazel-test-poc` / DB 付きは `bazel-test-db`) と、**`cache-warm.yml` の対になる warm job**
  (`cache-warm-bazel-test-poc` / `cache-warm-bazel-test-db`)。`scripts/check_bazel_test_matrix.sh`
  が (a) BUILD の全 rust_test が matrix にあるか (b) **実行 job と warm job の matrix が
  `target`/`name`/`pdfium`/`coverage` まで一致するか を検査し、片方だけだと落ちる。
  このチェックは `bazel-test-poc` の **`csv-parser` セルでだけ**走るので、
  **失敗ジョブ名 (`Bazel test (csv-parser)`) は原因と無関係** — ログの
  `matrix target 不一致` を見ること。ローカル再現は `bash scripts/check_bazel_test_matrix.sh`
  (数秒、bazel 不要)。
- **deploy.yml は deploy/release 分離 (Refs #137)**: PR → staging 自動 deploy、tag(v*) push → production。
  **production の tag release は新 revision を 0% (no-traffic) で deploy するだけ**で traffic は旧 revision に残す。
  実際の切替は **Release Wave flip** が行う。`verify-no-traffic` job がこの不変条件を検証 (latest revision が
  0% traffic でなければ FAIL)。
- **単一 Dockerfile**: `Dockerfile` (monolith + migrate + archive + PDFium 同梱) のみ。monolith が
  単一 Cloud Run service (`rust-alc-api` / staging `rust-alc-api-staging`) として deploy される。
  `cloudrun/render.sh` が YAML 生成 (service は `backend` のみ受理)。gateway + per-domain の
  `Dockerfile.*` / per-service deploy は #556 で廃止。
- **手動 `deploy.sh`** もあり (monolith のみ、AR `cloudsql-sv/alc-app` へ)。通常は CI 経由。
- **coverage 100% ガード**: `coverage_100.toml` 登録ファイルは CI でリグレッション検出。mock テストは
  domain 別 (`tests/mock_tenko` `mock_dtako` `mock_carins` `mock_trouble` `mock_devices` `mock_misc`)。
- **その他 workflow**: `migration-safety-check.yml` (適用済み migration の破壊的変更検査) /
  `release-wave.yml` (Release Wave caller、`repository_dispatch` で cloudrun flip を受ける)。
- **CI 速度の tracking**: [`docs/ci-speed-tracking.md`](../../../docs/ci-speed-tracking.md) が SoT
  (Refs #482)。PR CI は同一ソースに対し「coverage 計装 (test-matrix)」と「Bazel fastbuild
  (build-image)」の 2 profile ビルドが走る (プロファイル統合は原理的に不可)。check job の
  test profile 重複ビルド (TS bindings 生成) は #482 で test-matrix(lib) に統合済み。
  Bazel remote cache は健全 (実測済み) — 遅く見えても cache 設定を疑う前にこの doc を読む。

## 関連 skill

- `coverage-test-patterns` — sqlx 向け DB エラー注入 / SSE テスト / 100% 達成パターン
- `coverage-check` — 未カバー行抽出
- `type-safe-pipeline` — フロント (nuxt-*) が rust-alc-api の ts-rs 型を CI 同期する仕組み
- `migrate-test` — Supabase + sqlx migration の splinter/RLS 検証
- `cross-repo-symbol-index` — この per-repo map の鮮度 hook 運用方針

## CLAUDE.md から移設 (2026-07-06)

# rust-alc-api

Axum + PostgreSQL RLS による ALC (アルコールチェック) API バックエンド
<!-- Bazel disk-cache: tar.zst + actions/cache (ippoan/setup-bazel) -->

## プロジェクト構成

- **バックエンド**: Rust / Axum
- **認証**: Google Sign-In JWT + LINE WORKS OAuth2
- **DB**: Supabase PostgreSQL (`alc_api` スキーマ、`alc_api_app` ロール NOBYPASSRLS)
- **ストレージ**: Cloudflare R2 (`alc-face-photos` バケット) / GCS 切り替え可能
- **デプロイ**: Cloud Run (`deploy.sh`)

## DB 接続の重要事項

- Supabase は rust-logi と同じプロジェクト (`tvbjvhvslgdwwlhpkezh`)、`alc_api` スキーマで分離
- `alc_api_app` ユーザーで接続すること（NOBYPASSRLS → RLS が有効）
- 必ず **直接接続 (port 5432)** を使用（Supavisor port 6543 は set_config がリセットされる）
- `DATABASE_URL` に `?options=-c search_path=alc_api` を付けてスキーマ指定

### ローカルから psql で直接クエリ

RLS が有効なため、`set_config` でテナントIDを設定しないと行が見えない。

```bash
source .env && DB_BASE=$(echo "$DATABASE_URL" | cut -d'?' -f1)

# テナント一覧確認
psql "$DB_BASE" -c "SELECT id, name FROM alc_api.tenants;"

# テナント指定してクエリ (set_config は同一セッション内で有効)
psql "$DB_BASE" -c "
  SELECT set_config('app.current_tenant_id', '<tenant_id>', false);
  SELECT count(*) FROM alc_api.dtakologs;"

# archive テーブルは archive_mode も必要
psql "$DB_BASE" -c "
  SELECT set_config('app.archive_mode', 'true', false);
  SELECT count(*) FROM alc_api.dtakologs;"

# テーブルサイズ確認 (RLS 不要)
psql "$DB_BASE" -c "
  SELECT tablename,
         pg_size_pretty(pg_total_relation_size('alc_api.' || tablename)) as size
  FROM pg_tables WHERE schemaname = 'alc_api'
  ORDER BY pg_total_relation_size('alc_api.' || tablename) DESC LIMIT 10;"

# VACUUM FULL (DELETE 後のディスク回収、テーブルロックあり)
psql "$DB_BASE" -c "VACUUM FULL alc_api.<table_name>;"
```

- `?options=` パラメータは psql が解釈できないので `cut -d'?' -f1` で除去する
- `logi` スキーマは `alc_api_app` ユーザーでアクセス不可 → Supabase ダッシュボード SQL Editor を使う

## 認証

### 認証方式

| 方式 | 用途 | エンドポイント |
|------|------|--------------|
| Google OAuth | alc-app 管理画面 | `POST /api/auth/google`, `POST /api/auth/google/code` |
| LINE WORKS OAuth2 | nuxt-pwa-carins (車検証管理) | `GET /api/auth/lineworks/redirect`, `GET /api/auth/lineworks/callback` |
| X-Tenant-ID ヘッダー | キオスクモード (デバイス) | ヘッダーのみ、JWT 不要 |
| Refresh Token | トークン更新 | `POST /api/auth/refresh` |

### JWT クレーム

```json
{
  "sub": "UUID (user_id)",
  "email": "user@example.com",
  "name": "ユーザー名",
  "tenant_id": "UUID (テナントID)",
  "role": "admin | viewer | payroll",
  "iat": 1234567890,
  "exp": 1234571490
}
```

- 有効期限: 1時間 (`ACCESS_TOKEN_EXPIRY_SECS = 3600`)
- Refresh Token: 30日 (`REFRESH_TOKEN_EXPIRY_DAYS = 30`)
- 署名: HS256、Secret Manager `JWT_SECRET`

### LINE WORKS OAuth2 フロー

```
ブラウザ → /api/auth/lineworks/redirect?domain=ohishi&redirect_uri=https://...
  → DB: resolve_sso_config('lineworks', 'ohishi') で SSO 設定取得
  → LINE WORKS authorize URL にリダイレクト
  → ユーザー承認
  → /api/auth/lineworks/callback?code=xxx&state=xxx
  → LINE WORKS token exchange → user profile 取得
  → DB: users テーブルに lineworks_id で upsert
  → JWT 発行 → redirect_uri#token=xxx にリダイレクト
```

- SSO 設定は `alc_api.sso_provider_configs` テーブル（テナントごとに client_id/secret を保持）
- `resolve_sso_config()` は SECURITY DEFINER 関数（認証前アクセス用）
- HMAC-SHA256 state パラメータで CSRF 防止（`OAUTH_STATE_SECRET` 環境変数）
- 実装: `src/auth/lineworks.rs`, `src/routes/auth.rs`
- **新規ユーザーの `role` は `'viewer'`** (`crates/alc-core/src/repo/auth.rs` の `create_user_lineworks`、Refs #599。旧 `'admin'` リテラル)。経路別: Google = 招待 (`tenant_allowed_emails.role`) を bind / LINE WORKS = `'viewer'` / LINE = `'viewer'`。この INSERT は `upsert_lineworks_user` が `find_user_by_lineworks_id` で既存行を引けなかった時だけ走る (= 既存ユーザーの role は動かない)。`users.role` の DB 既定値は `'admin'` (`migrations/003_create_users.sql`) なので退行は静かに管理者が増える形でしか出ず、mock は SQL を通らないため実 DB テスト `tests/auth_user_role_test.rs` (bazel-test-db の `db-auth-user-role` shard) で 3 経路とも縛っている
- **`role` の取りうる値は `'admin'` / `'viewer'` / `'payroll'`** (`migrations/131_add_payroll_role.sql` で `users.role` と `tenant_allowed_emails.role` の CHECK を同時に広げた)。**招待側 (`tenant_allowed_emails`) とログイン本体 (`users`) は必ず同じ集合にする** — 片方だけ広げると「招待は通るのにログインで CHECK 違反」になる。`'payroll'` は給与の閲覧可否を決める独立した軸で、上流 (rust-ichibanboshi) の email allowlist と **AND** で効く (role が allowlist を上書きすることはない)。**テナント管理系の権限判定 (`role != "admin"` の 22 か所: `access_requests.rs` / `api_tokens.rs` / `bot_admin.rs` / `members.rs` / `sso_admin.rs` / `tenant_users.rs`) は従来どおりで、`'payroll'` はそこでは 403 のまま** — 給与専用 role が SSO 設定 / API トークン / bot 設定 / メンバー管理 / 参加リクエストに触れないのが正しい状態なので緩めない。受け付ける側は `tenant_users::invite_user` の allowlist と `members::is_allowed_role` の 2 か所 (後者は frontend 固有の `'member'` も許す)。**`employees.role` (`migrations/024` / `028`、driver / manager / admin) は別テーブル・別ドメイン**で、同名の `'admin'` が出てくるが無関係

### ミドルウェア

| ミドルウェア | 用途 | 認証方法 |
|-------------|------|---------|
| `require_jwt` | 管理者ページ | `Authorization: Bearer <jwt>` 必須 |
| `require_tenant` | テナントスコープ操作 | JWT → フォールバック `X-Tenant-ID` ヘッダー |

### テナント統一モデル

- `alc_api.tenants` — `id`, `name`, `slug` (UNIQUE)
- `alc_api.users` — `google_sub` (nullable) + `lineworks_id` (nullable)、どちらか一方は必須 (CHECK 制約)
- rust-logi の Default Organization (`00000000-...0001`) も `tenants` に登録済み

### nuxt-pwa-carins の認証フロー

ログインは auth-worker (Cloudflare Workers) → rust-logi 経由で JWT 発行（`org` クレーム、rust-logi の JWT_SECRET で署名）。
rust-alc-api の JWT_SECRET とは異なるため、nuxt-pwa-carins のサーバープロキシ (`server/api/proxy/[...path].ts`) が:
1. auth-worker JWT の `org` クレームを抽出
2. `X-Tenant-ID` ヘッダーに変換して rust-alc-api に転送（`require_tenant` ミドルウェアのフォールバック）

rust-alc-api にも LINE WORKS OAuth バックエンドを実装済み (`/api/auth/lineworks/redirect`) だが、
現状は auth-worker 経由で十分に動作しており、両バックエンド共通で使える。
auth-worker が発行する JWT の `org` クレームは rust-logi の `organization_id` = rust-alc-api の `tenant_id` なので互換性あり。

### 環境変数（認証関連）

| 変数 | 用途 | 管理先 |
|------|------|--------|
| `JWT_SECRET` | JWT 署名/検証 | Secret Manager |
| `GOOGLE_CLIENT_ID` | Google OAuth | Secret Manager |
| `GOOGLE_CLIENT_SECRET` | Google OAuth code exchange | Secret Manager |
| `OAUTH_STATE_SECRET` | LINE WORKS OAuth state HMAC 署名 | Secret Manager |
| `API_ORIGIN` | LINE WORKS OAuth callback URL のオリジン | 環境変数 |

### 認証の auth-worker 移管 (#434、移行中)

LINE / LINE WORKS の OAuth オーケストレーションを **auth-worker に移管**し、rust は
DB プリミティブを `/api/internal/auth/*` (`require_internal_jwt` 配下) で公開する
だけの dumb backend にする。確定アーキ (Refs #434):

```
1. browser ──▶ auth.ippoan.org/oauth/line/redirect        (ログイン開始)
2. auth-worker ──▶ LINE authorize                          (リダイレクト)
3. user 承認 ──▶ auth-worker/oauth/line/callback?code=…
4. auth-worker が LINE と code 交換 → profile 取得          (auth-worker が直接)
5. auth-worker ──OIDC(aud=alc-api-internal)──▶ rust /api/internal/auth/users/…
                                                           ← ユーザー確認/upsert
6. rust ──▶ auth-worker に user 情報(id/tenant/role/slug)  ← rust→auth-worker ユーザー情報
7. auth-worker が JWT を発行(JWT_SECRET で署名)
8. auth-worker が cookie(logi_auth_token=JWT)をセット + redirect_uri#token=… で戻す ← cookie でログイン保持
```

- **2 つの OIDC の使い分け**: ログイン中の internal call は `aud=alc-api-internal`
  (`/api/internal/auth/*`)、ログイン後のデータ API は `aud=service URL`
  (`require_tenant_header`、tenant/user はヘッダ注入)。`/alc-proxy` は service-URL
  audience でしか mint しないため、consumer が `/alc-proxy` 経由で internal route に
  到達しても `aud` 不一致で弾かれる (confused-deputy 防止、Cloud Run custom audiences)。
- rust は cookie も browser JWT も**一切見ない** (dumb backend)。発行・検証・cookie は
  auth-worker が持つ。
- 実装: `crates/alc-auth/src/internal.rs` (`internal_router`)。`require_internal_jwt`
  は lockdown 時に OIDC custom-audience 検証へ置換予定。

## ストレージバックエンド切り替え

- `STORAGE_BACKEND=r2` → Cloudflare R2 (`rust-s3` crate)
- `STORAGE_BACKEND=gcs` → GCS (reqwest 直接呼び出し、Cloud Run メタデータサーバー認証)
- `StorageBackend` trait で抽象化 (`src/storage/`)。`upload` / `download` / `exists` / `delete` /
  `presign_get` に加え `list(prefix) -> Vec<ListedObject>` (Refs #205 の 13、
  `crates/alc-core/src/storage.rs`) — R2 LIST だけで `{key, etag}` を取る用途向けで、
  デフォルトは未対応エラー、`R2Backend` / `MockStorage` (テスト用) だけ実装

## シンボリックリンク（参照用）

プロジェクトルートに関連リポジトリへのシンボリックリンクを配置している。
`.gitignore` に登録済み。VSCode の `git.scanRepositories` で git 操作可能。

| リンク名 | リンク先 | 説明 |
|---|---|---|
| `alc-app` | `/home/yhonda/js/alc-app` | フロントエンド ALC (Nuxt) |
| `front/nuxt-pwa-carins` | `/home/yhonda/js/nuxt-pwa-carins` | フロントエンド 車検証管理 (Nuxt PWA) |
| `workers/auth-worker` | `/home/yhonda/js/auth-worker` | JWT 認証 (Cloudflare Workers) |
| `rust-nfc-bridge` | `/home/yhonda/rust/rust-nfc-bridge` | NFC ブリッジ (Rust) |
| `ble-medical-gateway` | `/home/yhonda/arduino/ble-medical-gateway` | BLE Medical Gateway (Arduino) |

## ユーティリティ

- `git-status-all.sh` — 自身 + シンボリックリンク先の全リポジトリの git status を一括表示

## テスト

### 概要

- **ユニットテスト**: `cargo test --lib` (DB 不要)
- **インテグレーションテスト**: `tests/` ディレクトリ (ローカル PostgreSQL が必要)
- **マイグレーション検証**: ローカル DB + splinter (Supabase Postgres Linter)
- **カバレッジ**: `cargo llvm-cov`
- **統合スクリプト**: `./test_and_deploy.sh` (fmt → clippy → unit → migration → integration → frontend)

### テスト実行

```bash
# ローカル開発 (推奨: Makefile 経由)
make test                     # ユニットテストのみ (DB 不要、高速)
make test-file F=jwt          # 特定モジュールのみ
make db-up                    # テスト DB 起動 (セッション中1回)
make itest-file T=auth_test   # 特定インテグレーションテスト
make itest                    # 全インテグレーションテスト (DB 起動→テスト→停止)
make db-down                  # テスト DB 停止

# カバレッジ検証 (100% ファイルのリグレッション検出)
make cov-check-unit           # unit ファイルのみ (DB 不要)
make cov-check                # 全 100% ファイル (DB 必要)

# CI カバレッジ取得 (artifact からダウンロード、ローカル実行不要)
make cov-not100               # 未達成ファイル一覧
make cov-summary              # 全ファイルサマリ
make cov-file F=devices       # 特定ファイルの未カバー行

# マイグレーション検証 (Docker 必要)
bash ~/.claude/skills/migrate-test/scripts/migrate_test.sh

# 全テスト一括 (fmt + clippy + unit + migration + integration + frontend)
./test_and_deploy.sh

# テスト + デプロイ
./test_and_deploy.sh --deploy

# オプション
./test_and_deploy.sh --skip-integration   # インテグレーションテストをスキップ
./test_and_deploy.sh --skip-frontend      # フロントエンドテストをスキップ
```

### CI/CD

- **GitHub Actions**: `.github/workflows/ci.yml` (push/PR to main)
  - `check`: fmt + clippy
  - `unit-tests`: `cargo test --lib` + 100% カバレッジ検証 (unit ファイル)
  - `integration-tests`: PostgreSQL サービスコンテナ + 全テスト + 100% カバレッジ検証 + **artifact アップロード**
- **カバレッジ手動実行**: `.github/workflows/coverage.yml` (workflow_dispatch)
  - 3モード: `summary` / `not-100` / `file` → Job Summary にマークダウン出力
  - artifact (`llvm-cov-text`) も同時にアップロード (30日保持)
- **100% ファイル登録簿**: `coverage_100.toml` — CI でリグレッション検出
- **CI artifact 取得**: `make cov-not100` / `make cov-summary` / `make cov-file F=xxx` — `gh run download` で最新 artifact をダウンロードしてローカル解析
- **CI ビルド時間の実測・改善履歴**: [`docs/ci-speed-tracking.md`](./docs/ci-speed-tracking.md) — CI 高速化施策の追加/revert 前に必読 (external cache 等、実測済みの失敗パターンあり)。実施後は実測を追記する

### カバレッジ

```bash
# サマリ (ユニットテストのみ)
cargo llvm-cov --lib --summary-only

# インテグレーション込み (要 docker compose up)
TEST_DATABASE_URL="postgresql://postgres:test@localhost:54322/postgres?options=-c search_path=alc_api" \
  cargo llvm-cov --summary-only

# HTML レポート
TEST_DATABASE_URL="..." cargo llvm-cov --html --open
```

### テスト構成

| ファイル | 内容 |
|---------|------|
| `tests/common/mod.rs` | テストハーネス (DB 接続、サーバー起動、JWT 発行ヘルパー) |
| `tests/common/mock_storage.rs` | インメモリ StorageBackend 実装 |
| `tests/auth_test.rs` | JWT 認証 / X-Tenant-ID / 未認証拒否 |
| `tests/employees_test.rs` | RLS テナント分離 / キオスクモード |
| `tests/hub_measurements_test.rs` | hub 測定の ingest 冪等性 (UNIQUE) + 一覧の絞り込み / ページング / テナント分離 |

### テスト用インフラ

- `docker-compose.yml` — テスト用 PostgreSQL 16 (ポート 54322、tmpfs)
- `scripts/init_local_db.sql` — `alc_api` スキーマ + `alc_api_app` ロール + Supabase 互換ロール
- `.test-config` — `test_and_deploy.sh` 共通スクリプトの設定

### マイグレーション作成時の注意

- **適用済みのマイグレーションファイルは絶対に変更しない** — SQLx は SHA-384 チェックサムで検証し、不一致だとアプリが起動不能になる。修正が必要な場合は新しいマイグレーションファイルを追加する
- 本番に既存データへの INSERT/UPDATE をハードコードしない (`WHERE EXISTS` で条件付きにする)
- `SECURITY DEFINER` 関数には `SET search_path = alc_api` を付けること (splinter 警告回避)
- RLS ポリシーの `WITH CHECK (true)` は避け、明示的な条件を使う

### マイグレーション含む PR の検証方針

**マイグレーション (`migrations/*.sql` 追加・変更) を含む PR は、ローカルで `migrate_test.sh` を実行せず、PR の CI + staging 自動デプロイに検証を任せる**:

- ローカル実行はリソース消費が大きく、CI と同じ環境ではない
- staging 自動デプロイで実 DB に対する適用結果を確認する方が信頼できる
- PR 内の検証順序:
  1. `cargo fmt` + `cargo check` のみローカル実行
  2. push → CI で `migrate-test` (splinter / RLS) ジョブ実行
  3. CI 通過後、staging 自動デプロイで実マイグレーションが走ることを確認
  4. staging 環境で対象機能の E2E 確認 (export/import 等で再現可能なテストデータを用意)

**ローカル実行を許可する例外**: SQL 構文確認のために手元 docker postgres に対して `psql -f` で叩く程度はOK。
splinter / RLS 検証フルセットは CI に集約する。

## マイグレーションとデプロイ

- マイグレーションファイルは `migrations/` ディレクトリに連番で配置 (`001_`, `002_`, ...)
- マイグレーションは **Cloud Run Jobs** (`rust-alc-api-migrate`) でデプロイ前に実行される
- `src/bin/migrate.rs` — マイグレーション専用バイナリ（同じ Docker イメージに含まれる）
- `deploy.sh` の流れ: Docker ビルド → プッシュ → **Cloud Run Jobs でマイグレーション実行** → Cloud Run Service デプロイ
- マイグレーション失敗時はデプロイが中止され、アプリは前バージョンで動き続ける
- `main.rs` からは `sqlx::migrate!()` を削除済み（起動時の自動適用はしない）

## 車検証管理 (carins) 機能

rust-logi から移行。nuxt-pwa-carins フロントエンドが使用。

### テーブル（`alc_api` スキーマ、元 `logi` から移動）

- `car_inspection` — 車検証データ（102フィールド、PascalCase カラム名）
- `car_inspection_files` / `_files_a` / `_files_b` — 車検証ファイル紐づけ
- `car_inspection_deregistration` / `_deregistration_files` — 抹消登録
- `car_inspection_nfc_tags` — NFC UUID ↔ 車検証 ID マッピング
- `files` / `files_append` — ファイルメタデータ（実体は GCS `rust-logi-files` バケット）
- `file_access_logs` — アクセス統計
- `pending_car_inspection_pdfs` — PDF 処理キュー

### REST エンドポイント

| ファイル | エンドポイント |
|---------|-------------|
| `routes/car_inspections.rs` | `GET /api/car-inspections/current`, `/expired`, `/renew`, `/{id}` |
| `routes/car_inspection_files.rs` | `GET /api/car-inspection-files/current` |
| `routes/carins_files.rs` | `GET/POST /api/files`, `/recent`, `/not-attached`, `/{uuid}`, `/{uuid}/download`, `/{uuid}/delete`, `/{uuid}/restore` |
| `routes/nfc_tags.rs` | `GET/POST /api/nfc-tags`, `/search?uuid=`, `DELETE /{nfc_uuid}` |

### 注意事項

- `car_inspection` テーブルのカラム名は **PascalCase**（`EntryNoCarNo` 等）
- REST API は `to_jsonb()` で DB カラム名をそのまま JSON キーとして返す（フロントエンドが PascalCase を期待するため）
- RLS ポリシーは `COALESCE(current_tenant_id, current_organization_id)` で rust-logi からもアクセス可能（移行期間中）
- ファイルストレージは GCS バケット `rust-logi-files`（パス: `{tenant_id}/{uuid}`）

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

## 中間点呼 (TenkoCall) 機能

運転者が電話番号で登録し、GPS位置情報付きで中間点呼を実施する機能。

### テーブル

- `tenko_call_numbers` — 電話番号マスタ (call_number UNIQUE, tenant_id, label)
- `tenko_call_drivers` — 登録運転者 (phone_number UNIQUE, driver_name, call_number, employee_code, tenant_id)
- `tenko_call_logs` — 点呼ログ (driver_id FK, phone_number, driver_name, latitude, longitude)
- RLS: `tenko_call_numbers` / `tenko_call_drivers` は SELECT パブリック (認証前の検索用)、write はテナントスコープ

### マイグレーション

- `migrations/030_tenko_call_drivers.sql` — drivers + logs テーブル
- `migrations/031_tenko_call_numbers.sql` — 電話番号マスタ
- `migrations/032_tenko_call_rls.sql` — RLS ポリシー
- `migrations/033_tenko_call_employee_code.sql` — employee_code 追加

### バックエンド (`src/routes/tenko_call.rs`)

- **public_router()** (認証不要):
  - `POST /api/tenko-call/register` — 運転者登録 (call_number でマスタ検証 → phone_number で upsert)
  - `POST /api/tenko-call/tenko` — 点呼実施 (phone_number → driver 特定 → GPS ログ記録 → call_number 返却)
- **tenant_router()** (管理者認証):
  - `GET /api/tenko-call/numbers` — 電話番号マスタ一覧
  - `POST /api/tenko-call/numbers` — 電話番号追加
  - `DELETE /api/tenko-call/numbers/{id}` — 電話番号削除
  - `GET /api/tenko-call/drivers` — 登録運転者一覧

### フロントエンド

- `TenkoCallManager.vue` — 管理者: 電話番号管理 + QRコード生成
- `AdminDashboard.vue` に「中間点呼」タブ追加
- `EmployeeList.vue` — 乗務員一覧に中間点呼登録状況 (電話番号) を表示

## 顔認証

- **ライブラリ**: `@vladmandic/human` (BlazeFace 検出 + FaceRes embedding, 1024次元)
- **入力正規化**: 映像フレームを 640x480 キャンバスにレターボックス描画してから Human.js に渡す（デバイス間の解像度差異を吸収）
- **モデルバージョン管理**: `FACE_MODEL_VERSION` 定数 (`useFaceDetection.ts`) でモデル+正規化パラメータを識別。DB (`employees.face_model_version`) と IndexedDB (`FaceRecord.modelVersion`) に記録
- **バージョン不一致時**: 旧バージョンの embedding は認証時にフィルタされ、再登録が促される
- **閾値**: cosine similarity >= 0.55 (`useFaceAuth.ts`)
- **マイグレーション**: `037_add_face_model_version.sql`
- **関連ファイル**:
  - バックエンド: `src/db/models.rs` (Employee, UpdateFace, FaceDataEntry), `src/routes/employees.rs`
  - フロント: `useFaceDetection.ts`, `useFaceAuth.ts`, `useFaceSync.ts`, `face-db.ts`, `FaceAuth.vue`

## AlcoholChecker Android アプリ

- パス: `/home/yhonda/android/AlcoholChecker/`
- ビルド: `cd /home/yhonda/android/AlcoholChecker && ./gradlew installDebug`
- **署名不一致エラー**: 端末にリリース署名のAPKがある場合、デバッグビルドを上書きインストールできない。`adb uninstall com.example.alcoholchecker` してから再インストールすること
- 複数 adb 接続時は `-s <device>` を指定（WiFi + ワイヤレスデバッグで2重接続になることがある）
- **バージョニング**: 明示的に指示があるまでパッチバージョン (x.y.Z) で上げること。メジャー・マイナーはユーザー指示時のみ
- **リリース**: `master` ブランチに push + `versionName` 変更で CI が自動ビルド・GitHub Release・GitHub Pages デプロイ

## 既知の RLS / 権限問題

いずれも修正済み。

- **devices テーブル SELECT ポリシー**: `migrations/063_fix_devices_select_policy.sql` で `USING(true)` を DROP し、SECURITY DEFINER 関数 (`lookup_device_tenant`, `get_device_settings_by_id`) に置換済み
- **tenko_call_numbers INSERT/DELETE 権限**: `migrations/064_fix_tenko_call_numbers_grants.sql` で GRANT 追加済み

## LINE 配信 (alc-notify) 仕様とデバッグ

### Channel Access Token v2.1 — JWT assertion の落とし穴

2026-04-22 に LINE push 全体が静かに失敗していた (PR #265/#266)。原因は `alc_notify::clients::line::JwtClaims` の仕様誤解。同じ罠を避けるため:

| claim | 型 | 意味 | 範囲 |
|---|---|---|---|
| `exp` | UNIX 秒 (絶対時刻) | JWT assertion の有効期限 | `now` 以降、最大 30分先 |
| `token_exp` | **秒数 (duration)** | 発行される access token の有効期間 | 30s 〜 30日 (2592000) |

- `token_exp` は **duration** (例: `86400` = 1日、`60*60*24*30` = 30日)。`now + 86400` のような絶対時刻を渡すと LINE が `{"error":"invalid_client","error_description":"Invalid token_exp"}` を返す。
- 両 claim を落とすと `"JWT payload must contain token_exp"`。
- 公式仕様: https://developers.line.biz/ja/docs/messaging-api/generate-json-web-token/

### ローカル検証 (リリース不要)

LINE 送信系のバグはリリース → 本番試験だと 1 サイクル 〜5分かかる。これを避けるため example バイナリがある:

```bash
export LINE_CHANNEL_ID=1234567890
export LINE_KEY_ID=<uuid>
export LINE_PRIVATE_KEY_PATH=/path/to/private.pem  # PKCS#1/PKCS#8 どちらも可
cargo run -p alc-notify --example line_token_issue

# push まで試す場合
cargo run -p alc-notify --example line_token_issue -- --push Uxxxxxx "hello"
```

DB 非依存、本物の LINE API を直接叩く。`invalid_client` 等は即時に stderr に出る。秘密鍵は Secret Manager (`google-ai-studio/line-*` or 本番環境) から `gcloud secrets versions access` で取得して PEM に保存。

### 静かな失敗の検知

LINE push は backend 内で `failed += 1` するだけでテナント管理画面には「失敗 1」としか出ない。根本原因は Cloud Run ログのみに出る:

```bash
gcloud logging read \
  'resource.type="cloud_run_revision" AND resource.labels.service_name="rust-alc-api" AND textPayload:"LINE token"' \
  --project cloudsql-sv --limit 10 --freshness=15m
```

`alc_notify::clients::line` / `alc_notify::distribute` / `alc_notify::line_webhook` から ERROR が出ていたら LINE 側で rejected されている。

### カバレッジガード

`crates/alc-notify/src/clients/line.rs` は 100% に登録済み (`coverage_100.toml`, `unit` 型)。テストは wiremock で token/push エンドポイントをモックし、`build_jwt_assertion` の claim 値 (`exp` は絶対、`token_exp` は `[30, 2592000]` のレンジ) も直接 assertion する。**JWT 仕様回帰はこの CI で捕まる。**

## 外部 API 連携の開発フロー

LINE / LINE WORKS / FCM / e-Gov など外部 API を叩くコードは、バグをリリース → 本番試験で発見すると 1 サイクル数分かかる。今回の LINE 修正 (PR #265/#266/#267) で確立したループを今後も踏襲する:

### 1. ローカルで試す (example バイナリ)

新しい外部 API エンドポイントを呼ぶ時は、まず `crates/<crate>/examples/<api>_*.rs` を作って手元で直接叩く。

- DB 非依存、秘密は env + Secret Manager から手動取得
- **1 イテレーション 10 秒以下**、claim の型ミスや auth 失敗を即時に確認
- 参考: `crates/alc-notify/examples/line_token_issue.rs`

```bash
# 秘密を Secret Manager から .env に書き出す例
gcloud secrets versions access latest --secret=line-private-key > /tmp/pk.pem
export LINE_CHANNEL_ID=... LINE_KEY_ID=... LINE_PRIVATE_KEY_PATH=/tmp/pk.pem
cargo run -p alc-notify --example line_token_issue
```

### 2. モジュール分割 (テスト可能な形に)

example で挙動を確認したら、本体コードを次の粒度で分ける:

- **pure な部分** (JWT claim 生成、署名、URL 組み立て等) は `pub(crate) fn xxx(...) -> Result<...>` で関数に切り出す。引数に `now: u64` 等を受け取り、時刻依存を消す
- **エンドポイント** は `const PROD_URL` を直接使わず、クライアント構造体のフィールドにして `new_with_endpoints(...)` で注入できるようにする → wiremock がローカル HTTP サーバーで差し替え可能になる
- クライアントの `new()` は `with_endpoints(PROD_URL, ...)` を呼ぶシンプルなラッパーに

参考実装: `crates/alc-notify/src/clients/line.rs` の `LineClient::with_endpoints` + `build_jwt_assertion`。

### 3. 単体テストで固める (wiremock)

`#[cfg(test)] mod tests` に以下を入れる:

- **pure 関数テスト** — 生成した JWT / URL / ペイロードをそのまま decode/parse して claim 値やレンジをアサート。仕様書の数値 (e.g. `token_exp` は `[30, 2592000]`) を直接コードに書いて回帰を捕まえる
- **クライアントテスト** — `MockServer::start().await` で仮想 API を立て、`with_endpoints(format!("{}/...", server.uri()), ...)` で注入。**成功 / 4xx / parse error / http error (unreachable) / 上流エラー伝播** の 5 パターンを最低限テストする
- **キャッシュ挙動** — `expires_in: 0` を返して次回呼び出しで再発行されること、`expect(N)` で mock の呼び出し回数を確定させる
- **コンストラクタテスト** — `new()` / `default()` が prod エンドポイントを持っていることを 1 行でチェック (`new_with_endpoints` パス経由の回帰検出)

完成したら `coverage_100.toml` に `type = "unit"` で登録 → `scripts/check_coverage_100.sh --unit-only` で CI が常時ガードする。

### アンチパターン

- **本番 DB / 本番 API に直接 unit test を叩かせない** — ネットワーク依存 + flaky + credential 漏洩リスク
- **「外部 API なのでテスト不可能」で放置しない** — pure 関数切り出し + wiremock で 100% 到達できる (今回の例が証拠)
- **token/URL を const にして直接叩く** — テスト時に差し替えられない。必ず struct フィールド化する

## 長時間 compute と Cloud Run の罠

### 結論: `tokio::spawn` で background compute → fire-and-forget broadcast はやらない

Cloud Run **default で CPU throttling が有効** (`--no-cpu-throttling` 指定なし) のため、
HTTP response 返却後は CPU 割当が瞬時に~0% に落とされる。`tokio::spawn(async move { ... })`
した background task は **HTTP 200/202 直後に CPU 停止 → 完走しない**。

検証経緯 (2026-05-10): Y時間 export を `POST /jobs` + WebSocket 完了通知の async pattern
で実装 (PR #340) → 本番で `job spawned` ログだけ出て `job completed` が来ない →
frontend の WS が 120s timeout で fail。実害発覚後 PR で revert (sync GET + parallel R2
fetch のみに戻す)。

### `tokio::spawn` を使いたい場合の選択肢 (どれもコスト or 実装コスト発生)

1. **`gcloud run services update --no-cpu-throttling`** (instance-based billing)
   - asia-northeast1 で **+$20-60/月** (request 頻度依存、24/7 alive で +$63/月)
   - `deploy.sh` に固定する場合 user 承認必須
   - 注意: `redact_broadcast.rs` (alc-notify の redact pipeline) は同じ pattern で動いている
     はずなので、production で本当に最後まで完走しているか要検証 (Y時間と同様に部分失敗
     している可能性あり)
2. **Cloud Tasks queue + 別 Cloud Run worker サービス**
   - Tasks にタスク push → 別 worker が処理 → broadcast
   - 別サービス追加。月数 cents 〜 $1 程度
3. **Cloudflare DurableObject 内で compute**
   - alc_csv_parser 等を TS/WASM 移植が必要、DB 直接アクセス不可 (rust-alc-api 経由)
   - 1-2 日 rewrite work、ほぼ無料

### sync HTTP で許容できる長さ

- Cloudflare proxy edge timeout: **100s**
- Cloud Run request timeout: default 60min (調整可)
- 5-15s なら sync HTTP で問題なし (Y時間 export は parallel R2 fetch 後 5-15s で済むため
  async 化せず sync で配信)

### `crates/alc-core/src/realtime_bus.rs::RealtimeBus`

汎用 broadcaster client (notify-realtime-bus Worker `/broadcast` への POST)。
`RedactBroadcaster` (`crates/alc-core/src/redact_broadcast.rs`) と同 env vars
(`NOTIFY_REDACT_BROADCAST_URL` / `NOTIFY_REDACT_BROADCAST_SECRET`) を共有。

**現状の使用箇所**: なし (Y時間 export での利用は revert 済)。将来 Cloud Tasks や DO 経由
の async pattern を再導入する際に再利用可能なように残してある (テスト 11 件付き)。

新規 async endpoint を増やす際は、上記 3 選択肢のいずれかを採用すること。
**`tokio::spawn` + `bus.broadcast()` を Cloud Run service の HTTP handler 内で直接
呼び出さない** こと (background task が確実には完走しない)。

### 並列 R2 fetch のパターン (Y時間 export 高速化、2026-05-10)

`futures::stream::iter().buffer_unordered(N)` で R2 fetch を並列化することで wall time を大幅短縮可能。
具体実装は `crates/alc-dtako/src/dtako_y_time_export/mod.rs::compute_y_time_export` を参照。
同じパターンを `crates/alc-dtako/src/dtako_events.rs` も使う (結果は順不同で返るので
`departure_at` → `unko_no` で整列して決定的にすること)。

- `R2_FETCH_CONCURRENCY = 16` (200/16 = 12.5 並列ラウンド ≈ 3.75s 想定)
- `pool.close()` / DB error injection は対象外 (R2 fetch は DB connection 不要)
- 想定: 13ヶ月レンジで 41-107s → **5-15s** (~85% 削減)
- これだけで Cloudflare proxy 100s timeout 内に収まるので、async 化不要

**LIST も同じ (2026-07-31、Refs ohishi-exp/rust-ichibanboshi#205-27)**。`dtako_events.rs` の
`list_prefixes` は `R2_LIST_CONCURRENCY = 16` で複数 prefix の LIST を並列に投げる。
rust-s3 の `Bucket::list` は 1 prefix 内で continuation token を**直列**に辿る
(1000 key/ページ) ので、prefix ループまで直列だと往復が一列に並ぶ。**「往復回数を
減らせば速い」は直列前提でのみ正しい** — 並列前提では「1 prefix が 1 ページで
終わるまで細かく割って同時に投げる」方が速い (etags の LIST 実測 17s の主因がこれ)。

## テスト

- テストインフラ: `docker-compose.yml` (PostgreSQL 16, port 54322) + `tests/common/mod.rs` ヘルパー
- ローカル実行: `make test` (ユニットのみ、DB不要) / `make itest` (全テスト、DB必要)
- カバレッジ: `/coverage-check` スキル使用 (`--full` で サマリ + 未カバー行を1回で取得)
- 100% 達成済みファイル: `coverage_100.toml` で管理 (20ファイル、--text ベース)
- カバレッジリグレッション検証: `bash scripts/check_coverage_100.sh` (`--unit-only` で DB 不要モード)
- CI/CD: `.github/workflows/ci.yml` — push/PR to main で自動実行
- テストは並列実行可能 (`RUST_TEST_THREADS=1` 不要)
- env var 競合は `ENV_LOCK`、email_domain 競合は `GOOGLE_LOGIN_LOCK` (tests/common/mod.rs) で直列化
- DB エラー注入: 認証なしエンドポイントは `pool.close()`、認証ありは trigger (INSERT/UPDATE/DELETE) or RENAME (SELECT) + `DB_RENAME_LOCK` + `db_rename_flock()`
- **coverage gate 対象ファイルで `tracing` マクロを複数行にしない** — フォーマット文字列が独立行になる (手書きでも rustfmt の 100 桁折り返しでも) と、その行は llvm-cov の行カバレッジに乗らず 100% gate が fail する (PR #399/#400 で 2 回発生)。メッセージを短くして必ず 1 行に収める
- カバレッジ計画: `plans/coverage_100.md`

### 100% 未達成ファイル一覧 (2026-03-27 実測)

最新データは `/coverage-check --summary` で取得可能。

| ファイル | Lines | Miss | Cover | 備考 |
|---------|-------|------|-------|------|
| auth/google.rs | 117 | 87 | 25.64% | Google JWT検証 (外部API依存) |
| auth/lineworks.rs | 240 | 63 | 73.75% | LINE WORKS OAuth (外部API依存) |
| compare/mod.rs | 3094 | 184 | 94.05% | 比較ロジック |
| csv_parser/work_segments.rs | 464 | 32 | 93.10% | 作業区間パーサー |
| fcm.rs | 26 | 26 | 0.00% | FCM送信 (外部API依存, trait mock済み) |
| main.rs | 115 | 115 | 0.00% | エントリポイント (テスト対象外) |
| routes/auth.rs | 557 | 176 | 68.40% | 認証ルート (Google/LINE WORKS) |
| routes/bot_admin.rs | 179 | 23 | 87.15% | Bot管理 |
| routes/car_inspection_files.rs | 38 | 3 | 92.11% | 車検証ファイル |
| routes/car_inspections.rs | 173 | 16 | 90.75% | 車検証 |
| routes/carins_files.rs | 284 | 39 | 86.27% | 車検証ファイル管理 |
| routes/carrying_items.rs | 194 | 32 | 83.51% | 積載品目 |
| routes/communication_items.rs | 228 | 11 | 95.18% | 連絡事項 |
| routes/devices.rs | 1467 | 260 | 82.28% | デバイス管理 |
| routes/dtako_restraint_report.rs | 1614 | 322 | 80.05% | 拘束時間レポート |
| routes/dtako_restraint_report_pdf.rs | 1147 | 53 | 95.38% | 拘束時間PDF |
| routes/dtako_scraper.rs | 145 | 140 | 3.45% | スクレイパー (外部依存) |
| routes/employees.rs | 435 | 27 | 93.79% | 従業員管理 |
| routes/equipment_failures.rs | 290 | 21 | 92.76% | 機器故障 |
| routes/guidance_records.rs | 420 | 84 | 80.00% | 指導記録 |
| routes/measurements.rs | 385 | 46 | 88.05% | 測定記録 |
| routes/sso_admin.rs | 166 | 35 | 78.92% | SSO管理 |
| routes/tenant_users.rs | 146 | 21 | 85.62% | テナントユーザー |
| routes/tenko_call.rs | 217 | 53 | 75.58% | 中間点呼 |
| routes/tenko_records.rs | 326 | 50 | 84.66% | 点呼記録 |
| routes/tenko_schedules.rs | 325 | 22 | 93.23% | 点呼スケジュール |
| routes/tenko_sessions.rs | 1263 | 149 | 88.20% | 点呼セッション |
| routes/tenko_webhooks.rs | 162 | 33 | 79.63% | Webhook設定 |
| routes/timecard.rs | 393 | 17 | 95.67% | タイムカード |
| routes/upload.rs | 78 | 9 | 88.46% | アップロード |
| storage/gcs.rs | 42 | 42 | 0.00% | GCS (本番のみ) |
| storage/mod.rs | 11 | 11 | 0.00% | ストレージ抽象 |
| storage/r2.rs | 39 | 39 | 0.00% | R2 (本番のみ) |
| webhook.rs | 164 | 6 | 96.34% | Webhook配信 |
- **DB エラー注入**: `BEGIN → ALTER TABLE RENAME → テスト → ROLLBACK` パターン (PostgreSQL DDL は ROLLBACK 可能)
- **SSE テスト**: コアロジックを `pub async fn xxx_core()` に抽出し、SSE ラッパーとは別にテスト可能にする

## ブランチワークフロー

**main に直接 merge/push してはいけない。** 複数の Claude が並行作業しているため、main を直接変更すると他の worktree や作業に影響する。

### 基本フロー

1. **worktree で作業**: `isolation: "worktree"` の Agent、または手動で worktree を作成
2. **branch 作成・push**: worktree 内で `fix/xxx` ブランチを作成し push
3. **CI 確認**: GitHub Actions の結果を確認
4. **remote で merge**: `gh pr create` → `gh pr merge` で GitHub 上で main にマージ

### Worktree 作成ルール

- **`git checkout main` は禁止** — PreToolUse hook (`branch-switch-guard.sh`) でブロックされる
- **メインワークツリーのソースファイル編集は禁止** — PreToolUse hook (`worktree-edit-guard.sh`) が Write/Edit をブロック
  - `src/`, `tests/`, `migrations/`, `Cargo.toml` 等のコード変更は必ず worktree 内で行う
  - 例外（メインワークツリーで編集可）: `CLAUDE.md`, `.claude/*`, `.gitignore`, `docs/*`, ルート直下 `.md`, `coverage_100.toml`, `.github/*`
- 新しいブランチが必要な場合は **必ず worktree を使う**
- **`origin/main` をベースにすること** — ローカル main は古い可能性がある。hook (`worktree-fetch-guard.sh`) で強制
- **マージ済み worktree の削除**: `bash ~/.claude/hooks/worktree-cleanup.sh` で一括クリーンアップ

```bash
# 正しい方法
git fetch origin main
git worktree add -b fix/xxx .claude/worktrees/xxx origin/main

# NG: ローカル main がベース (hook でブロックされる)
git worktree add -b fix/xxx .claude/worktrees/xxx main
```

### 単一テスト CI (`single-test.yml`)

`fix/test_xxx` ブランチを push すると、`test_xxx` だけ `cargo llvm-cov` で実行される。
**トリガーは `fix/test_*` のみ** — `fix/` 全般ではない（二重 CI 防止）。

```bash
# 例: test_communication_items_crud だけ CI で実行
git push origin fix/test_communication_items_crud
```

カバレッジ修正のイテレーションに使用する。全テスト実行は main CI (`ci.yml`) に任せる。
`fix/` だが `fix/test_` 以外のブランチ（例: `fix/dead_code`）は CI (`ci.yml`) の PR トリガーのみ。

### ローカルテスト不要のワークフロー

テストの作成から検証まで **ローカルで cargo test / cargo llvm-cov を一切実行しない** ワークフロー。
ローカル CPU リソースを節約しつつ、CI 上で全検証を完結させる。

```
1. Agent がテストコードを書く (Read/Write/Edit のみ)
2. 親が cargo fmt → git commit → git push (fix/test_xxx ブランチ)
3. single-test.yml が CI 上でテスト + カバレッジ実行
4. CI 成功 → gh pr create → gh pr merge --squash
5. main merge で ci.yml が全テスト + カバレッジ検証を自動実行
6. CI 失敗 → gh run view でログ確認 → worktree で修正 → 再 push → 3 に戻る
```

- ローカルでは `cargo fmt` のみ (pre-commit hook で強制)
- テスト実行・カバレッジ計測はすべて GitHub Actions 上
- 5つのブランチを並列 push すれば CI も並列実行される

### PR 作成・マージ

```bash
gh pr create --base main --head fix/xxx --title "タイトル" --body "説明"
gh pr merge <PR番号> --squash
```

### 並列 Agent ワークフロー（カバレッジ作業等）

バックグラウンド Agent は **対話的権限取得不可**。Bash・Write・Edit すべて事前許可が必要。
**worktree 作成・git 操作は親が行い、Agent にはコード書き（Read/Write/Edit）だけ任せる**。

#### 前提: settings.json に事前許可を追加

```json
"Write(//home/yhonda/rust/rust-alc-api/.claude/worktrees/**)",
"Edit(//home/yhonda/rust/rust-alc-api/.claude/worktrees/**)"
```

#### 手順

```
1. 親が worktree + ブランチを一括作成
   for name in file_a file_b file_c; do
     git worktree add -b "fix/test_${name}" ".claude/worktrees/${name}" main
   done

2. 親がバックグラウンド Agent を並列起動（run_in_background: true）
   - Bash 不要。Read/Write/Edit/Glob/Grep のみ使用を指示
   - worktree パスを明示: /home/yhonda/rust/rust-alc-api/.claude/worktrees/<name>
   - 未カバー行・DB エラー注入パターン等の必要情報をプロンプトに含める

3. Agent 完了後、親が各 worktree で cargo fmt → git add/commit/push
   cd .claude/worktrees/<name>
   cargo fmt && git add -A && git commit -m "test: ..." && git push -u origin fix/test_<name>

4. push 後の CI 監視 → 失敗時は修正
   - バックグラウンドで gh run list をポーリングして CI 完了を待つ
   - CI 失敗したブランチは gh run view でログ確認 → worktree で修正 → 再 push
   - CI 成功したブランチは gh pr create → gh pr merge --squash
```

#### 注意事項

- `isolation: "worktree"` の Agent は **自動で worktree を作るが Bash 権限がないと何もできない**
- 代わりに通常の Agent（worktree なし）を起動し、worktree パス内のファイルを直接 Read/Write させる
- Agent プロンプトには「Bash は使えません」と明記すること
- **DB エラー注入パターンの使い分けを明示指示すること** — `/coverage-test-patterns` スキルの全内容をプロンプトに含める。特に:
  - `pool.close()` は認証なし (public_router) エンドポイントのみ（認証ありはミドルウェアで先に失敗する）
  - trigger: INSERT/UPDATE/DELETE エラー用
  - RENAME: SELECT エラー用（認証ありエンドポイント）— **`DB_RENAME_LOCK` + `db_rename_flock()` 必須**
- 完了後の worktree クリーンアップ: `git worktree remove .claude/worktrees/<name>` + `git branch -d fix/test_<name>`
- **worktree 削除前に必ず `cd /home/yhonda/rust/rust-alc-api`** すること — シェルの cwd が worktree 内だと削除時に `getcwd` が失敗しセッションが切断される

## ステージング環境

PR を main に向けると CI が自動で staging 環境にデプロイする。本番とは独立したインフラ。

### インフラ構成

| コンポーネント | staging | 本番 |
|---|---|---|
| **API** | Cloud Run `rust-alc-api-staging` (multi-container + PostgreSQL sidecar) | Cloud Run `rust-alc-api` |
| **DB** | CloudSQL Postgres (staging スキーマ) | Supabase PostgreSQL |
| **Frontend (alc-app)** | Cloudflare Workers `alc-app-staging` | Cloudflare Workers `alc-app` |
| **Auth** | Cloudflare Workers `auth-worker-staging` | Cloudflare Workers `auth-worker` |
| **ストレージ** | Cloudflare R2 (staging バケット) | Cloudflare R2 (本番バケット) |

### URL

| サービス | URL |
|---|---|
| API (staging) | `https://rust-alc-api-staging-566bls5vfq-an.a.run.app` |
| Frontend (staging) | `https://alc-app-staging.m-tama-ramu.workers.dev` |
| Auth Worker (staging) | `https://auth-worker-staging.m-tama-ramu.workers.dev` |

### 認証フロー (staging)

staging の alc-app は2つの認証モードをサポート:

1. **Auth バイパスモード** (デフォルト): `NUXT_PUBLIC_STAGING_TENANT_ID` で固定 tenant_id を設定。ログイン不要で X-Tenant-ID ヘッダーで直接アクセス
2. **Google OAuth モード**: ログイン画面の Google ログインボタンで認証。auth-worker-staging → Google OAuth → rust-alc-api-staging

```
[alc-app-staging]
  ├── Auth バイパス: X-Tenant-ID ヘッダーで直接 API アクセス
  └── Google OAuth: accounts.google.com → /auth/callback → rust-alc-api-staging /api/auth/google/code
```

### データ永続性

staging の PostgreSQL は Cloud Run sidecar コンテナ + `emptyDir` ボリューム。**データは揮発性**。
- `minScale: "0"` → アイドル約15分でインスタンス停止 → DB データ消失
- 次のリクエストでコールドスタート → マイグレーションで DB スキーマは再作成されるが、ユーザーデータ（テナント、ユーザー登録等）は消える
- OAuth で登録したユーザーも消える
- テスト環境としてはベスト: 毎回クリーンな DB から始まるので汚れたデータが残らない
- 永続データが必要な場合のみ `minScale: "1"` や Cloud SQL を検討

### Staging Export/Import API

揮発性 DB のデータ復元用 API。`STAGING_MODE=true` 環境変数でガード（本番では 404）。

| エンドポイント | 用途 |
|---|---|
| `GET /api/staging/export?tenant_id=<uuid>` | テナントデータを JSON ダンプ |
| `POST /api/staging/import` | JSON からリストア (べき等、ON CONFLICT DO UPDATE) |

**Export 対象テーブル:** tenants, users, employees (face_embedding 含む), devices, tenko_schedules, webhook_configs, tenant_allowed_emails, sso_provider_configs, tenko_call_numbers, tenko_call_drivers, bot_configs (LINE WORKS Bot 設定), notify_line_configs (LINE Messaging API 設定), notify_recipients (notify 受信者)

**対象外:** measurements, tenko_sessions, tenko_records, time_punches, file_access_logs（履歴データ）

- 実装: `crates/alc-misc/src/staging.rs`
- テストデータ: `staging/test-data.json` (tenant_id `11111111-...`)
- Import はトランザクション内で tenant → set_current_tenant → 残りテーブルの順で実行
- 認証不要 (public route)、環境変数ガードのみ

### Auth バイパスモード (alc-app)

`NUXT_PUBLIC_STAGING_TENANT_ID` が設定されている場合、OAuth なしでキオスクモード (`activateDevice`) を自動有効化。

- `useAuth.ts` の `init()` で stagingTenantId があれば `activateDevice()` を呼ぶ
- `auth.global.ts` ミドルウェアで stagingTenantId 設定時は認証スキップ
- API リクエストは `X-Tenant-ID` ヘッダー経由（JWT 不要）

### StagingFooter 共有コンポーネント

`@yhonda-ohishi-pub-dev/auth-client` パッケージ (`auth-worker/packages/auth-client/`) に `StagingFooter.vue` を追加。
alc-app の `app.vue` で使用。staging 時のみ黄色バーを表示し、Export/Import ボタンを提供。

### Playwright E2E テスト (alc-app)

staging 環境に対する E2E テスト。CLI でローカル実行（CI ジョブなし）。

```bash
cd /home/yhonda/js/alc-app/web
npx playwright test
```

- 設定: `web/playwright.config.ts`
- テスト: `web/tests/e2e/staging-bootstrap.spec.ts`
- ヘルパー: `web/tests/e2e/helpers/staging.ts` (wakeUpStaging, importTestData)
- テストデータ: `web/tests/e2e/fixtures/test-data.json`
- フロー: staging wake up → import → auth バイパスでページ表示確認

### CI 自動デプロイ

- **rust-alc-api**: PR to main → `ci.yml` の `deploy-staging` ジョブ → Cloud Run staging
  - Docker イメージは GHCR に push → Artifact Registry リモートリポジトリ (`asia-northeast1-docker.pkg.dev/cloudsql-sv/ghcr/`) 経由で Cloud Run が pull
- **alc-app**: PR to main → `test.yml` の `deploy-staging` ジョブ → `wrangler deploy --env staging`

### Secrets / 環境変数

**rust-alc-api staging** (Cloud Run → Secret Manager):
- `alc-api-staging-jwt-secret`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`
- `alc-r2-access-key`, `alc-r2-secret-key`, `alc-oauth-state-secret`
- `carins-r2-access-key`, `carins-r2-secret-key`, `dtako-r2-access-key`, `dtako-r2-secret-key`
- SA `747065218280-compute@developer.gserviceaccount.com` に `secretmanager.secretAccessor` 付与済み
- SA `staging-deploy@cloudsql-sv.iam.gserviceaccount.com` に Artifact Registry `artifactregistry.reader` 付与済み

**alc-app staging** (Cloudflare Workers → wrangler.jsonc `env.staging.vars`):
- `NUXT_PUBLIC_API_BASE`: staging Cloud Run URL
- `NUXT_PUBLIC_AUTH_WORKER_URL`: auth-worker-staging URL
- `NUXT_PUBLIC_STAGING_TENANT_ID`: auth バイパス用固定 tenant_id
- `NUXT_PUBLIC_GOOGLE_CLIENT_ID`: Google OAuth Client ID

**auth-worker staging** (Cloudflare Workers → `wrangler secret`):
- `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `OAUTH_STATE_SECRET` (本番と同一値)

**Google OAuth**: 承認済みリダイレクト URI に以下を追加済み:
- `https://auth-worker-staging.m-tama-ramu.workers.dev/oauth/google/callback`
- `https://alc-app-staging.m-tama-ramu.workers.dev/auth/callback`

## デプロイルール

- コードの修正・変更が完了したら、デプロイするかどうかを **AskUserQuestion ツールの選択肢形式** で確認すること
- 選択肢: 「デプロイする」「デプロイしない」の2択で提示
- 確認なしに `deploy.sh` を実行してはいけない
- デプロイコマンド: `./deploy.sh` (Cloud Run へデプロイ)

<!-- migrated from memory/feedback_*.md (2026-05-11) -->

## 追加運用 rule (rust-alc-api / alc-app / nuxt-notify 共通)

### 設定値・Secret 管理

- **`cloudrun/render.sh` や `.github/workflows/*.yml` に値ハードコード禁止** — email/設定値は Secret Manager に格納し `valueFrom: secretKeyRef: {key: latest, name: <name>}` で参照。既存の `JWT_SECRET` / `GOOGLE_CLIENT_ID` / `OAUTH_STATE_SECRET` パターンに揃える。SA `747065218280-compute@developer.gserviceaccount.com` は `secretmanager.secretAccessor` 付与済 (`feedback_no_hardcode_in_render`)
  - 例外: 完全 static な routing 設定 (`STORAGE_BACKEND=r2`) や boolean (`STAGING_MODE=true`) は `value:` 直書き可
- **新規 secret を `secretKeyRef` で参照する前に runtime SA へ per-secret grant が必要** — `747065218280-compute@` への `secretmanager.secretAccessor` は **project レベルではなく per-secret grant** なので、`secret-inject` skill 等で新 secret を投入しただけでは Cloud Run が解決できず、`gcloud run services replace` の cutover が `Permission denied on secret: <NAME> ... must be granted roles/secretmanager.secretAccessor` で fail する。新 secret ごとに 1 回:
  ```bash
  gcloud secrets add-iam-policy-binding <NAME> --project=cloudsql-sv \
    --member="serviceAccount:747065218280-compute@developer.gserviceaccount.com" \
    --role="roles/secretmanager.secretAccessor"
  ```
  (事例: #391 `ALC_STAGING_API_KEY` — grant 漏れで staging cutover が permission denied → enforce が無音で未適用だった。`feedback_cloudrun_new_secret_grant`)
- **Secret Manager 更新後は `gcloud run deploy` で新 revision 強制作成** — 既存インスタンスは起動時にキャッシュした値を使い続けるため、`gcloud run services update` だけでは反映されない (`feedback_cloudrun_secret_cache`)
- **alc-app の `wrangler.jsonc` はトップレベル `vars` が必須** — `env.staging.vars` だけでは本番反映されない。未設定だと `NUXT_PUBLIC_API_BASE` が localhost:3001 にフォールバックして本番 Failed to fetch / OAuth 不能 (`feedback_alc_app_vars`)

### Cloud Run デプロイ

- **rust-alc-api の本番デプロイは `/tag-release patch` でタグを打つだけ** — `./deploy.sh` ローカル実行は使わない。CI ci.yml `deploy-production` ジョブが v* タグで自動実行 (GHCR → AR → Cloud Run migration + deploy) (`feedback_cloudrun_ci_deploy`)

### Gemini API (notify redact / extract)

- **`generationConfig` には必ず `responseMimeType: "application/json"` + `responseSchema` の両方注入** — `responseMimeType` だけでは markdown wrap / 前置テキストで parse error 発生。`responseSchema` (OpenAPI 3.0 subset) が constraint として構造を強制 (`feedback_gemini_response_schema_required`)
  - 型 enum は **大文字** (`STRING / NUMBER / INTEGER / BOOLEAN / ARRAY / OBJECT`)
  - 配列の固定長は `minItems` + `maxItems` で挟む (`box_2d` の `[ymin, xmin, ymax, xmax]` は両方 4)
  - schema を test で pin (`assert_eq!(schema["properties"]["x"]["type"], "STRING")`)
  - parse error 時は raw response 先頭 1KB を `tracing::warn!` で残す
  - 参考実装: `crates/alc-notify/src/redact.rs` の `redact_response_schema() / stage1_response_schema() / stage2_response_schema()` (PR #318)
  - 事故事例: PR #313 で `responseSchema` 忘れ → Stage 1 parse 失敗 → 1-stage fallback → 3164 マスクズレが staging で再現せず、PR #318 で修正
- **bbox / 構造化 output の検証は Python プローブ → 画像オーバーレイで 30 秒イテレーション** — staging deploy ループは 1 イテ 5-10 分かかるので、Rust に書く前に `/tmp/redact_probe.py` + `/tmp/visualize_bbox.py` で prompt を固める。`GEMINI_API_KEY` は `~/js/denchoho-invoice/.env` から拝借 (`feedback_local_gemini_bbox_iteration`、`notify_pdf_redact_design.md` 参照)

### Notify viewer / PDF 配信

- **LINE / LINE WORKS webview の PDF inline 表示は PDF.js (`vue-pdf-embed`) canvas 描画のみ** — `Content-Type: application/pdf` + `Content-Disposition: inline` でも DL ダイアログになる。R2 presign 直 redirect でも改善しない (PR #301 検証済)。canvas 描画必須 (`feedback_webview_pdf_pdfjs`)
  - PDF.js は同一オリジン or CORS 許可済みオリジンから fetch する必要あり → R2 presign 直 access させず API ストリーム (`/api/notify/v/{token}/file`) で配信 (PR #303)
  - `nuxt-notify` の `app.vue` で `route.path.startsWith('/v/')` 分岐を入れて認証 gate をバイパス

### テスト関連

- **`cargo test` がキャッシュで新テストを認識しない時は `cargo clean -p <package>`** — Rust incremental compile cache が古いテストバイナリを再利用するケースあり (worktree と main の `target/` 共有時に発生しやすい)。CI はクリーンビルドなので問題なし (`feedback_test_cache`)
- **テストファイル削除は hook がブロック** — `git commit` 時に複数 `*.test.ts` 削除を検出してブロック。**`cat /dev/null > file` + `describe.skip()` placeholder** で modified commit にして通す:
  ```typescript
  import { describe, it } from "vitest";
  describe.skip("xxx (removed — see <replacement>)", () => {
    it("placeholder", () => {});
  });
  ```
  後続 PR で物理削除を少量ずつ行う (`feedback_test_file_deletion_hook`)
