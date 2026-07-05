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
