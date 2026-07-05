# CI パフォーマンス tracking

CI ビルド時間の実測ベースライン・改善施策の履歴・確定した知見を記録する。
**CI 高速化の施策を入れる (または revert する) 前に必ずここを読み、実施後は実測を追記すること。**
実測済みの失敗パターン (external cache 等) の再導入を防ぐのが第一目的。

関連: `.github/workflows/ci.yml` / Refs #506 #507 #508

## ベースライン実測 (2026-07-04)

計測元: PR run [28698773673](https://github.com/ippoan/rust-alc-api/actions/runs/28698773673) / tag run [28701104111](https://github.com/ippoan/rust-alc-api/actions/runs/28701104111) (v0.0.107)

### 全体

| run | 全体時間 | critical path |
|---|---|---|
| PR CI | 4分52秒 | runner queue 待ち (~50-80s) → Tests (~2分) → Coverage Check (11s) → staging deploy (60s) |
| tag CI | 7分01秒 | Tests matrix (~3分) → Promote 22s → migrations 62s → deploy×6 並列 60s → verify 38s → report 26s |

### Tests (lib) job 122s の内訳

| 区間 | 時間 | 備考 |
|---|---|---|
| postgres service 起動 + runner 準備 | ~21s | → #507 で lib を service なし job に分離して解消 |
| rustup + sccache + rust-cache restore | ~27s | cache restore は 795MB @ ~230MB/s + 展開 |
| cargo build 段 (`Finished in 28.46s`) | 28.5s | **sccache 100% hit (コンパイル 0 件) でもかかる固定費** (下記「知見」参照) |
| nextest 実行 (677 tests) | 38.2s | 4 vCPU |
| artifact upload 等 | ~5s | |

### Build backend (Bazel) job 139s の内訳

| 区間 | 時間 | 備考 |
|---|---|---|
| checkout + disk cache restore (445MB) | ~8s | |
| **Bazel analysis** | **60s** | `Analyzed 3 targets (603 packages loaded, 23369 targets configured)`。CI は Bazel サーバが毎回コールド |
| Bazel execution | 41s | 1824 actions 中 1366 disk cache hit / 実行 15、Critical Path 37s |
| docker build/push ×2 | ~20s | |

## 施策履歴

| # | 日付 | 施策 | PR | 実測結果 | 判定 |
|---|---|---|---|---|---|
| 1 | 2026-07-05 | build-image に external-cache / repository-cache 追加 | #506 | manifest 恒常 miss + save 並列競合で全滅 + job 139s→201s **悪化** | ❌ #508 で revert |
| 2 | 2026-07-05 | Tests (lib) を postgres service なしの test-lib job に分離 | #507 | **122s → 108s**、queue 待ち 49s → 8s (run 相対で完了 66s 早い) | ✅ 維持 |
| 3 | 2026-07-05 | mold リンカ導入 (rui314/setup-mold、ld 差し替え方式) | #507 | cargo build 段 28.46s → 27.93s で**中立** (リンク支配仮説は否定) | ⚠️ cache-warm (--all-targets 全リンク) での効果を確認して撤去判断 |

## 確定した知見 (再検証不要)

### Bazel analysis 60s はダウンロードではなく CPU 処理

- repository-cache (266MB) が hit した run でも analysis は 60s で不変 (#507 run で実証)
- 正体は repo rule 実行 + 603 packages の loading + 23369 targets の analysis (Skyframe、CPU-bound)
- **actions/cache 系では削れない**。削るなら larger runner (CPU 並列) か Bazel 構造の変更のみ

### setup-bazel の external-cache は job 跨ぎ warm が構造的に効かない

- manifest のキャッシュキーは `external-<workflow>-<job>-manifest` 形式で **job 名 namespace**
  → `cache-warm-bazel` job が保存した manifest は `build-image` job から見えない
- さらに PR run の cache scope は PR merge-ref 単位 → 前 PR が保存した cache も次の PR からは見えない
- 並列 matrix (7 job) の save は同一キーの reserve を取り合い、`Unable to reserve cache ... another job may be creating` で**全滅し得る** (2 run 連続で全 job fail を確認)
- disk-cache (名前を `disk-cache` input で揃えられる) とは挙動が違う点に注意

### cargo build 段 28s は「リンク」でも「コンパイル」でもない固定費

- sccache 100% hit (`Compile requests executed 14, Cache hits 14`) でも 28s かかる
- mold (ld 差し替え) で 28.46s → 27.93s = リンク支配ではない
- `build.rs` は `rerun-if-changed=migrations` のみで無害 (sha 焼き込みなし)
- 残る候補: cargo のフィンガープリント再検証 + 依存チェーン直列の sccache 復元 + rustc 起動
- これ以上の深掘りは `cargo build --timings` を CI で一度取るのが次の手

## 未実施の候補 (期待値順)

1. **tag run の test-matrix スキップ** (期待 −3分): tag は main の green sha に打たれ、同一 commit は PR CI + main CI でテスト済み。tag run では check / test-matrix を省き deploy chain へ直行 (docker-latest の存在 = main CI green を軽量 gate にする)。tag→staged 7分→約4分
2. **runner queue 待ち削減** (期待 −1分弱): PR run は 16 job 同時起動で started_at が最大 81s 遅延。lib 分離で悪化はしていないが、plan の同時実行枠 or Bazel build 7 job の統合を検討
   - **Bazel build job 統合 (共通化) の効果の正しい見積り**: analysis 60s は「7 job が並列に 1 回ずつ払っている」ので、統合しても critical path 上の 60s は**消えない** (払う回数が 7→1 になるだけ)。効果は (a) queue 圧削減 (16→10 job)、(b) CPU 総量 / Actions 課金 7×60s→1×60s (cache-warm-bazel 側も同様)、(c) disk-cache restore の 1 回化。トレードオフは docker build/push ×7 の直列化 (buildx の並列 push or 「bazel 1 job → binary artifact → docker 7 並列 job」の 2 段构成で緩和可能だが artifact 往復 ~20s が乗る)。**wall-clock はほぼ中立、コスト削減策として有効**
   - **analysis 60s 自体を縮めるなら larger runner が本命** (Bazel の loading/analysis は Skyframe 並列なのでコア数が効く。4→8/16 vCPU で 60s → 25-35s 見込み)
3. **`debug = "line-tables-only"`**: rust-cache 795MB の縮小 (restore 高速化)。導入 PR の 1 run はフル再ビルドになる点と、llvm-cov の行カバレッジ表示検証が必要
4. **nextest slow test の特定**: lib 38s / mock 系の実行時間の内訳。nextest の slow test レポートで sleep / wiremock 系を特定
5. **larger runner**: analysis (CPU-bound) と test 実行の両方に効くがコスト増

## 計測方法 (再現手順)

- job 一覧と started_at/completed_at: `gh api repos/ippoan/rust-alc-api/actions/runs/<run_id>/jobs` (または ci-dashboard)
- job 内のステップ境界: job log から `##[group]Run ` 行のタイムスタンプを抽出
- Bazel analysis 時間: log の `INFO: Invocation ID` → `INFO: Analyzed N targets` の差分。execution は `Elapsed time` / `Critical Path` / `N processes:` 行
- cargo build 段: `Finished \`test\` profile ... in Xs` 行。sccache の hit 状況は post step の `sccache --show-stats`
- テスト実行: nextest の `Summary [ Xs] N tests run` 行
