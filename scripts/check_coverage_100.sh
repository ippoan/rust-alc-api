#!/bin/bash
# coverage_100.toml に登録されたファイルが 100% カバレッジを維持しているか検証する
#
# Usage:
#   bash scripts/check_coverage_100.sh              # 全ファイル (unit + mock + integration)
#   bash scripts/check_coverage_100.sh --unit-only   # unit タイプのファイルのみ
#   bash scripts/check_coverage_100.sh --mock-only   # mock タイプのファイルのみ (DB 不要)
#
# 前提: cargo-llvm-cov がインストール済み
# integration モードでは TEST_DATABASE_URL が設定済みであること
#
# NOTE: --text 出力ベースで判定 (既存の /coverage-check スキルと一貫)
#       --json は閉じ括弧等を余分にカウントするため結果が異なる

set -euo pipefail

UNIT_ONLY=false
MOCK_ONLY=false
COMBINED=false
EXTERNAL_CACHE=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --unit-only) UNIT_ONLY=true; shift ;;
    --mock-only) MOCK_ONLY=true; shift ;;
    --combined) COMBINED=true; shift ;;
    --use-cache) EXTERNAL_CACHE="$2"; shift 2 ;;
    # 未知のオプションを黙って捨てない。古い --xxx が script に残っていると
    # gate が静かに緩くなるため、usage エラーで落とす
    *) echo "ERROR: unknown option: $1" >&2; sed -n '4,8p' "$0" >&2; exit 2 ;;
  esac
done

CONFIG="coverage_100.toml"
if [[ ! -f "$CONFIG" ]]; then
  echo "ERROR: $CONFIG not found"
  exit 1
fi

# --- Parse coverage_100.toml ---
declare -a PATHS=()
declare -A FILE_TYPES=()

