# CI 速度改善 tracking

rust-alc-api の PR CI (~214s 実測 / 45 jobs) の速度改善の調査・施策・実測を追跡する doc。
発端は rust-ichibanboshi (72s / 3 jobs) との比較調査 (Refs #482)。

結論の先出し: **速度差は技術選定ミスではなくスコープの差** (7 マイクロサービスの
Bazel build + staging deploy を PR CI に含む運用)。改善は「同一ソースの重複ビルドの排除」
を中心に進める。

## 確定した事実 (2026-07-01 調査、Refs #482)

### Bazel remote cache は健全

- `Build backend` job (106s) 実測: `1832 processes: 1375 disk cache hit, 443 internal,
  14 processwrapper-sandbox` / `Elapsed 103.4s, Critical Path 36.1s`
- cache miss ではなく PR 差分 (alc-core 79 files 等) による正当な差分ビルド
- `gateway` job は `1 process: 1 internal` で完全 cache hit
- **Bazel の技術選定・cache 設定に問題はない**

### 同一ソースに対して 3 つの独立ビルドが走っていた

| # | job | ツール | profile | cache key |
|---|---|---|---|---|
| 1 | `check` (Format & Lint) | `cargo test export_bindings --workspace` | test/debug | rust-cache `check` |
| 2 | `test-matrix` ×7 | `cargo llvm-cov nextest` | coverage 計装 | rust-cache `test-shared` |
| 3 | `build-image` ×7 | `bazel build -c fastbuild` | fastbuild | bazel disk-cache |

プロファイルが違うため sccache / rust-cache のキャッシュキーは一切共有されず、
`alc-core` 等の変更のたびに実質フルリコンパイルが 3 回走る。

## 確定した事実 (2026-07-04/05 実測、Refs #506-#511)

### ベースライン (2026-07-04)

計測元: PR run [28698773673](https://github.com/ippoan/rust-alc-api/actions/runs/28698773673) / tag run [28701104111](https://github.com/ippoan/rust-alc-api/actions/runs/28701104111)

| run | 全体時間 | critical path |
|---|---|---|
| PR CI | 4分52秒 | runner queue 待ち (~50-80s) → Tests (~2分) → Coverage Check (11s) → staging deploy (60s) |
| tag CI | 7分01秒 | Tests matrix (~3分) → Promote 22s → migrations 62s → deploy×6 並列 60s → verify 38s → report 26s |

Tests (lib) job 122s の内訳: postgres service 起動 ~21s / setup (rustup + 795MB cache
restore) ~27s / cargo build 段 28.5s / nextest 実行 (677 tests) 38.2s / upload ~5s。

Build backend (Bazel) job 139s の内訳: setup ~8s / **analysis 60s**
(`Analyzed 3 targets (603 packages loaded, 23369 targets configured)`) / execution 41s
(1824 actions 中 1366 disk cache hit、実行 15) / docker ×2 ~20s。

### Bazel analysis 60s はダウンロードではなく CPU 処理

- repository-cache (266MB) が hit した run でも analysis は 60s で不変 (#507 run で実証)
- 正体は repo rule 実行 + 603 packages の loading + 23369 targets の analysis (Skyframe、CPU-bound)
- **actions/cache 系では削れない**。削るなら larger runner (CPU 並列) か依存グラフ削減のみ

### setup-bazel の external-cache は job 跨ぎ warm が構造的に効かない (#506/#508 で実証)

- manifest のキャッシュキーは `external-<workflow>-<job>-manifest` 形式で **job 名 namespace**
  → `cache-warm-bazel` job が保存した manifest は `build-image` job から見えない
- さらに PR run の cache scope は PR merge-ref 単位 → 前 PR の cache も次の PR から見えない
- 並列 matrix (7 job) の save は同一キーの reserve を取り合い全滅し得る (2 run 連続で確認)
- restore/save 試行のオーバーヘッドで Build backend が 139s → 201s に**悪化**したため revert (#508)

### cargo build 段 28s は「リンク」でも「コンパイル」でもない固定費

- sccache 100% hit (`Compile requests executed 14, Cache hits 14`) でも 28s かかる
- mold (ld 差し替え) で 28.46s → 27.93s = リンク支配ではない (#507 で実証)
- `build.rs` は `rerun-if-changed=migrations` のみで無害 (sha 焼き込みなし)
- 残る候補: cargo のフィンガープリント再検証 + 依存チェーン直列の sccache 復元 + rustc 起動。
  深掘りは `cargo build --timings` を CI で一度取るのが次の手

### 依存グラフの実態 (2026-07-05 Cargo.lock 実測)

- 572 packages / 53 crate が複数バージョン重複
- **rust-s3 0.35 が単独で hyper 0.14 / http 0.2 の旧 HTTP スタックを引き込んでいる**
  (workspace は reqwest 0.12 = hyper 1.x に統一済みなのに二重、~15-20 packages)。
  rust-s3 の hyper 1.x 対応版へ更新 or object_store 等へ移行が最大の単発削減
- 小物: lopdf 0.34/0.35 二重 (pdf-extract 0.7 経由)、rand 0.7 + getrandom 0.1 (phf 0.8 経由)
- 期待値: analysis はグラフサイズ比例なので 1 割減で 60s → ~54s 程度。ただし rust-cache
  795MB 縮小 / fingerprint 検証対象減 / コンパイル総量減と全レイヤーに薄く効く
- targets 数 (23369) は crate_universe が 1 crate に複数 target を生成するため。
  依存 crate 数にほぼ比例するので依存削減がそのままターゲット削減になる

## 確定した事実 (2026-07-05 調査、Refs #513 / #515)

### runner queue 遅延の真因 = org が GitHub Free plan (同時 20 job)

- run 28698773673 の実測: pr-limit 完了 (gate open) 後、即時に走れたのは 6-7 job のみ。
  残りは 27〜73s かけて滴下開始 (Tests mock-devices は 66s 待ち = critical path 直撃)
- gate open 時点で **同一ブランチの 2 分前 push の CI (~17 job) がまだ走行中**で、
  org 全体の需要は ~35 job。billing 画面で **ippoan org は GitHub Free ($0) = 上限 20 並列**
  を確認 → 需要 35 > 枠 20 が滴下の説明として整合
- 対策: org Team 化 ($4/user/月) で 60 並列。cancel-in-progress は「同一ブランチ連続 push」
  のみ救済 + deploy レース考慮が要るため優先度下げ (経緯は #513 の議論)
- 注: public repo のため標準 runner の分数は無料。job 統合による「課金削減」効果は無い

### Bazel analysis ~55s/job は「lockfile 全体の固定費」でターゲット閉包に非比例

同 run の 3 job 実測 (loading + analysis):

| job | 閉包 | loading (repo rule) | analysis | 合計 |
|---|---|---|---|---|
| backend | 603 pkgs / 23,369 targets | ~34s | ~24s | ~58s |
| tenko | 373 pkgs / 14,221 targets | ~33s | ~23s | ~56s |
| gateway | 288 pkgs / 10,174 targets | ~32s | ~21s | ~53s |

- 前半 ~33s は `Loading: 0 packages loaded` のまま = **crate_universe が Cargo.lock 全体
  (572 crate) を処理する固定費**。どのターゲットでも満額
- 閉包を半分にしても −5s → **「build を細かく分割して analysis を減らす」は成立しない**
- 削る手段は CPU 増 (larger runner、ただし public repo でも有料) / 依存削減 /
  Bazel サーバ常駐 (self-hosted、public repo では不可) / Skycache (OSS 未公開) のみ

### alc-core ドメイン分割 Phase A (#513 / PR #514、merge 済み)

- alc-core (10,495 行、18 crate が依存) の `AppState` が全 ~60 repository を束ねる
  god-object で、**domain 機能 PR が全 21 crate + 全 test shard を再コンパイル**していた
  (実例: feat/trouble-field-layout = trouble 専用機能なのに backend 443 actions 再ビルド)
- 使用行列の実測で「層分割は無効 (全 crate が全層参照)、ドメイン分割が正解」と確定
- Phase A で tenko の trait 9 本 + models 392 行 + driver_info + overdue を alc-tenko へ移設。
  再流入は `scripts/check_domain_split.sh` (check job) が loud fail
- **merge 直後の 1-2 run は cache 焼き直しでむしろ遅い (正常)**。warm 後の効果
  (tenko 系 PR で他 shard がキャッシュヒット化) は実測して本 doc に追記すること
- 後続: Phase B trouble (11 repo + 17 struct) → dtako / notify / carins

### Bazel test 化 PoC (#515 / PR #517 #518、merge 済み)

- `bazel-test-poc` job (独立 job、merge gate 外) で alc-csv-parser 1 crate を検証:
  - bazel test 動作 ✅ / **テスト結果キャッシュ** ✅ (同一サーバ 2 回目 `(cached) PASSED in 0.0s`)
  - **lcov gate (`scripts/check_coverage_100_lcov.sh`) は llvm-cov --text と判定意味論が一致** ✅
    (4 ファイル中 3 つは行数まで一致 — gate 移行の最大の壁は突破可能と確定)
- 検出された差異はフォーマットではなく**測定範囲**: combined 測定 (他 crate のテスト経由) で
  100% だった work_segments.rs の 5 行が、Bazel の per-target 測定で露呈 → crate 内テスト
  3 本追加で解消 (#518)。他 crate へ拡大する際も同種の「里帰りテスト追加」が必要になる見込み
- **run 跨ぎの test result cache (2026-07-05 PR #519 で実測 → 原因確定 → warm 追加で解消見込み)**:
  1 回目 invocation は `Executed 1 out of 1` + restore が `No disk cache found` で不発だった。
  原因は Bazel のキャッシュ意味論ではなく **GH Actions cache の scope 分離**:
  - setup-bazel の disk-cache tar は GH Actions cache 保存で、PR run の save は
    `refs/pull/N/merge` scope に隔離され**別 PR から読めない** (save 自体は毎 run 成功していた)
  - `bazel-test-poc` job は pull_request 限定 → 全 PR から読める **main scope に save される
    機会がゼロ** = どの PR の 1 回目も恒常 miss する構造だった
  - 対照: `Build backend` は main push の cache-warm-bazel が save するので PR から
    `Cache hit for: ...disk-bazel-backend-tar-...` (488MB、1366 disk cache hit) で復元できている
  - 対策: `cache-warm-bazel-test-poc` job (main push で `bazel test` を warm) を追加。
    invocation flags を poc job と一致させるのが必須 (configuration 差 = action key 差)
  - PR #520 merge 後の main push で初回 warm 完了 (test PASSED → disk cache 60MB を
    main scope に save、run 28743114702)。warm は `bazel test` のみなので poc job の
    `bazel coverage` action は含まれない (coverage ~30s は PR 側で毎回実行、判定点の
    test result cache には影響しない)
- **run 跨ぎ hit 成立を PR #521 で実証**: 1 回目の invocation から `(cached) PASSED` /
  `Executed 0 out of 1` (298 disk cache hit、コンパイル実行ゼロ)。job 3分30秒 → **1分44秒**
  (残りは loading/analysis 固定費 61s + coverage 29s)
- 実証を受けて poc を**全 crate の unit test に拡大** (`bazel test //... --build_tests_only`、
  warm も同一 invocation)。lcov gate / coverage gate の SoT は引き続き cargo llvm-cov 側
- 拡大時に洗い出した bazel 環境差 2 件 (どちらも修正済み): (1) dev-dependencies を持つ
  crate (alc-misc / alc-notify) の rust_test に `all_crate_deps(normal_dev)` 配線が必要、
  (2) alc-notify redact テストの libpdfium.so dlopen — cargo Tests と同じ Install PDFium
  step を bazel job にも配置 (#525)。修正後 17/17 PASSED、warm 定常の単一 job は 2m52s
- **crate 単位 shard 分割** (17 shard、`bazel-test-<crate>` の専用 disk cache key):
  重複ビルドが浪費するのは CPU であって wall time ではなく、wall time は最重 shard で
  bounded になる (alc-core 変更時: 単一 job 4-5 分 → 並列 ~2-3 分、最重は monolith
  shard の最深チェーン)。leaf shard の定常は analysis 60s + restore ≈ 1.5-2 分。
  トレードオフ: PR あたり +16 job (Free 20 枠では Tests の queue 悪化 → Team 化推奨) と
  cache 容量 (17 key、GH 10GB 上限)。matrix の追加漏れ/warm-poc 同期は
  `scripts/check_bazel_test_matrix.sh` (csv-parser shard 内) が loud fail
- **shard 分割の実測 (PR #527 = alc-core を触る最悪ケース)**: 17 shard 全 green、
  wall **~2.5 分に bounded** (最重 notify 151s、大半 80-90s)。単一 job の cold 5 分から
  設計どおり
- **bazel gate 移行 PR1**: mock 統合テスト 6 binary (`tests/mock_<domain>/main.rs`、DB 不要)
  を rust_test target 化して shard matrix に追加 (17 → 23 shard)。cargo Tests matrix は
  当面 gate として並走 (PR4 で退役予定)。注意: mock shard は monolith lib 閉包を含むため
  disk cache が各 ~500MB 級 — GH cache 10GB 上限との兼ね合いで eviction が観測されたら
  shard 統合で対処する
- **PR2**: CI が実際に走らせている DB integration 2 binary (`trouble_test` /
  `archive_repo_test`) を bazel 化 (25 shard)。postgres service + `--test_env=
  TEST_DATABASE_URL` + init_local_db.sql を持つ別 job (`bazel-test-db` とその warm)。
  発見: tests/ の他 6 binary (devices_test / employees_test / measurements_test /
  devices_re_pair_test / trouble_task_statuses_test / trouble_tasks_cross_ticket_test)
  は **cargo CI でも走っていない** (test-matrix の `--test` 列挙に無い) — 要別途判断
- **PR3a pilot (mock-trouble)**: mock target 経由の route ファイル lcov は per-target
  でも 100% (files.rs 160/160 / workflow.rs 126/126、里帰り不要)。計装ビルド初回 +2 分弱、
  warm/PR-scope cache で以降 hit。Pg repo 実装 (repo/trouble_*.rs) は db shard の寄与が
  無いと 0% = **per-file gate は shard 別 lcov の merge が必須**と確定。スクリプトの
  comment-out 誤検出は tomllib 化で修正 (#532)
- **PR3b**: coverage field を matrix に追加 (21 shard が対象、無登録 crate の 4 shard は
  対象外)。各 shard が lcov を artifact 化し `bazel-coverage-gate` job が merge して
  coverage_100.toml **全域**を warn モードで突合。warm も coverage を焼く (PR 側 cache hit)。
  差分ゼロを確認したら fail 化
- **PR3b 初回 warn 結果 (PR #533 run 28747584151)**: 63 file 中 53 OK / 10 差分。
  **差分は全て instrumentation_filter の配線問題で、テスト不足・意味論不一致はゼロ**
  (分析全文は #515 コメント)。修正は PR #534:
  - `^//$` は label 文字列 (`//:unit_tests` 等) への regex match でどれにも一致せず
    root-package shard (monolith / db-archive) の lcov が空だった → **`^//:` が正**
  - shard の filter には「そのテストが exercise する**他 crate**」も含める必要がある
    (driver_info は mock_misc に、compare/pdf は mock_dtako に、alc-core の serde
    default fn は各 mock shard にテスト実体が居る)。coverage_100.toml の mock/combined
    type は shard 横断の lcov max-merge union で 100% になる設計
  - 大物 (devices.rs 1245 行 / dtako_upload.rs 1244 行 / tenko_sessions.rs 851 行) は
    行数まで cargo llvm-cov gate と一致 — merge 方式の意味論は確定
  - dormant DB テスト 6 binary (#530) に coverage を依存している file は無し
    (gate 移行のブロッカーではない)
- 残: PR #534 で warn 差分ゼロ確認 → fail 化、cargo Tests matrix 退役 + required
  checks 差し替え (PR4)、dormant な DB テスト 6 binary の扱い (#530)

### cache-warm build-only 化の実測 (PR #519、2026-07-05)

`Cache Warm (test-shared)` が **60 分 → 1 分 54 秒** (run 28742594624)。postgres の無い
warm job が DB 依存テストを `--ignore-run-fail` で実行し、PoolTimedOut (acquire timeout 30s)
× 数十本を直列で浪費していたのを `-E 'none()' --no-tests=pass` (build-only) にした効果。
同 run では tenko 分割 (Phase A) 後の warm 状態で Tests ~3 分・Builds ~3 分、
PR CI 全体 4 分 44 秒 (run 28742465845) を確認。

## 施策 log

### #482: check job の TS bindings 生成を test-matrix(lib) に統合 (PR #483)

上表 #1 の `cargo test export_bindings --workspace` (~74s) は #2 の lib shard と
丸ごと重複していた。検証と統合の内容:

**検証 (ローカル、cargo-llvm-cov 0.8.4 = CI 同版 + cargo-nextest):**

- ts-rs の `export_bindings_*` は全て各 crate の **lib target 内 `#[test]`**。
  `cargo llvm-cov nextest --lib -E 'test(export_bindings)'` で 47 テスト実行、
  51 .ts が生成された (生成元は alc-core / alc-misc / alc-carins の 3 crate のみ。
  alc-auth は Cargo.toml に ts-rs 依存があるが `#[derive(TS)]` 未使用)
- coverage 計装 (`-Cinstrument-coverage`) の有無で .ts 出力を sha256 diff →
  **完全一致**。nextest のプロセス並列書き込みでも 3 回連続で決定的

**変更 (PR #483):**

- `check` job から `Generate TypeScript bindings` step + `ts-bindings-${sha}`
  artifact upload を削除 (check は fmt + clippy のみに)
- `test-matrix` の lib shard 完了後に `ts-bindings-${sha}` を upload
  (`if-no-files-found: error` で将来の生成漏れを loud fail 化)
- `cache-warm` の check warm cmd を clippy のみに縮小
- artifact 名 / パス / retention 不変 → `scripts/export-ts-bindings.sh` の契約に影響なし

**実測 ([run 28550523724](https://github.com/ippoan/rust-alc-api/actions/runs/28550523724)):**

- 変更前: Format & Lint job 141s (うち bindings 生成 ~74s)
- 変更後: Format & Lint job **65s (-76s、54% 短縮)**
- Tests (lib): 129s — bindings upload 追加後も mock shard 群 (161〜192s) より短く、
  critical path への影響なし。`ts-bindings-*` artifact (19.7 KB) も lib shard から
  正常に upload されたことを確認

### deploy chain の直列 hop 削減 (staging image 統合 + gateway 並列化)

PR CI の deploy leg (~4 分実測) は `staging-images (max 46s) → deploy-services
(max 60s) → deploy-gateway (56s)` の 3 直列 hop で、hop ごとに runner 起動 +
checkout + gcloud auth/setup (~20-40s) が乗ることが支配要因だった (service 数の
matrix は並列なので支配要因ではない)。変更:

- **staging app image の build を ci.yml build-image job に統合** — 旧
  staging-images job は「prod image から `docker create/cp` でバイナリ抽出 →
  再パッケージ」だったが、build-image は bazel-bin にバイナリを既に持つので
  直接 build できる。staging entrypoint が使う `migrate` は各 service job で
  `//:migrate` を追加 build (BUILD.bazel で deps を sqlx/tokio/anyhow に slim 化
  済みなので増分はほぼ migrate 本体のみ)。buildx gha cache (scope:
  `<name>-staging`) で apt/PDFium layer もキャッシュされる (旧 job は cache 無しで
  毎回 apt-get + PDFium DL していた)
- **staging-db image build を ci.yml の並列 job に移設** — deploy の needs に
  入るが build-image / coverage-check より速く終わるため critical path に乗らない
- **deploy-gateway を deploy-services と並列化** — gateway が必要なのは各 service
  の `status.url` のみで、Cloud Run の URL は revision が変わっても不変。
  完全新規環境の bootstrap (service 未作成) のみ re-run が必要

**image レベルの検証の位置は不変**: staging deploy (deploy-services) → 
smoke-staging-ingest → drift-check-staging → auto-merge gate という「PR ごと
staging deploy 検証」の運用ポリシー・順序・merge gate はそのまま。むしろ staging
image の build 失敗は CI 前半 (build-image) で早く検出されるようになる。

**実測 (PR #485、[run 28551859880](https://github.com/ippoan/rust-alc-api/actions/runs/28551859880) vs 旧構成の直前 run [28551174949](https://github.com/ippoan/rust-alc-api/actions/runs/28551174949)):**

- deploy leg (最初の Deploy job 開始 → 最後の Deploy job 完了):
  **164s → 99s (-65s、40% 削減)**。同日連続 run の比較。queue 遅延が乗る run
  では旧構成は ~4 分に達していた (staging build 46s + services 1m + hop 間
  ギャップの実測 screenshot)
- 新構成では deploy-services と Deploy gateway が同時刻 (+216s) にスタート
  していることを確認 (並列化が効いている)
- build-image は staging image 統合 + cold cache で一時的に +10〜55s
  (backend 142s→197s が最大)。テスト群 (最大 193s) と並列のため critical path
  への影響は backend の +6s のみ。`<name>-staging` buildx cache と bazel
  disk-cache (//:migrate) が温まれば縮む見込み

### #506/#508: build-image への external/repository cache は実測で逆効果 → revert

期待は「analysis 60s の短縮」だったが、上記「external-cache は job 跨ぎ warm が
構造的に効かない」の通り機能せず、Build backend 139s → 201s に悪化。#508 で revert し、
経緯は ci.yml の build-image NOTE コメントにも固定 (再導入の再発防止)。

### #507: Tests (lib) の DB なし分離 + mold 導入

- **lib shard を test-matrix から分離し postgres service なしの test-lib job に**:
  Tests (lib) **122s → 108s**、runner queue 待ち 49s → 8s (run 相対で完了 66s 早い)。✅ 維持
- **mold (rui314/setup-mold、ld 差し替え = RUSTFLAGS 非変更で cache 互換)**:
  cargo build 段 28.46s → 27.93s で**中立** (リンク支配仮説は否定)。warm 側
  (--all-targets 全リンク) でも 8分29秒 vs 導入前 ~9分半でノイズ範囲。撤去判断は保留

### #510: 依存グラフ監視の常設化 (CI 常設ガード)

- **`ci.yml` の `dep-check` job** — `ippoan/ci-workflows` の `rust-dep-check.yml` reusable
  (ci-workflows#153) を呼ぶ。cargo-deny `check bans` (deny.toml `multiple-versions = "warn"`、
  重複解消後に `"deny"` 引き上げ検討) + cargo-machete (未使用依存 warn)。
  独立 job (~13s) で needs チェーン外 = critical path に乗らない
- **`dep-graph.yml` の BEP metrics step** (main push 毎) — BuildMetrics から
  targets configured / packages loaded / analysis 時間を Job Summary に出力、
  しきい値 (26000 targets / 660 packages) 超過で `::warning::`
  - 注意: `packagesLoaded` は invocation 相対値 (同 job 内の先行 bazel query がサーバを
    温めるため小さく出る、初回実測 61)。**推移監視の主役は targetsConfigured** (絶対値、
    初回実測 23370)

### #511: cargo-machete 初回検出の未使用依存 7 件を削除

grep 裏取りの上で root:alc-pdf 直接依存 / alc-dtako:csv,reqwest,zip / alc-trouble:tokio /
alc-auth:sqlx,ts-rs / gateway:http-body-util を削除。alc-storage の rust-s3 は false
positive (lib 名 `s3` で text match に掛からない) → `[package.metadata.cargo-machete]
ignored` 登録。**削除依存は全て他 crate が使用中のため package 数は 572 のまま** =
グラフ削減効果はゼロ (マニフェスト衛生 + machete warn 解消)。

## 検証済みで「効果薄い / スコープ外」と判断した案 (Refs #482)

- **FROM scratch image 化**: docker build は既に軽い (3-9s、Bazel がコンパイルを担い、
  PDFium curl レイヤーも GHA cache でヒット済み)。PDFium が動的 .so 依存のため
  scratch 化は musl 静的リンク作業が必要でリスク高、CI 速度目的の優先度は低い
- **バイナリ + main のみ image 化 (rust-ichibanboshi 方式)**: PR ごとの staging deploy
  検証という CLAUDE.md 明記の運用ポリシー変更を伴う大きな決断のため別途相談
- **cargo-llvm-cov の RUSTC_WRAPPER 干渉**: ci.yml 側で `RUSTC_WRAPPER: "sccache"` を
  明示指定しているため `RUSTFLAGS` 経由の注入にフォールバックしているはず
  (ドキュメント通りなら協調動作)。実害の証跡なし

## 今後の候補 (未着手)

- 上表 #2 (coverage 計装) と #3 (Bazel fastbuild) のプロファイル統合は原理的に不可能
  (計装バイナリと配布バイナリは別物)。残る余地は shard 分割の見直し・cache 世代管理の
  チューニング程度で、いずれも計測してから判断する
- **tag run の test-matrix スキップ** (期待 −3分): tag は main の green sha に打たれ、
  同一 commit は PR CI + main CI でテスト済み。tag run では check / test-matrix を省き
  deploy chain へ直行 (docker-latest の存在 = main CI green を軽量 gate にする)。
  tag→staged 7分→約4分
- **runner queue 待ち削減** (期待 −1分弱): PR run は同時 17 job 起動で started_at が
  最大 81s 遅延。Bazel build job 統合 (共通化) は analysis 60s を「7 回→1 回」にする
  だけで critical path の 60s は消えない (wall-clock ほぼ中立、CPU 総量 / 課金 1/7 の
  コスト削減策)。docker push 直列化のトレードオフあり
- **analysis 60s 自体を縮めるなら larger runner が本命** (loading/analysis は Skyframe
  並列でコア数が効く。4→8/16 vCPU で 60s → 25-35s 見込み。コスト増)
- **`debug = "line-tables-only"`**: rust-cache 795MB の縮小 (restore 高速化)。導入 PR の
  1 run はフル再ビルド + llvm-cov の行カバレッジ表示検証が必要
- **nextest slow test の特定**: lib 38s / mock 系の実行時間の内訳を slow test レポートで
- **rust-s3 の hyper 1.x 化 or object_store 移行**: 依存グラフ削減の最大の単発ターゲット
  (上記「依存グラフの実態」参照)

## 計測方法 (再現手順)

- job 一覧と started_at/completed_at: `gh api repos/ippoan/rust-alc-api/actions/runs/<run_id>/jobs`
- job 内のステップ境界: job log の `##[group]Run ` 行のタイムスタンプ
- Bazel analysis 時間: log の `INFO: Invocation ID` → `INFO: Analyzed N targets` の差分。
  execution は `Elapsed time` / `Critical Path` / `N processes:` 行。構造化して取るなら
  dep-graph.yml の BEP metrics step (main push 毎に Job Summary へ自動出力)
- cargo build 段: `Finished` 行の `in Xs`。sccache hit 状況は post step の `sccache --show-stats`
- テスト実行: nextest の `Summary [ Xs] N tests run` 行

## 関連

- 調査 issue: [#482](https://github.com/ippoan/rust-alc-api/issues/482)
- cache 設計の経緯: #426 (shared-key 統合 + 非対称 save-if、rust-flickr#28-#32 の実験由来)
- auto-merge と deploy の race: #405 / #391 (ci.yml の needs 設計の背景)
