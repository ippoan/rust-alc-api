#!/usr/bin/env bash
# alc-vein Worker が外から素で届かないことを wrangler.toml で検査する (Refs #680 / #683 / #691)。
#
# この Worker は JWT を検証せず X-Tenant-ID を信頼するので、本番の到達経路は auth-worker からの
# Service Binding だけでなければならない (#556 と同じ穴を開けない)。
#   - トップレベル (本番): workers_dev = false と preview_urls = false が明示されている
#   - どこにも (トップレベル・env 配下とも) route / routes が無い (custom_domain も routes の中に書く)
#   - workers_dev / preview_urls が true になってよいのは env.staging だけ
#   - どの vars にも ALLOW_INSECURE_DB (DB への平文接続を許すローカル専用フラグ) が無い
#     (staging の workers.dev は Cloudflare Access で保護する前提。README 参照)
# 違えば exit 1。CI で毎回走らせる。陰性対照は scripts/check-exposure-test.sh。
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

for scope, table in [("トップレベル", cfg)] + [(f"env.{n}", e) for n, e in envs.items()]:
    for key in ("route", "routes"):
        if key in table:
            err(f"{scope} に {key} がある (外から直接届く)")

# 公開してよいのは Access で守る env.staging だけ (workers_dev / preview_urls は env へ継承される)
for name, e in envs.items():
    for key in ("workers_dev", "preview_urls"):
        if e.get(key, cfg.get(key)) is True and name != "staging":
            err(f"env.{name} の {key} が true (公開してよいのは Access で守る env.staging だけ)")

# 平文の DB 接続を許すフラグはローカル (wrangler dev --var / .dev.vars) にだけ置く
for scope, table in [("トップレベル", cfg)] + [(f"env.{n}", e) for n, e in envs.items()]:
    if "ALLOW_INSECURE_DB" in table.get("vars", {}):
        err(f"{scope} の vars に ALLOW_INSECURE_DB がある (DB 接続が平文に落ちうる。ローカル専用)")

if errors:
    sys.exit(1)
print(f"OK: {path} は本番が workers_dev / preview_urls = false・route 無しで、公開する env は staging だけ")
PY
