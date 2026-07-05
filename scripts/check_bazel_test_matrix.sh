#!/bin/bash
# ci.yml の bazel test matrix (bazel-test-poc / cache-warm-bazel-test-poc) が
# BUILD ファイルの rust_test target を網羅しているか + 両 job の matrix が
# 一致しているかを検証する (Refs #515)。
#
# crate を追加して rust_test を定義したのに matrix へ足し忘れると、その crate の
# テストが bazel 側で「無言で対象外」になる。逆に BUILD から消した target が
# matrix に残ると shard が恒常 fail する。どちらも loud fail させる。
set -euo pipefail

cd "$(dirname "$0")/.."

python3 - <<'PYEOF'
import glob
import re
import sys

import yaml

ci = yaml.safe_load(open(".github/workflows/ci.yml", encoding="utf-8"))

def matrix_targets(job_name):
    job = ci["jobs"].get(job_name)
    if job is None:
        print(f"::error::ci.yml に job '{job_name}' がありません")
        sys.exit(1)
    return {e["target"]: e for e in job["strategy"]["matrix"]["include"]}

poc = matrix_targets("bazel-test-poc")
warm = matrix_targets("cache-warm-bazel-test-poc")

# --- BUILD ファイルから rust_test target を列挙 ---
expected = set()
for f in glob.glob("crates/*/BUILD.bazel") + ["BUILD.bazel"]:
    src = open(f, encoding="utf-8").read()
    for m in re.finditer(r'rust_test\(\s*name = "([^"]+)"', src):
        pkg = "" if f == "BUILD.bazel" else f[: -len("/BUILD.bazel")]
        expected.add(f"//{pkg}:{m.group(1)}")

fail = 0

missing = expected - set(poc)
for t in sorted(missing):
    print(f"::error::rust_test target {t} が ci.yml の bazel-test-poc matrix にありません (追加漏れ = 無言で対象外)")
    fail += 1

stale = set(poc) - expected
for t in sorted(stale):
    print(f"::error::bazel-test-poc matrix の {t} に対応する rust_test が BUILD にありません (削除漏れ)")
    fail += 1

# --- warm と poc の matrix 一致 (target / name / pdfium) ---
if poc.keys() != warm.keys():
    for t in sorted(set(poc) ^ set(warm)):
        print(f"::error::warm/poc の matrix target 不一致: {t}")
        fail += 1
else:
    for t, e in poc.items():
        w = warm[t]
        for k in ("name", "pdfium"):
            if e.get(k) != w.get(k):
                print(f"::error::{t} の matrix field '{k}' が warm/poc で不一致 ({e.get(k)} != {w.get(k)})")
                fail += 1

if fail:
    print(f"::error::bazel test matrix check: {fail} 件の不整合")
    sys.exit(1)

print(f"bazel test matrix OK — {len(expected)} rust_test target を網羅、warm/poc 一致")
PYEOF
