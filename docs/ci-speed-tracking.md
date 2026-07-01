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

**実測:**

- 変更前: Format & Lint job 141s (うち bindings 生成 ~74s)
- 変更後: (PR #483 の CI 実測後に記入)

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

## 関連

- 調査 issue: [#482](https://github.com/ippoan/rust-alc-api/issues/482)
- cache 設計の経緯: #426 (shared-key 統合 + 非対称 save-if、rust-flickr#28-#32 の実験由来)
- auto-merge と deploy の race: #405 / #391 (ci.yml の needs 設計の背景)
