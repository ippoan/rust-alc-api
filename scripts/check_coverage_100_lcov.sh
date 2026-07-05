#!/bin/bash
# coverage_100.toml の 100% gate を lcov レポートで再現する PoC (Refs #515)
#
# 現行 gate (check_coverage_100.sh) は cargo llvm-cov --text の出力を突合するが、
# Bazel (`bazel coverage --combined_report=lcov`) は lcov 形式を吐く。本スクリプトは
# lcov の DA (line, hit-count) レコードから「登録ファイルの全計装行が hit>0」を検証し、
# llvm-cov --text ベースの gate と同じ判定が出るかを確かめる。
#
# 使い方: check_coverage_100_lcov.sh <lcov.dat> <path-prefix>
#   <path-prefix> に一致する coverage_100.toml 登録ファイルだけを対象にする
#   (PoC は crates/alc-csv-parser 限定。全体適用は gate 移行の判断後)。
set -euo pipefail

LCOV_FILE="${1:?usage: $0 <lcov.dat> <path-prefix>}"
PREFIX="${2:?usage: $0 <lcov.dat> <path-prefix>}"

python3 - "$LCOV_FILE" "$PREFIX" <<'PYEOF'
import re
import sys

lcov_file, prefix = sys.argv[1], sys.argv[2]

# --- coverage_100.toml から対象ファイルを取る ---
registered = []
with open("coverage_100.toml", encoding="utf-8") as fh:
    for m in re.finditer(r'path\s*=\s*"([^"]+)"', fh.read()):
        if m.group(1).startswith(prefix):
            registered.append(m.group(1))

if not registered:
    print(f"::error::coverage_100.toml に prefix '{prefix}' の登録ファイルがありません")
    sys.exit(1)

# --- lcov をパース: SF -> {line: max(hit)} (複数レコードはマージ) ---
files = {}
cur = None
with open(lcov_file, encoding="utf-8") as fh:
    for line in fh:
        line = line.strip()
        if line.startswith("SF:"):
            cur = files.setdefault(line[3:], {})
        elif line.startswith("DA:") and cur is not None:
            ln, hit = line[3:].split(",")[:2]
            ln, hit = int(ln), int(hit)
            cur[ln] = max(cur.get(ln, 0), hit)
        elif line == "end_of_record":
            cur = None

def find_record(path):
    # bazel の SF は execroot 相対や絶対のことがあるので suffix match
    for sf, das in files.items():
        if sf == path or sf.endswith("/" + path):
            return sf, das
    return None, None

fail = 0
print(f"=== lcov 100% gate ({prefix}) ===")
for path in registered:
    sf, das = find_record(path)
    if das is None:
        print(f"::error::{path}: lcov に見つかりません (instrumentation_filter / SF パス形式を確認)")
        fail += 1
        continue
    total = len(das)
    missed = sorted(ln for ln, hit in das.items() if hit == 0)
    if missed:
        sample = ", ".join(map(str, missed[:10])) + (" …" if len(missed) > 10 else "")
        print(f"::error::FAIL: {path} — {total - len(missed)}/{total} lines ({len(missed)} lines missed: {sample})")
        fail += 1
    else:
        print(f"  OK: {path} — {total}/{total} instrumented lines (100%) [SF={sf}]")

if fail:
    print(f"::error::lcov gate: {fail} 件が 100% 未達。llvm-cov gate との行数差異も含め #515 に記録すること")
    sys.exit(1)
print("lcov gate OK — 全登録ファイル 100%")
PYEOF
