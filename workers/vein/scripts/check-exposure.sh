#!/usr/bin/env bash
# alc-vein Worker が外から素で届かないことを wrangler.toml で検査する (Refs #680 / #683 / #691)。
#
# この Worker は JWT を検証せず X-Tenant-ID を信頼するので、本番の到達経路は auth-worker からの
# Service Binding だけでなければならない (#556 と同じ穴を開けない)。
#   - トップレベル (本番): workers_dev = false と preview_urls = false が明示されている
#   - どこにも (トップレベル・env 配下とも) route / routes / custom_domain が無い
#   - workers_dev / preview_urls が true になる env (staging) は、src/staging_gate.rs の栓が
#     必ず効く設定を持つ: secrets.required に STAGING_TEST_SECRET (無ければ deploy が失敗する) と
#     vars.STAGING_GATE = "required" (secret が消えても全部 401 にする)
# 違えば exit 1。CI で毎回走らせる。
#
#   bash scripts/check-exposure.sh [wrangler.toml]
set -euo pipefail

TOML="${1:-$(dirname "$0")/../wrangler.toml}"

python3 - "$TOML" <<'PY'
import sys
import tomllib

path = sys.argv[1]
with open(path, "rb") as f:
    cfg = tomllib.load(f)

errors = []

def err(msg):
    errors.append(msg)
    print(f"::error file={path}::{msg}")

# 本番 (トップレベル): 明示的に false
for key in ("workers_dev", "preview_urls"):
    if cfg.get(key) is not False:
        err(f"トップレベルに {key} = false がない (本番が外から直接届く)")

envs = cfg.get("env", {})

# route / routes はどこにも書かない (custom_domain も routes の中に書くのでこれで塞がる)
for scope, table in [("トップレベル", cfg)] + [(f"env.{n}", e) for n, e in envs.items()]:
    for key in ("route", "routes"):
        if key in table:
            err(f"{scope} に {key} がある (外から直接届く)")

# 公開される env には staging の栓が必須
for name, e in envs.items():
    # workers_dev / preview_urls は env へ継承される
    public = [k for k in ("workers_dev", "preview_urls") if e.get(k, cfg.get(k)) is True]
    if not public:
        continue
    required = e.get("secrets", {}).get("required", [])
    if "STAGING_TEST_SECRET" not in required:
        err(f"env.{name} は {'/'.join(public)} = true なのに secrets.required に STAGING_TEST_SECRET がない")
    if e.get("vars", {}).get("STAGING_GATE") != "required":
        err(f'env.{name} は {"/".join(public)} = true なのに vars.STAGING_GATE = "required" がない')

if errors:
    sys.exit(1)
print(f"OK: {path} は本番が workers_dev / preview_urls = false・route 無しで、公開する env は STAGING_TEST_SECRET の栓を持つ")
PY
