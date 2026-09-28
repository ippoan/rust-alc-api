#!/usr/bin/env bash
# alc-vein Worker が外から直接届かないことを wrangler.toml で検査する (Refs #680 / #683)。
#
# この Worker は JWT を検証せず X-Tenant-ID を信頼するので、到達経路は auth-worker からの
# Service Binding だけでなければならない (#556 と同じ穴を開けない)。
#   - workers_dev = false と preview_urls = false が明示されている
#   - route / routes / custom_domain が無い
# 違えば exit 1。CI で毎回走らせる。
set -euo pipefail

TOML="${1:-$(dirname "$0")/../wrangler.toml}"
fail=0

# コメントを落とし、キー名の前後の空白を詰めた行で判定する
body="$(sed -e 's/#.*$//' "$TOML" | sed -e 's/[[:space:]]//g' | grep -v '^$' || true)"

for key in workers_dev preview_urls; do
  if ! grep -qx "${key}=false" <<<"$body"; then
    echo "::error file=${TOML}::${key} = false がない (外から直接届く)"
    fail=1
  fi
  if grep -q "^${key}=true" <<<"$body"; then
    echo "::error file=${TOML}::${key} = true がある"
    fail=1
  fi
done

# route / routes (トップレベル・[[routes]]・env 配下の [env.x.route] も含む) と custom_domain
if grep -Eq '^(route|routes)=|^\[\[?(env\.[^]]+\.)?routes?\]\]?$|custom_domain' <<<"$body"; then
  echo "::error file=${TOML}::route / routes / custom_domain がある (外から直接届く)"
  grep -En '^(route|routes)=|^\[\[?(env\.[^]]+\.)?routes?\]\]?$|custom_domain' <<<"$body" || true
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "OK: ${TOML} は workers_dev / preview_urls が false で、route / routes / custom_domain を持たない"
