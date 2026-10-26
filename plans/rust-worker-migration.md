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
| W8 | notify の残り (documents・emails・ingest・viewer・LINE webhook) | alc-notify-worker 拡張 + **redact/rasterize は Containers** | L | pdfium は wasm 不可。Cloudflare Containers に redact だけ置き worker から呼ぶ (#486 Phase 2・#476)。viewer の token を URL パスに載せる問題 (#648) を移行と同時に直す |
| W9 | 撤収 | — | M | archive Job を Cron + R2 か Container へ → Cloud Run 停止 → contract migration (`bot_configs.private_key_encrypted` 等) → secret 退役 (#494・#503・#502) → rust-alc-api を archive |

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
  3. W8 の redact を Containers にするか、小さい Cloud Run を残すか
  4. W9 の archive Job の置き場

## 4. 次の一歩

- W0: 測定の口 7 本を削除 (#757、本番で読み書きとも確認済み)。#747 の rust 側は済み (残る `internal/lineworks/event` は W8)。`hub/measurements` の置き場は未決
- W1: camera を削除 (この PR)
- W2a: ログインの内部口 `/internal/auth/*` 8 本を削除 (本番は 10/13 から alc-auth-worker が受けている)
- W2b: 本番の me・logout・my-orgs を auth-worker の `ALC_AUTH_SESSION=1` で alc-auth-worker に切り替え済み (auth-worker v0.2.328)。monolith の 3 本の削除は様子見の後
- W2c: admin 系の分け方を決定。**消す**: API トークン (検証する実装がどこにも無い)・メンバー (`/admin/users` と重複、60 日 0 件) — この PR。**alc-auth-worker へ**: ユーザー・招待、SSO 設定、参加の申請。**alc-lineworks-worker へ**: Bot 設定
- W2c: ユーザー・招待・SSO 設定・参加の申請は alc-auth-worker (v0.2.4)、Bot 設定は alc-lineworks-worker (v0.1.5) に移し、本番で切り替え済み (auth-worker v0.2.329・v0.2.330)。rust 側の削除は W2b の 3 本と一緒に (10/26–27 以降)
- W3: carins (呼び手の無い 7 本を除く) と maintenance を新 ippoan/alc-carins-worker (v0.0.1) に移し、本番で切り替え済み (auth-worker v0.2.331、smb の取り込みも RPC で worker へ)。読み取りは本番で確認済み。smb の取り込みと書き込みの確認・rust 側の削除は後で
- W4a: `staging/export・import` を削除 (#761、auth-worker v0.2.332)
- W4b: upload 3 口を alc-measurements-worker (v0.1.12) に移し、本番で切り替え済み (auth-worker v0.2.333)。dev 端末の測定で `face-photo`・`blow-video` の書き込みと読み返しを確認。`report-audio` は W7 で確かめる。rust 側の 3 口はこの PR で削除
- W4c: items・carrying・communication・guidance を新 alc-records-worker (v0.0.1) に移し、本番で切り替え済み (auth-worker v0.2.336)。本番で 4 つの一覧の読み取りを確認 (worker のログで 200)。書き込みとファイルの上げ下ろしは未確認。rust 側の口はこの PR で削除 (先に alc-app#532・nuxt-items#60 で live の検証を外した)
- W4d は 3 段に分ける (2026-10-26 に Cloud Run の 7 日の件数と呼び手を調べて決めた): W4d-0 使われていない `POST /api/timecard/punch` を消す (この PR。打刻は alc-app → cf-alc-recorder → `/api/hub/measurements` で入る) / W4d-a timecard (カード・打刻一覧・CSV) → 新 alc-people-worker (振り分けは browser・device・rpc の 3 種。カードの照合 `resolve_employee_by_card` は hub_measurements 用に monolith に残す) / W4d-b employees → 同じ worker (`bulk-by-code` が保留カードを同じ transaction で結ぶので 1 つの worker に置く)
- W4d-a: timecard を新 alc-people-worker (v0.0.1) に移し、本番で切り替え済み (auth-worker v0.2.337、画面用・端末用・RPC の 3 経路)。本番で台帳・打刻の一覧・CSV の読み取りと端末の読み込みが worker に届くこと、切り替え後の Cloud Run への auth-worker 経由の要求が 0 件なことを確認。RPC (カード同期) と書き込みは未確認。rust 側の口はこの PR で削除 (カードの照合 `resolve_employee_by_card` は hub_measurements 用に残す。先に alc-app#533 で live の検証を外した)
- 次は W4d-b (employees → alc-people-worker)
rust-alc-api は open PR 1 本までなので、W1 以降の worker 側の PR (別 repo) と並行して進める。
