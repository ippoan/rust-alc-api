# rust-alc-api → Rust Worker 移行計画 (2026-10-25 時点)

Refs #680 (段階移行の親) / #697 (運用体制) / #725 / #747

## 0. 現状

### 分割済み (本番は auth-worker の `ALC_BINDING_ROUTES` → Service Binding)

| worker (repo) | 持っている口 | 最新タグ |
|---|---|---|
| alc-vein-worker | `/api/vein/*` | v0.1.10 |
| alc-dtako-worker | `/api/upload`・`split-csv*`・`uploads`・`internal/{pending,download,rerun}`・`recalculate*` | v0.3.6 |
| alc-notify-worker | `notify/{recipients,groups,lineworks/channels,line-config,lineworks/users,login-activity,test-distribute,employees}`・documents の distribute | v0.1.6 |
| alc-lineworks-worker | `internal/lineworks/send`・`lineworks/deploy-check` | v0.1.4 |
| alc-trouble-worker | `/api/trouble/*`・trouble の internal 2 本 | v0.1.2 |
| alc-measurements-worker | `/api/measurements*`・`bp-notify/*` | v0.1.11 |
| alc-auth-worker | ログインの内部口・機械用資格情報 | v0.2.3 |
| (他) rust-leave / rust-fuel-worker / alc-cam-sd-ingest | `leave/*`・`fuel/*`・`cam-clips/*` | — |