current_path=""
while IFS= read -r line; do
  if [[ "$line" =~ ^path\ =\ \"(.+)\" ]]; then
    current_path="${BASH_REMATCH[1]}"
  elif [[ "$line" =~ ^type\ =\ \"(.+)\" && -n "$current_path" ]]; then
    PATHS+=("$current_path")
    FILE_TYPES["$current_path"]="${BASH_REMATCH[1]}"
    current_path=""
  fi
done < "$CONFIG"

echo "=== Coverage 100% Check ==="
if [ "$UNIT_ONLY" = true ]; then MODE="unit-only"; elif [ "$MOCK_ONLY" = true ]; then MODE="mock-only"; elif [ "$COMBINED" = true ]; then MODE="combined"; else MODE="full"; fi
echo "Mode: $MODE"
echo "Registered files: ${#PATHS[@]}"
echo ""

# --- Run cargo llvm-cov --text ---
CACHE_DIR="/tmp/llvm-cov-cache"
mkdir -p "$CACHE_DIR"
PROJECT_HASH=$(echo "$PWD" | md5sum | cut -c1-8)
CACHE_FILE="$CACHE_DIR/text-$PROJECT_HASH.txt"

if [ -n "$EXTERNAL_CACHE" ]; then
  echo "Using pre-built coverage data: $EXTERNAL_CACHE"
  CACHE_FILE="$EXTERNAL_CACHE"
else
  echo "Running cargo llvm-cov --text..."
  # --workspace が無いと root package しか計測されず、crates/* の登録ファイルが
  # 丸ごと「not found」になる (実際に kudgivt.rs / kudguri.rs が一度も検証されて
  # いなかった)。--lib は --workspace と併せて全 package の lib target を見る
  if [ "$UNIT_ONLY" = true ]; then
    cargo llvm-cov --workspace --lib --text > "$CACHE_FILE" 2>&1 || { echo "cargo llvm-cov failed:"; tail -50 "$CACHE_FILE"; exit 101; }
  elif [ "$MOCK_ONLY" = true ]; then
    # --lib を併せる理由: workspace crate では「unit test は crate 内の lib に在り、
    # mock test は root package から crate を叩く」ので、--lib を外すと crates/* の
    # mock 登録ファイルが人工的な部分集合で測られる (例: alc-misc/health.rs が
    # 124 行中 14 行 = 11.3% に見える。lib を含めれば 355/355 = 100%)
    cargo llvm-cov --workspace --lib --test mock_tenko --test mock_dtako --test mock_devices --test mock_carins --test mock_misc --text > "$CACHE_FILE" 2>&1 || { echo "cargo llvm-cov failed:"; tail -50 "$CACHE_FILE"; exit 101; }
  else
    [[ -f .test-config ]] && source .test-config
    cargo llvm-cov --workspace --text > "$CACHE_FILE" 2>&1 || { echo "cargo llvm-cov failed:"; tail -50 "$CACHE_FILE"; exit 101; }
  fi
fi

# --- --text 出力から全ファイルの Lines/Miss を awk で集計 ---
# 結果を一時ファイルに出力: "ファイル名 total miss"
#
# ファイル見出し行の判定は「絶対パス (先頭 /) + '.rs:' で終わる」でのみ行う。
# 以前は ^/home 決め打ちだったため、CI (/home/runner/...) 以外の場所
# (例: /tmp 配下に worktree を作るローカル運用) では 1 行もマッチせず、
# 登録簿の全ファイルが「見つからない」= 全 FAIL に見えていた。
# `/src/` を必須にしないのも意図的: 将来 src/ 以外に登録ファイルが増えたときに
# 静かに取りこぼさないため (この gate は取りこぼしたら fail、握り潰さない設計)
SUMMARY_FILE=$(mktemp)
awk '
/^\/.*\.rs:$/ {
    if (file != "") {
        total = covered + uncovered
        printf "%s %d %d\n", file, total, uncovered
    }
    file = $0; sub(/:$/, "", file)
    covered = 0; uncovered = 0; next
}
# ヒット数は 1.98k / 2.50M のようにサフィックス表記されることがある。
# 純粋な数字だけを見ると、そういう行は covered/uncovered どちらにも掛からず
# 分母から静かに消える。カウント欄を取り出して "0" かどうかだけで判定する
/^[[:space:]]*[0-9]+\|[[:space:]]*[0-9][0-9.]*[kMGTE]?[[:space:]]*\|/ {
    split($0, f, "|")
    cnt = f[2]
    gsub(/[[:space:]]/, "", cnt)
    if (cnt == "0") uncovered++; else covered++
    next
}
END {
    if (file != "") {
        total = covered + uncovered
        printf "%s %d %d\n", file, total, uncovered
    }
}
' "$CACHE_FILE" > "$SUMMARY_FILE"

# 登録簿の全ファイルが "not found" になるのは大抵「カバレッジ不足」ではなく
# 「このスクリプトが --text の出力を 1 行も読めていない」。区別できるよう、
# ファイル見出しが 1 件も拾えていない場合は先に ERROR を出す
if [ ! -s "$SUMMARY_FILE" ]; then
  echo "ERROR: cargo llvm-cov --text の出力からファイル見出し行を 1 件も拾えませんでした。" >&2
  echo "       これから登録簿の全ファイルが FAIL しますが、原因はカバレッジ不足ではなく" >&2
  echo "       このスクリプトが出力形式を読めていないことです。" >&2
  echo "       考えられる原因: cargo-llvm-cov のバージョン差で --text の書式が変わった / $CACHE_FILE が空・壊れている" >&2
  echo "       $CACHE_FILE の中身を確認してください" >&2
  echo "" >&2
fi

# --- Check each file ---
FAILED=0
CHECKED=0
SKIPPED=0

for filepath in "${PATHS[@]}"; do
  ftype="${FILE_TYPES[$filepath]}"

  # unit-only モードでは unit タイプのみチェック
  if [ "$UNIT_ONLY" = true ] && [ "$ftype" != "unit" ]; then
    SKIPPED=$((SKIPPED + 1))
    continue
  fi

  # mock-only モードでは mock タイプのみチェック
  if [ "$MOCK_ONLY" = true ] && [ "$ftype" != "mock" ]; then
    SKIPPED=$((SKIPPED + 1))
    continue
  fi

  # combined モードでは全タイプをチェック (lib + mock の combined report を使用)
  # unit, mock, combined すべて対象

  # サマリから該当ファイルを検索。
  # 部分一致だと別ファイルに巻き込まれ、複数行ヒットした時に後段の数値比較が壊れる。
  # "/" 境界での末尾完全一致にし、複数マッチは ambiguous として明示的に落とす
  MATCH=$(awk -v want="/$filepath" 'index($1, want) == length($1) - length(want) + 1' "$SUMMARY_FILE" || true)
  MATCH_COUNT=$(printf '%s' "$MATCH" | grep -c . || true)

  if [ "$MATCH_COUNT" -gt 1 ]; then
    echo "FAIL: $filepath — ambiguous, matched $MATCH_COUNT files in coverage data:"
    printf '%s\n' "$MATCH" | awk '{print "      " $1}'
    FAILED=1
    continue
  fi

  if [ -z "$MATCH" ]; then
    # 黙ってスキップしない。gate が「見ている」と広告する範囲が実際に検証した
    # 範囲を静かに超えるなら、gate が無いより悪い
    echo "FAIL: $filepath — not found in coverage data"
    echo "      考えられる原因: (1) 計測コマンドの対象外 (crates/* は --workspace が要る)"
    echo "                      (2) パスが coverage_100.toml と実体でずれている"
    echo "                      (3) ファイルが削除・移動された"
    FAILED=1
    continue
  fi

  TOTAL=$(echo "$MATCH" | awk '{print $2}')
  MISS=$(echo "$MATCH" | awk '{print $3}')
  CHECKED=$((CHECKED + 1))

  if [ "$TOTAL" -eq 0 ]; then
    # 実行可能行ゼロを OK 扱いすると、登録されているのに何も検証していない
    # ファイルが「維持されている」ように見える
    echo "FAIL: $filepath — 0 executable lines (登録する意味がない。登録簿から外すこと)"
    FAILED=1
    continue
  fi

  if [ "$MISS" -gt 0 ]; then
    COVERED=$((TOTAL - MISS))
    PCT=$(awk "BEGIN {printf \"%.1f\", $COVERED/$TOTAL*100}")
    echo "FAIL: $filepath — $COVERED/$TOTAL lines ($PCT%, $MISS lines missing)"
    # 未カバー行を表示
    FULL_PATH=$(grep "$filepath" "$SUMMARY_FILE" | awk '{print $1}')
    awk -v fp="$FULL_PATH:" '$0 == fp {found=1; next} /^$/{found=0} found && /^[[:space:]]*[0-9]+\|[[:space:]]*0\|/ {print "      " $0}' "$CACHE_FILE" | head -20
    FAILED=1
  else
    echo "  OK: $filepath — $TOTAL/$TOTAL lines (100%)"
  fi
done

rm -f "$SUMMARY_FILE"

echo ""
echo "Checked: $CHECKED, Skipped: $SKIPPED"

if [ "$FAILED" -eq 1 ]; then
  echo ""
  echo "FAILED: Coverage regression detected. Fix the files above."
  exit 1
fi

echo "All registered files maintain 100% coverage."
