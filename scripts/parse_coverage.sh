#!/bin/bash
# parse_coverage.sh -- Parse cargo llvm-cov --text output for GitHub Actions Job Summary
#
# Usage:
#   bash scripts/parse_coverage.sh summary "" /tmp/llvm-cov-output.txt
#   bash scripts/parse_coverage.sh not-100 "" /tmp/llvm-cov-output.txt
#   bash scripts/parse_coverage.sh file "devices" /tmp/llvm-cov-output.txt

set -euo pipefail

MODE="$1"
FILE_PATTERN="${2:-}"
COV_FILE="$3"

if [[ ! -f "$COV_FILE" ]]; then
  echo "ERROR: Coverage file not found: $COV_FILE"
  exit 1
fi

if [[ "$MODE" == "file" && -z "$FILE_PATTERN" ]]; then
  echo "ERROR: mode=file requires file_pattern input"
  exit 1
fi

# GITHUB_STEP_SUMMARY fallback for local testing
OUT="${GITHUB_STEP_SUMMARY:-/dev/stdout}"

case "$MODE" in
  summary)
    {
      echo "## 📊 Coverage Summary"
      echo ""
      echo "| File | Lines | Miss | Coverage |"
      echo "|------|------:|-----:|---------:|"
      awk '
      /^\/.*\/src\/.*\.rs:$/ {
          if (file != "") {
              total = covered + uncovered
              if (total > 0) {
                  pct = sprintf("%.1f%%", covered * 100.0 / total)
                  printf "| %s | %d | %d | %s |\n", file, total, uncovered, pct
              }
              grand_total += total; grand_uncov += uncovered
          }
          file = $0; sub(/:$/, "", file); sub(/.*\/src\//, "src/", file)
          covered = 0; uncovered = 0; next
      }
      /^[[:space:]]*[0-9]+\|[[:space:]]*0\|/ { uncovered++; next }
      /^[[:space:]]*[0-9]+\|[[:space:]]*[1-9][0-9]*\|/ { covered++; next }
      END {
          if (file != "") {
              total = covered + uncovered
              if (total > 0) {
                  pct = sprintf("%.1f%%", covered * 100.0 / total)
                  printf "| %s | %d | %d | %s |\n", file, total, uncovered, pct
              }
              grand_total += total; grand_uncov += uncovered
          }
          pct = (grand_total > 0) ? sprintf("%.1f%%", (grand_total - grand_uncov) * 100.0 / grand_total) : "-"
          printf "| **TOTAL** | **%d** | **%d** | **%s** |\n", grand_total, grand_uncov, pct
      }
      ' "$COV_FILE"
    } >> "$OUT"
    ;;

  not-100)
    {
      echo "## 🔍 Files NOT at 100% Coverage"
      echo ""
      echo "Sorted by uncovered line count (descending)."
      echo ""
      echo "| File | Lines | Miss | Coverage |"
      echo "|------|------:|-----:|---------:|"
      awk '
      /^\/.*\/src\/.*\.rs:$/ {
          if (file != "") {
              total = covered + uncovered
              if (uncovered > 0 && total > 0) {
                  pct = sprintf("%.1f%%", covered * 100.0 / total)
                  printf "%d\t| %s | %d | %d | %s |\n", uncovered, file, total, uncovered, pct
              }
          }
          file = $0; sub(/:$/, "", file); sub(/.*\/src\//, "src/", file)
          covered = 0; uncovered = 0; next
      }
      /^[[:space:]]*[0-9]+\|[[:space:]]*0\|/ { uncovered++; next }
      /^[[:space:]]*[0-9]+\|[[:space:]]*[1-9][0-9]*\|/ { covered++; next }
      END {
          if (file != "") {
              total = covered + uncovered
              if (uncovered > 0 && total > 0) {
                  pct = sprintf("%.1f%%", covered * 100.0 / total)
                  printf "%d\t| %s | %d | %d | %s |\n", uncovered, file, total, uncovered, pct
              }
          }
      }
      ' "$COV_FILE" | sort -t$'\t' -k1 -rn | cut -f2-
    } >> "$OUT"
    ;;

  file)
    {
      echo "## 🔎 Uncovered Lines: \`${FILE_PATTERN}\`"
      echo ""

      # Summary for matching files
      echo "### Summary"
      echo ""
      echo "| File | Lines | Miss | Coverage |"
      echo "|------|------:|-----:|---------:|"
      awk -v pat="$FILE_PATTERN" '
      /^\/.*\/src\/.*\.rs:$/ {
          if (file != "") {
              total = covered + uncovered
              if (total > 0 && index(file, pat) > 0) {
                  pct = sprintf("%.1f%%", covered * 100.0 / total)
                  printf "| %s | %d | %d | %s |\n", file, total, uncovered, pct
              }
          }
          file = $0; sub(/:$/, "", file); sub(/.*\/src\//, "src/", file)
          covered = 0; uncovered = 0; next
      }
      /^[[:space:]]*[0-9]+\|[[:space:]]*0\|/ { uncovered++; next }
      /^[[:space:]]*[0-9]+\|[[:space:]]*[1-9][0-9]*\|/ { covered++; next }
      END {
          if (file != "") {
              total = covered + uncovered
              if (total > 0 && index(file, pat) > 0) {
                  pct = sprintf("%.1f%%", covered * 100.0 / total)
                  printf "| %s | %d | %d | %s |\n", file, total, uncovered, pct
              }
          }
      }
      ' "$COV_FILE"

      echo ""
      echo "### Uncovered Lines"
      echo ""

      # Extract uncovered lines with context
      awk -v pat="$FILE_PATTERN" '
      /^\/.*\/src\/.*\.rs:$/ {
          if (in_target && printed_any) printf "```\n\n"
          file = $0; sub(/:$/, "", file)
          display = file; sub(/.*\/src\//, "src/", display)
          in_target = (index(file, pat) > 0) ? 1 : 0
          if (in_target) {
              header = display
          }
          delete lines; line_count = 0
          delete printed
          ctx_after = 0; printed_any = 0; header_printed = 0
          next
      }
      !in_target { next }
      /^[[:space:]]*[0-9]+\|[[:space:]]*0\|/ {
          line_count++
          lines[line_count] = $0
          if (!header_printed) {
              printf "**%s**\n\n```\n", header
              header_printed = 1
          }
          # Print up to 3 context lines before
          start = line_count - 3
          if (start < 1) start = 1
          for (i = start; i < line_count; i++) {
              if (!printed[i]) { printf "  %s\n", lines[i]; printed[i] = 1 }
          }
          printf ">>> %s\n", $0
          printed[line_count] = 1
          printed_any = 1
          ctx_after = 3
          next
      }
      {
          line_count++
          lines[line_count] = $0
          if (ctx_after > 0) {
              printf "  %s\n", $0
              printed[line_count] = 1
              ctx_after--
          }
      }
      END {
          if (in_target && printed_any) printf "```\n"
      }
      ' "$COV_FILE"
    } >> "$OUT"
    ;;

  *)
    echo "ERROR: Unknown mode: $MODE"
    exit 1
    ;;
esac

echo "Coverage report written to Job Summary."