土台はできている: worker ごとの repo、`alc-worker-kit` (`alc-worker-db` の `tenant_tx` + `query_typed`)、
共有 crate の移設 (#736 済み)、共有の Hyperdrive 設定、alc-migrations (#754 で切替済み)、
worker の Tag Release (手動 = 本番 / merge = staging)。

### monolith に残っている口 (origin/main `ead565d`)

| crate | 行数 | 残りの口 | 外部依存・難所 |
|---|---|---|---|
| alc-tenko | 7.0k | `tenko/*` (sessions・records・schedules・dashboard・webhooks)・`tenko-call/*` | 60 秒ループの予定超過チェック (`src/main.rs:364`)、webhook 配信、`device_dev` |
| alc-misc | 8.4k | admin (users・sso・bot・members・api-tokens・access-requests)・employees (+face/nfc)・timecard・items/item-files・carrying/communication/guidance・upload (blow-video/face-photo/report-audio)・staging export/import・health | `bot_admin` の private key (#747)。**`measurements/start`・`{id}/face-photo`・`{id}/video` は worker 側に振り分け済みで死蔵** |
| alc-devices | 4.1k | `devices/*` (登録・承認・FCM・OTA・設定)・`hub/measurements` | FCM (GCP SA)、device_pair_client |
| alc-dtako | 9.9k | operations・restraint-report (+PDF・CSV 比較)・tickets・events・work-times・daily-hours・vehicles・settings dumps・event-classifications・dvr・y-time・csv proxy | `alc-pdf` (printpdf、lab で unknown 上で動作確認済み)、email-receiver からの internal ingest |
| alc-notify | 8.4k | documents (一覧・upload・search)・emails・ingest・viewer (`/notify/v/{token}`)・LINE webhook・`internal/lineworks/event` | **pdfium (FFI、wasm 不可)** の redact / rasterize、Gemini |
| alc-carins + alc-maintenance | 4.5k | car-inspections・files・nfc-tags・maintenance | R2/GCS (alc-storage) |
| alc-auth | 0.5k | `auth/me`・`logout`・`my-orgs`・`internal/auth/*` | — |
| alc-core | 8.8k | `/i` `/public` `/t` `/u` (短縮・公開口)、共通基盤 | `realtime_bus`、session RLS |
| src/bin/archive | — | アーカイブ Job | Cloud Run Job |

## 1. 原則 (全 wave 共通)

1. **monolith に新機能を足さない。** 残すドメインでも、新しい口は移行先 worker に書く (#755 / #756 の `hub/measurements` は本来 measurements 側)。
2. **strangler**: worker を staging で動かす → 本番に出す → auth-worker の表に 1 行 → Release Wave の flip を複数標本で確認 → monolith の口を消す PR。消すのは常に最後。
3. **DB は同じ 1 つを共有。** worker 同士の Service Binding 呼び出しは「副作用 (送信・通知) を持つ相手」に限り、参照は各 worker が自分で SQL を引く (employees などの共有表)。表の所有者を worker ごとに決め、他の worker は読むだけ。
4. migration は alc-migrations。消す・名前変更は全 worker が移った後の contract。
5. 1 worker = 1 repo、`alc-worker-db` は git rev 固定で 1 か所、公開範囲の検査と陰性対照、Hyperdrive はトップレベルだけ。
6. 外向き HTTP の URL は struct フィールド (wiremock)、Gemini は `responseMimeType` + `responseSchema`、unit test は本番を叩かない — monolith の規範はそのまま worker に持ち込む。

### 1 worker の切り替えチェックリスト

- [ ] repo 作成 (既存 worker の雛形を複写: `deploy.yml`・`tag-release`・`check-exposure*.sh`・staging DB の container・`ALC_MIGRATIONS_REV`)
- [ ] route crate + 実 DB テスト (実行用ロール、`tenant_tx`、並列でテナントが漏れないこと)
- [ ] staging で消費側 (画面・端末) から通す
- [ ] 本番タグ → auth-worker の表 + `[[services]]` (本番だけ) → flip を複数標本で確認
- [ ] monolith の口・テスト・BUILD.bazel の target・coverage_100.toml の行・map skill を消す PR
- [ ] 不要になった env / secret を render.sh から外す

## 2. Wave (順序と理由)

| # | 対象 | 移行先 | 規模 | 前提・論点 |
|---|---|---|---|---|
| W0 | **掃除**: 死蔵の measurements 3 口・`hub/measurements` の置き場決め・#747 の残り (`internal/lineworks/event`・`bot_admin` の private key 保存) | 既存 worker | S | 先にやると以降の差分が小さくなる |
| W1 | `cameras*` (監視カメラの死活監視、#345) | **移さず削除** | S | 60 日間 health-logs の書き込み 0 件 (書き手の ippoan/alc-app#27 が未実装)。台帳は alc-cam-sd-ingest の `/api/cam-clips/cameras` が持つ |
| W2 | `auth/me`・`logout`・`my-orgs`・`internal/auth/*`、misc の admin 系 (users・sso・members・api-tokens・access-requests・tenants/by-slug) | alc-auth-worker | M | 認証まわりを 1 か所に。`api-tokens` の発行は鍵の扱いを確認 |
| W3 | carins + maintenance | 新 `alc-carins-worker` | M | ストレージは R2 binding に寄せる (GCS が残っていれば先に R2 へ) |
| W4 | misc の残り: items / item-files / carrying / communication / guidance / employees (+face/nfc) / timecard / upload 3 口 | 4 段に分ける: W4a 消す (`staging/export・import`) / W4b upload 3 口 → alc-measurements / W4c items・carrying・communication・guidance → 新 `alc-records-worker` / W4d employees・timecard → 新 `alc-people-worker` | L | employees は多くの worker が読む共有表 → 所有者を W4d の worker に決める。`health/secret-fingerprint` は PR の CI の Drift Check (staging) が使うので W9 まで残す |
| W5 | dtako の残り | alc-dtako-worker を拡張 | L | 拘束時間 PDF は `alc-pdf` を wasm で (lab で printpdf 確認済み、#486 Phase 1)。email-receiver の internal ingest は Service Binding へ |
| W6 | devices | 新 `alc-devices-worker` | L | FCM は SA 鍵で RS256 を自前署名 (`rsa` crate は unknown で動作確認済み)。OTA・pair の外部呼び出しの置き換え |
| W7 | tenko + tenko-call + core の公開口 (`/i` `/t` `/u` `/public`) | 新 `alc-tenko-worker` | XL | **切り替えの確かめに、dev 端末の業務後点呼で運行報告の音声 (`POST /api/upload/report-audio`、W4b で alc-measurements に移した口) も通す** (W4b では測定だけ確かめた)。予定超過の 60 秒ループ → Cron Trigger (1 分)。webhook 配信 → `wait_until` か Queues (fire-and-forget 禁止の規範を引き継ぐ)。`realtime_bus` の代替 (DO)。`device_dev` の扱いを `alc-core-wasm` 側へ。業務の中心なので最後に、他の wave で型を固めてから |
| W8 | notify の残り (documents・emails・ingest・viewer・LINE webhook) | alc-notify-worker 拡張 + **黒塗りは GCP の別 repo** (入力 PDF・出力 黒塗り済み JPEG の 1 口、Gemini の枠の検出も GCP 側) | L | 4 段に分ける: W8a 閲覧の口を消す (閲覧は nuxt-notify の Worker が KV・R2 から配信済み) / W8b LINE・LINE WORKS の受け口 → alc-notify-worker / W8c 文書・メール・取り込み・Gemini → worker (取り込みは email-receiver から Service Binding) / W8d 黒塗り (pdfium、wasm 不可) → GCP の別 repo。viewer の token を URL パスに載せる問題 (#648) は W8a で口ごと消える |
| W9 | 撤収 | — | M | archive Job を Cron + R2 か Container へ → Cloud Run 停止 → contract migration (`bot_configs.private_key_encrypted` 等) → secret 退役 (#494・#503・#502) → rust-alc-api を archive。Cloud Run を止めるときは、使われていない Cloud Run `rust-logi` (古い平文の env が残る) も一緒に消す (2026-10-26 のオーナー判断)。cloudsql-sv は組織配下に移り、端末の gcloud は閲覧だけなので、Cloud Run の削除・変更は PAM の `run-admin` を申請し、オーナーの承認を待ってから行う |

依存: W0 → (W1, W2, W3 は並行可) → W4 → (W5, W6 並行可) → W7 → W8 → W9。
W4 を W5〜W7 より前に置くのは、employees / timecard を他の wave が読むため所有者を先に決めたいから。

## 3. 横断の先行課題

- **worker 雛形の共通化**: worker が 7 本を超えるので、`deploy.yml`・tag-release・公開範囲の検査・staging DB の container を複写でなく共有する (ci-workflows の reusable workflow 化、または kit に template)。W3 の前にやる。
- **版の可視化**: ci-dashboard に worker ごとの本番タグ・git SHA・`ALC_MIGRATIONS_REV` を集める (#697 の未実装分)。
- **`alc-core-wasm` に寄せるもの**: `device_dev`・`api_error`・`master_data` 等、W4・W7 で worker から使うものを順に移す (sqlx / reqwest 非依存に)。
- **monolith の CI を縮める**: wave ごとに crate を消すので、BUILD.bazel・coverage gate・shard (#542) を毎回追従させる。消した crate の gate の行が残ると CI が落ちる。
- **決めること (オーナー判断)**:
  1. W2 の admin 系を alc-auth-worker に入れるか、別 worker にするか
  2. W4 の misc を 1 本にするか (最終形 8 本の「misc」)、employees/timecard を分けるか
  3. ~~W8 の redact を Containers にするか、小さい Cloud Run を残すか~~ → **W8 の黒塗り (pdfium) は GCP に別 repo** (2026-10-27 のオーナー判断。Containers は使わない。notify のほかの口は worker へ)
  4. W9 の archive Job の置き場

## 4. 次の一歩

- W0: 測定の口 7 本を削除 (#757、本番で読み書きとも確認済み)。#747 の rust 側は済み (残る `internal/lineworks/event` は W8)。`hub/measurements` の置き場は alc-people-worker に決めた (2026-10-27、W6a)
- W1: camera を削除 (この PR)
- W2a: ログインの内部口 `/internal/auth/*` 8 本を削除 (本番は 10/13 から alc-auth-worker が受けている)
- W2b: 本番の me・logout・my-orgs を auth-worker の `ALC_AUTH_SESSION=1` で alc-auth-worker に切り替え済み (auth-worker v0.2.328)。monolith の 3 本の削除は様子見の後
- W2c: admin 系の分け方を決定。**消す**: API トークン (検証する実装がどこにも無い)・メンバー (`/admin/users` と重複、60 日 0 件) — この PR。**alc-auth-worker へ**: ユーザー・招待、SSO 設定、参加の申請。**alc-lineworks-worker へ**: Bot 設定
- W2c: ユーザー・招待・SSO 設定・参加の申請は alc-auth-worker (v0.2.4)、Bot 設定は alc-lineworks-worker (v0.1.5) に移し、本番で切り替え済み (auth-worker v0.2.329・v0.2.330)。rust 側の削除は W2b の 3 本と一緒に (10/26–27 以降)
- W2b・W2c の rust 側の削除: alc-auth crate (me・logout・my-orgs) と alc-misc の sso_admin・tenant_users・access_requests (`tenants/by-slug` を含む)・Bot 設定の一覧・保存・削除を消した。Cloud Run への要求は切り替え (2026-10-25 13:25・15:22・15:59 UTC) の後 0 件。Bot 設定の export (開発者だけ。auth-worker の `handleBotConfigExport` が Cloud Run を呼ぶ) は残す。auth-worker の my-orgs は `ALC_AUTH` が無いときだけ Cloud Run に落ちる作りなので、消した後その落ち先は 404 になる
- W3: carins (呼び手の無い 7 本を除く) と maintenance を新 ippoan/alc-carins-worker (v0.0.1) に移し、本番で切り替え済み (auth-worker v0.2.331、smb の取り込みも RPC で worker へ)。読み取りは本番で確認済み。smb の取り込みと書き込みの確認は後で。rust 側は W3 の削除で消した (下の行)
- W4a: `staging/export・import` を削除 (#761、auth-worker v0.2.332)
- W4b: upload 3 口を alc-measurements-worker (v0.1.12) に移し、本番で切り替え済み (auth-worker v0.2.333)。dev 端末の測定で `face-photo`・`blow-video` の書き込みと読み返しを確認。`report-audio` は W7 で確かめる。rust 側の 3 口はこの PR で削除
- W4c: items・carrying・communication・guidance を新 alc-records-worker (v0.0.1) に移し、本番で切り替え済み (auth-worker v0.2.336)。本番で 4 つの一覧の読み取りを確認 (worker のログで 200)。書き込みとファイルの上げ下ろしは未確認。rust 側の口はこの PR で削除 (先に alc-app#532・nuxt-items#60 で live の検証を外した)
- W4d は 3 段に分ける (2026-10-26 に Cloud Run の 7 日の件数と呼び手を調べて決めた): W4d-0 使われていない `POST /api/timecard/punch` を消す (この PR。打刻は alc-app → cf-alc-recorder → `/api/hub/measurements` で入る) / W4d-a timecard (カード・打刻一覧・CSV) → 新 alc-people-worker (振り分けは browser・device・rpc の 3 種。カードの照合 `resolve_employee_by_card` は hub_measurements 用に monolith に残す) / W4d-b employees → 同じ worker (`bulk-by-code` が保留カードを同じ transaction で結ぶので 1 つの worker に置く)
- W4d-a: timecard を新 alc-people-worker (v0.0.1) に移し、本番で切り替え済み (auth-worker v0.2.337、画面用・端末用・RPC の 3 経路)。本番で台帳・打刻の一覧・CSV の読み取りと端末の読み込みが worker に届くこと、切り替え後の Cloud Run への auth-worker 経由の要求が 0 件なことを確認。RPC (カード同期) と書き込みは未確認。rust 側の口はこの PR で削除 (カードの照合 `resolve_employee_by_card` は hub_measurements 用に残す。先に alc-app#533 で live の検証を外した)
- W4d-b: employees を alc-people-worker (v0.0.2) に移し、本番で切り替え済み (auth-worker v0.2.338、画面用・端末用・RPC の 3 経路)。本番で一覧・1 件・社員番号・顔データの読み取りが worker に届くこと、切り替え後の Cloud Run への社員の要求が 0 件なことを確認。端末・RPC (乗務員マスタ同期) と書き込みは未確認。rust 側の口はこの PR で削除 (先に alc-app#534 で live の検証を外した。テストの社員は SQL で入れる)
- W5・W6 は 6 段に分ける (2026-10-26 に Cloud Run の 30 日の件数と呼び手を調べて決めた): W5-0 30 日 0 件の口の呼び手を調べて、使われていない口を消す / W5a 機械の取り込み (`dtako-logs/bulk`・`dvr/*`) → alc-dtako-worker / W5b 勤怠向けの読み取り (`dtako/events`・`etags`) → alc-dtako-worker / W5c 画面の読み取りと分類の更新・拘束時間の PDF → alc-dtako-worker / W6a `hub/measurements` (カードの照合 `resolve_employee_by_card` ごと) / W6b devices の管理・FCM・登録・OTA → 新 alc-devices-worker
- W5-0: 30 日 0 件の 37 口 (dtako 15・devices 22) のうち、呼び手が無いのは `vehicle-settings-dumps` の 3 口だけだったので消した (この PR。表は残す)。ほかは nuxt_dtako_logs・nuxt-dtako-admin・daiun-salary・email-receiver・alc-app 等が呼ぶ (頻度が低いだけ)
- W5a: 機械の取り込み (`dtako-logs/bulk`・`dvr/notifications`・`dvr/files/{id}`) を alc-dtako-worker (v0.3.7) に移し、本番で切り替え済み (auth-worker v0.2.339。bulk は端末用・RPC、dvr は内部用)。本番で 13:20 UTC の回までが Cloud Run、13:30 からが worker で 200、書いた車両位置が読み取りの口から見えることを確認。`dvr/files` と端末用の bulk は本番の要求がまだ来ていない。rust 側の 3 口はこの PR で削除 (live の検証は叩いていなかった)
- W5b: 勤怠向けの読み取り (`dtako/events`・`dtako/events/etags`) を alc-dtako-worker (v0.3.8) に移し、本番で切り替え済み (auth-worker v0.2.340。events は RPC、etags は RPC・画面用・端末用)。etags の ETag は切り替えの前後で一致 (265 件、先頭 25 件の運行NO と ETag が同じ)、切り替え後の Cloud Run への `dtako/events*` の要求は 0 件。rust 側の 2 口と、それ専用の列挙 3 つ (`list_drivers_with_operations`・`list_operations_for_drivers`・`list_unsplit_operations`) はこの PR で削除 (live の検証は 404 を「届いた」と数えるので外さなくてよかった)
- W5c は 5 段に分ける (2026-10-26。残る dtako 約 30 口・7,600 行): W5c-1 一覧系の薄い読み取り 20 口 / W5c-2 Y時間 export / W5c-3 拘束時間の帳票 / W5c-4 拘束時間の PDF (`alc-pdf` を wasm で) / W5c-5 tickets
- W5c-1: 画面向けの読み取りと分類の更新 20 口 (`drivers`・`vehicles`・`work-times`・`daily-hours/*`・`operations/*`・`internal/operations`・`dtako/operation-changes`・`event-classifications/*`・`scraper/history`・`dtako-logs` の読み取り 4 口) を alc-dtako-worker (v0.3.9) に移し、本番で切り替え済み (auth-worker v0.2.341。画面の口は画面用、`internal/operations` は内部用、`scraper/history` は画面用・RPC・端末用)。本番で読み取り 13 口が worker に届いて 200 なことを確認。書き込み (削除・分類の更新・履歴の記録)・内部用・RPC は本番の要求でまだ確かめていない。`dtako-logs/by-date-range` が 8.4 秒かかる (保存先のアーカイブの目録を毎回読む。要調査)。rust 側の 20 口と repo・型はこの PR で削除 (先に alc-app#535・nuxt_dtako_logs#56・nuxt-dtako-admin#1241 で live の検証を外した)
- W5c-2: Y時間 export の 2 口 (`GET /api/dtako/y-time-export`・`POST /api/dtako/y-time-rows`) を alc-dtako-worker (v0.3.10) に移し、本番で切り替え済み (auth-worker v0.2.343。画面用だけ)。rust から口・repo・trait・mock・期間条件の実 DB テスト (`dtako_operation_window_test`) を消した。worker の `repo.rs` は rust と同じ文だが、読取日 OR 運行日・±1 日の境界を縛る実 DB テストは worker にまだ無い (足すのは worker の repo)
- W5c-3: 拘束時間の帳票の画面の 2 口 (`GET /api/restraint-report`・`POST /api/restraint-report/compare-csv`) を alc-dtako-worker (v0.3.11) に移し、本番で切り替え済み (auth-worker の振り分けは画面用だけ。PDF は回さない)。rust から口・CSV 比較・trait の `get_driver_name`・`list_drivers_with_cd`・alc-dtako の `alc-compare` 依存を消し、組み立て (`build_report_with_name`) は PDF のために残した (mock テストは組み立てを直接呼ぶ形に)
- W5c-4: 拘束時間の PDF の 2 口 (`GET /api/restraint-report/pdf`・`pdf-stream`) を alc-dtako-worker (v0.3.12。`alc-pdf` とフォントも worker の repo へ移した。wasm は約 20 MB) に移し、本番で切り替え済み (auth-worker v0.2.348。画面用だけ)。rust の PDF は乗務員の SQL の列名 `name` と型の欄 `driver_name` が食い違い、本番で常に 500 だった (worker では列を位置で読み、実 DB のテストで通す)。本番で 1 人ぶんの PDF が worker から 200 (CPU 約 450 ms)。全員ぶん (`pdf-stream`) は、社員の全員 (約 400 人) を 1 ファイルにすると CPU 30 秒の上限で止まり、上限を 5 分にした (v0.3.13) 後はメモリ 128 MB の上限で止まった (CPU 98 秒の時点)。対象を「その月に運行のある乗務員」に絞った (v0.3.14、2026 年 9 月で 110 人)。最後まで出るかは未確認 (rust でも常に 500 だった)。rust から PDF の口・組み立て・trait・repo・mock・`alc-pdf` crate・直下の `assets/fonts` を消した
- W5c-5: 車両のエラーの起票の 5 口 (`POST /api/dtako/tickets`・`PATCH /api/dtako/tickets/{id}/scraped` = email-receiver の内部用、`GET /api/dtako/tickets`・`GET /api/dtako/tickets/{id}`・`POST /api/dtako/tickets/close` = 画面用) を alc-dtako-worker (v0.3.15) に移し、本番で切り替え済み (auth-worker v0.2.349。id は UUID に固定)。本番で一覧と 1 件が worker から返り、Cloud Run への `dtako/tickets` の要求は切り替え後 0 件。close_token は worker では DB が作る (長さと字種は同じ)。rust から口・trait・repo・型・mock と、これで空になった `alc-dtako` crate・`AppState` の `dtako_storage`・`tests/mock_dtako` を消し、crate の再作成を `check_domain_split.sh` が落とすようにした。**dtako の口はこれで全部 worker に移った**
- W5c-5 の補足: auth-worker の振り分けの表に proxy ごとの method の絞り (`methods`) を足し、`/api/dtako/tickets` は画面用 GET・内部用 POST だけを worker に渡す (auth-worker v0.2.350)。それまで画面からの起票の POST も worker に届いていた (rust では画面の経路は GET だけ)
- W6a: 端末の測定・打刻の取り込みと一覧 (`POST`・`GET /api/hub/measurements`) を alc-people-worker (v0.0.3) に移し、本番で切り替え済み (auth-worker v0.2.351。取り込みは内部用の POST、一覧は画面用・RPC の GET。内部用の binding の経路でも dev端末の印を渡すようにした)。打刻はカードを社員に解決して payload に凍結する (カードの照会と同じ SQL)。本番で一覧が worker から返り、切り替え後の Cloud Run への `hub/measurements` の要求は 0 件。取り込みの POST は、本番の端末に開発用の試験記録を作らせて (MCP `create_test_record`) recorder → 内部用の経路 → worker で入ること (dev の軸、`is_dev = true`) と RPC の読み返しを確認。切り替え後の Cloud Run への `hub/measurements` の要求は 0 件。本番の測定・打刻 (本番の軸) の取り込みは次の業務時間に通る。rust から口・repo・trait・型・mock・実 DB テストと、打刻の照合のためだけに残していた timecard (`TimecardRepository`・`resolve_employee_by_card`)、空になった共有の秘密の経路 (`internal_shared_secret_router`・`require_internal_shared_secret`) を消し、再流入を `check_domain_split.sh` が落とす
- W6b: devices を新 ippoan/alc-devices-worker に移した (3 段、2026-10-27 のオーナー判断)。W6b-a 登録・管理・報告の 21 口と auth-worker が使う `pairing-tenant` (v0.1.0、auth-worker v0.2.352) / W6b-b kiosk の再ペアリング (v0.1.1。発行は auth-worker の新しい入口 `DevicePairEntrypoint` (v0.2.353) を Service Binding で呼び、worker は共有の秘密を持たない。振り分けは v0.2.354) / W6b-c FCM で端末に送る口と着信の設定 (v0.1.2。**FCM の鍵は既存の送信専用の鍵 `CHIP_REMOTE_FCM_SA_KEY` を Secrets Store から共有** = オーナー判断。振り分けは auth-worker #702)。本番で一覧・承認待ちが worker から返ることを確認。端末の口・再ペアリング・FCM は 30 日の件数がほぼ 0 で、本番の要求ではまだ確かめていない。rust から alc-devices crate ごと消した (この PR。先に alc-app#537 で live の検証を外した)。移さずに消した 5 口と、残る別件: 共有の秘密 `FCM_INTERNAL_SECRET` が本番に無く `fcm-notify-call`・`test-fcm-all-exclude`・`trigger-update-dev` は今も使えない (前の 2 つは cf-alc-signaling が Cloud Run を直接呼ぶ。`fcm-notify-call` は 30 日 14 件がすべて 403、消すと 404) / 端末の登録の申請・状態 (`register/request`・`register/status`) は auth-worker の内部用の許可リストに無く本番で 403 (QR の一時フロー)
- W3 の rust 側の削除: alc-carins・alc-maintenance crate を丸ごと消した (`AppState` の carins の欄・`MaintenanceState`・alc-core の carins の trait と `lookup_expiry`・mock・テスト・Bazel・CI の行)。worker に回っていない口 (`/api/files/not-attached`・`/api/files/{uuid}` の取得・削除・復元・`/api/car-inspection-files/current`) は呼び手が無く 30 日 0 件なので一緒に消した。切り替え (2026-10-25 17:18 UTC) の後、Cloud Run への carins・maintenance の要求は 0 件。先に frontend の live の検証を外した (alc-app#538・nuxt-pwa-carins#72・nuxt-vehicle-maintenance#13。carins と maintenance の結合テストは rust の `/api/health` だけになった)。`CARINS_R2_*` の env は render.sh / deploy.sh に残る
- W7 の決定 (2026-10-27 のオーナー判断): 4 段に分ける。W7-0 使われていない口の整理 / W7a 新 `alc-tenko-worker` と読み取り・管理 (記録・ダッシュボード・予定・基準値・機器故障・日々の健康・乗務員情報・webhook の設定・点呼の電話) / W7b 点呼のセッションと webhook の送信 (`ctx.wait_until`。dev 端末で業務前・業務後を通し、運行報告の音声も確かめる) / W7c 予定超過の確認 (60 秒ごとの `tokio::spawn` のループ) を Cron Trigger (毎分) へ。W7c は worker 側を「通知済みの印を先に付けた行だけ送る」形にし、rust のループを止める PR を続けて出す (重なる数分も二重にならない)。表の W7 の「core の公開口 (`/i` `/t` `/u`)」はテストの中だけの口で、本番の口ではない
- W7-0: 点呼の 39 口のうち Cloud Run で 30 日 0 件の口の呼び手を全 repo で調べた。呼び手の無い `GET /api/tenko/records/{id}` だけを消した (60 日も 0 件。一覧は画面が使う)。ほかの 0 件の口は、alc-app の画面 (点呼のセッションの各段・予定・基準値・機器故障・webhook) か、点呼の電話のアプリ → alc-app のサーバー → auth-worker の内部用 (`tenko-call/register`・`tenko`) に呼び手がある。1 件取得の 4 口 (機器故障・基準値・予定・webhook の `{id}` の GET) は alc-app の api.ts に関数があり画面からは使われていないが、同じ path の更新・削除は画面が使うので残す (W7a で写す)。`POST /api/tenko/sessions/{id}/judgment` (30 日 8 件) は route に在る (運行管理者の判定、`record_manager_judgment`)
- W7a: 点呼の読み取りと管理 (記録の一覧・CSV、予定、基準値、機器故障、webhook の設定と配信の記録、ダッシュボード、日々の健康状態、乗務員情報、点呼の電話) を新 ippoan/alc-tenko-worker (v0.1.0) に移し、本番で切り替え済み (auth-worker v0.2.358、22 行)。本番で記録・予定・ダッシュボードが worker から 200。点呼の電話の乗務員の一覧は rust が全テナントの行を返していたのを worker はテナントで絞り、確かめたテナントでは空 — 点呼の電話のアプリは今は使っていないので確認は飛ばした (ユーザー判断)
- W7b: 点呼のセッション 16 口 (一覧・1 件・開始と各段・中止・中断・再開・判定) と webhook の送信を alc-tenko-worker (v0.1.1) に移し、本番で切り替え済み (auth-worker v0.2.359)。webhook は応答の後に `ctx.wait_until` で送る (打ち切り 7 秒 × 3 回 + 待ちで最長 27 秒)。切り替え後、Cloud Run に点呼の要求は 0 件。本番の通常点呼は alc-measurements-worker が測定の確定で記録を書く (段の口を通らない)。段の口を使うのは自動点呼・遠隔点呼・IT点呼の判定・管理画面のセッションの監視で、どれも今は使っていないので本番では確かめていない
- W7c: 予定超過の確認 (60 秒ごとの `tokio::spawn` のループ) を alc-tenko-worker の Cron Trigger (毎分、本番だけ) に移した。worker は通知済みの印を先に付ける UPDATE が返した予定だけ送る (2026-10-27 のオーナー判断「印を先に付ける形」。rust のループと重なって動いても二重に送らない)。rust のループ・`overdue.rs`・`repo/overdue.rs`・テストはこの PR で削除
- W7d: rust から alc-tenko crate ごと消した (先に alc-app の api.test.ts で live の検証を外した。api.ts が叩く口は rust に 1 つも残らないので `describe('api')` ごと mock だけに)。使い手が無くなった点呼の webhook 一式 (alc-core の `webhook.rs`・`WebhookRepository`・`AppState.webhook`・`Webhook*` の model、alc-misc の `PgWebhookRepository`) も一緒に消した。点呼の DB テスト (`normal_tenko_record_test`・`tenko_bp_required_escalate_test`・`driver_info_maintenance_test`、`device_dev_axis_test` と `tenant_scoped_queries_test` の点呼の口を叩く分) は #778 と同じく丸ごと消した (表の制約の検査は alc-tenko-worker の実 DB テストが持つ)。crate の再作成は `check_domain_split.sh` が落とす
- W8a: rust の閲覧の口 (公開の `/api/notify/v/{token}`・`/file`・`/image.jpg`・`/image`、既読の `/api/notify/read/{token}`、内部用の `/api/internal/notify/view/{token}`・`/read`) を消した。先に nuxt-notify の閲覧画面から rust への fallback と live の検証 (`/api/notify/read`) を外した。閲覧は nuxt-notify の Worker (KV `NOTIFY_VIEW_KV`・R2)、配信の URL は alc-notify-worker が nuxt-notify を指して送る。消す前の 30 日の Cloud Run は `/notify/v`・`/notify/read` が数件ですべて 403。使い手が無くなった repo の `mark_read`・`get_for_view` と `ReadResult`・`DeliveryViewInfo` も消した (DB の関数 `mark_delivery_read`・`lookup_delivery_for_view` は W9 の contract で)。再流入は `check_domain_split.sh` が落とす
- W8b: LINE の webhook (公開の `/api/notify/line/webhook` と内部用の `/api/internal/notify/line/webhook`) と LINE WORKS の 2 口 (`/api/internal/lineworks/bot-secret/{bot_id}`・`/api/internal/lineworks/event`) を ippoan/alc-notify-worker へ移し、auth-worker の受け口 (`line-webhook.ts`・`LineworksWebhookDO`) を Service Binding `ALC_NOTIFY` に替えてから rust の口を消した。nuxt-notify の設定画面が表示する LINE の webhook URL も auth-worker の `/line/webhook` に直した。テナントが決まる前の読みは worker でも同じ SECURITY DEFINER 関数 (`list_enabled_line_configs`・`lookup_bot_config_for_webhook`) を nil のテナントの transaction で呼ぶ。関数が行を返すかは呼び手のロール (rust の alc_api_app / worker の alc_api_rt) ではなく関数の所有者 (と bot_configs の FORCE RLS) で決まり、rust と worker で差は無い (使い捨て DB で所有者を非 superuser にすると両方 0 件。テスト DB は所有者が superuser なので実 DB テストでは本番を再現できない。本番の所有者の確認は別に扱う)。使い手が無くなった LINE の client (`clients/line.rs`)・repo (`notify_recipients`・`lineworks_channels`)・AppState の欄・mock・`mock_lineworks_internal_test` も消した。`device_dev_axis_test` (標本の公開の口が無くなる) はテストごと消した。再流入は `check_domain_split.sh` が落とす
- **点呼の入口の一本化 (2026-10-28 のオーナー判断)**: 点呼の記録を書く入口は「測定の口」(alc-measurements-worker、通常点呼) と「段の口」(alc-tenko-worker、自動・遠隔・IT点呼) の 2 つあり、記録の規則が 2 か所にある。**worker 移行 (W8・W9) が終わってから、測定の口に一本化し、自動点呼・遠隔点呼・IT点呼をその上に載せ直す**。それまで段の口はつなぎ (手を入れない)
rust-alc-api は open PR 1 本までなので、W1 以降の worker 側の PR (別 repo) と並行して進める。
