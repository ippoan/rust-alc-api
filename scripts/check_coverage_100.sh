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
EXTERNAL_CACHE=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --unit-only) UNIT_ONLY=true; shift ;;
    --mock-only) MOCK_ONLY=true; shift ;;
    --use-cache) EXTERNAL_CACHE="$2"; shift 2 ;;
    *) shift ;;
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
if [ "$UNIT_ONLY" = true ]; then MODE="unit-only"; elif [ "$MOCK_ONLY" = true ]; then MODE="mock-only"; else MODE="full"; fi
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
  if [ "$UNIT_ONLY" = true ]; then
    cargo llvm-cov --lib --text > "$CACHE_FILE" 2>&1 || { echo "cargo llvm-cov failed:"; tail -50 "$CACHE_FILE"; exit 101; }
  elif [ "$MOCK_ONLY" = true ]; then
    cargo llvm-cov --test 'mock_*' --text > "$CACHE_FILE" 2>&1 || { echo "cargo llvm-cov failed:"; tail -50 "$CACHE_FILE"; exit 101; }
  else
    [[ -f .test-config ]] && source .test-config
    cargo llvm-cov --text > "$CACHE_FILE" 2>&1 || { echo "cargo llvm-cov failed:"; tail -50 "$CACHE_FILE"; exit 101; }
  fi
fi

# --- --text 出力から全ファイルの Lines/Miss を awk で集計 ---
# 結果を一時ファイルに出力: "ファイル名 total miss"
SUMMARY_FILE=$(mktemp)
awk '
/^\/home.*\/src\/.*\.rs:$/ {
    if (file != "") {
        total = covered + uncovered
        printf "%s %d %d\n", file, total, uncovered
    }
    file = $0; sub(/:$/, "", file)
    covered = 0; uncovered = 0; next
}
/^[[:space:]]*[0-9]+\|[[:space:]]*0\|/ { uncovered++; next }
/^[[:space:]]*[0-9]+\|[[:space:]]*[1-9][0-9]*\|/ { covered++; next }
END {
    if (file != "") {
        total = covered + uncovered
        printf "%s %d %d\n", file, total, uncovered
    }
}
' "$CACHE_FILE" > "$SUMMARY_FILE"

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

  # サマリから該当ファイルを検索 (パス末尾一致)
  MATCH=$(grep "$filepath" "$SUMMARY_FILE" || true)

  if [ -z "$MATCH" ]; then
    echo "WARN: $filepath — not found in coverage data"
    continue
  fi

  TOTAL=$(echo "$MATCH" | awk '{print $2}')
  MISS=$(echo "$MATCH" | awk '{print $3}')
  CHECKED=$((CHECKED + 1))

  if [ "$TOTAL" -eq 0 ]; then
    echo "WARN: $filepath — 0 lines (no executable code)"
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
