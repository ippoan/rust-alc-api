#!/bin/bash
# ci.yml の bazel test matrix (bazel-test-poc / bazel-test-db とその warm) が
# BUILD ファイルの rust_test target を網羅しているか + 各 job pair (poc↔warm)
# の matrix が一致しているかを検証する (Refs #515)。
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

# 実行 job は ci.yml、warm job は cache-warm.yml (ci.yml から workflow_call で呼ぶ
# reusable。cache 全消失時に単体 dispatch するため分離した、Refs #574) にある。
WORKFLOWS = [".github/workflows/ci.yml", ".github/workflows/cache-warm.yml"]
jobs = {}
for wf in WORKFLOWS:
    jobs.update(yaml.safe_load(open(wf, encoding="utf-8"))["jobs"])

def matrix_targets(job_name):
    job = jobs.get(job_name)
    if job is None:
        print(f"::error::{' / '.join(WORKFLOWS)} のどれにも job '{job_name}' がありません")
        sys.exit(1)
    return {e["target"]: e for e in job["strategy"]["matrix"]["include"]}

# (実行 job, 対応する warm job) の pair。db 系は postgres service 付きの別 job。
PAIRS = [
    ("bazel-test-poc", "cache-warm-bazel-test-poc"),
    ("bazel-test-db", "cache-warm-bazel-test-db"),
]

fail = 0
listed = {}
for run_job, warm_job in PAIRS:
    run_m = matrix_targets(run_job)
    warm_m = matrix_targets(warm_job)
    if run_m.keys() != warm_m.keys():
        for t in sorted(set(run_m) ^ set(warm_m)):
            print(f"::error::{run_job}/{warm_job} の matrix target 不一致: {t}")
            fail += 1
    else:
        for t, e in run_m.items():
            w = warm_m[t]
            for k in ("name", "pdfium", "coverage"):
                if e.get(k) != w.get(k):
                    print(f"::error::{t} の matrix field '{k}' が {run_job}/{warm_job} で不一致 ({e.get(k)} != {w.get(k)})")
                    fail += 1
    for t in run_m:
        if t in listed:
            print(f"::error::{t} が複数 job の matrix に重複しています ({listed[t]} と {run_job})")
            fail += 1
        listed[t] = run_job

# --- matrix の coverage config が .bazelrc に定義されているか ---
# (filter 実体は .bazelrc の "coverage:cov-<shard>" config。ci.yml 側は config 名のみ)
rc_src = open(".bazelrc", encoding="utf-8").read()
rc_configs = set(re.findall(r"^coverage:(\S+) --instrumentation_filter=", rc_src, re.M))
used_configs = set()
for run_job, _ in PAIRS:
    for e in matrix_targets(run_job).values():
        cov = e.get("coverage")
        if cov is None:
            continue
        used_configs.add(cov)
        if cov not in rc_configs:
            print(f"::error::matrix の coverage config '{cov}' が .bazelrc に定義されていません (coverage:{cov} --instrumentation_filter=... を追加)")
            fail += 1
for stale_cfg in sorted(rc_configs - used_configs):
    print(f"::error::.bazelrc の coverage config '{stale_cfg}' を使う matrix entry がありません (削除漏れ)")
    fail += 1

# --- dev-dependencies を持つ crate の rust_test に normal_dev 配線があるか ---
# cargo は Cargo.toml から dev-deps を自動解決するが bazel は BUILD 配線が別なので、
# `cargo check --tests` では捕まらない (alc-misc #523 / alc-devices #539 で 2 回実害)。
import tomllib
for bf in glob.glob("crates/*/BUILD.bazel") + ["BUILD.bazel"]:
    crate_dir = "." if bf == "BUILD.bazel" else bf[: -len("/BUILD.bazel")]
    try:
        manifest = tomllib.load(open(f"{crate_dir}/Cargo.toml", "rb"))
    except FileNotFoundError:
        continue
    if not manifest.get("dev-dependencies"):
        continue
    bsrc = open(bf, encoding="utf-8").read()
    for m in re.finditer(r'rust_test\((.*?)\n\)', bsrc, re.S):
        block = m.group(1)
        name = re.search(r'name = "([^"]+)"', block)
        if "normal_dev" not in block:
            print(f"::error::{bf} の rust_test '{name.group(1) if name else '?'}' に all_crate_deps(normal_dev = True) がありません ({crate_dir}/Cargo.toml は dev-dependencies を持つ → bazel で FAILED TO BUILD になる)")
            fail += 1

# --- BUILD ファイルから rust_test target を列挙 ---
expected = set()
for f in glob.glob("crates/*/BUILD.bazel") + ["BUILD.bazel"]:
    src = open(f, encoding="utf-8").read()
    for m in re.finditer(r'rust_test\(\s*name = "([^"]+)"', src):
        pkg = "" if f == "BUILD.bazel" else f[: -len("/BUILD.bazel")]
        expected.add(f"//{pkg}:{m.group(1)}")

missing = expected - set(listed)
for t in sorted(missing):
    print(f"::error::rust_test target {t} が ci.yml のどの bazel test matrix にもありません (追加漏れ = 無言で対象外)")
    fail += 1

stale = set(listed) - expected
for t in sorted(stale):
    print(f"::error::matrix の {t} に対応する rust_test が BUILD にありません (削除漏れ)")
    fail += 1

if fail:
    print(f"::error::bazel test matrix check: {fail} 件の不整合")
    sys.exit(1)

print(f"bazel test matrix OK — {len(expected)} rust_test target を網羅、各 job pair の warm 一致")
PYEOF
