#!/usr/bin/env bash
# `/vein/identify` の CPU 時間をテンプレート件数ごとに測る (Refs #680 / #683)。
# 環境変数は run-local.sh と同じ (PG_ADMIN_URL / APP_DB_URL)。件数は SIZES で渡す。
#
# 1:N 照合の上限は 500 人 (matcher::MAX_TEMPLATES、Library::new の上限) なので、500 を超える
# 件数は登録 (PUT) できない。その件数は 500 人を PUT で入れたあと残りを SQL で複写して入れ、
# 照合が 422 (too_many_templates) で断ること・そこまでの一覧の読み出し時間を測る。
set -euo pipefail
cd "$(dirname "$0")/.."

: "${PG_ADMIN_URL:?}" "${APP_DB_URL:?}"
PORT="${PORT:-8787}"
SIZES="${SIZES:-100 500 1000 5000}"
TENANT="${TENANT:-0c000000-0000-4000-8000-00000000000c}"

log="$(mktemp)"
CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE="$APP_DB_URL" \
  npx --yes "wrangler@${WRANGLER_VERSION:-4.143.0}" dev --port "$PORT" --ip 127.0.0.1 >"$log" 2>&1 &
pid=$!
trap 'kill $pid 2>/dev/null || true; wait $pid 2>/dev/null || true; rm -f "$log"' EXIT
for _ in $(seq 1 300); do
  if curl -s -o /dev/null "http://127.0.0.1:$PORT/vein/templates"; then break; fi
  sleep 1
done

for n in $SIZES; do
  psql -q "$PG_ADMIN_URL" -v tenant="$TENANT" -v name=bench -v employees="$n" -f tests/seed.sql
  enroll=$(( n < 500 ? n : 500 ))
  if [ "$n" -gt 500 ]; then
    # 登録は 500 人まで。残りは SQL で複写する (上限を超えた状態の照合を測るため)
    VEIN_URL="http://127.0.0.1:$PORT" TENANT="$TENANT" ENROLL="$enroll" R=0 node tests/bench-identify.mjs >/dev/null
    psql -q "$PG_ADMIN_URL" -v tenant="$TENANT" <<'SQL'
INSERT INTO alc_api.vein_templates (tenant_id, employee_id, template)
SELECT e.tenant_id, e.id, (SELECT template FROM alc_api.vein_templates WHERE tenant_id = :'tenant' LIMIT 1)
FROM alc_api.employees e
WHERE e.tenant_id = :'tenant'
  AND NOT EXISTS (SELECT 1 FROM alc_api.vein_templates v WHERE v.employee_id = e.id);
SQL
    enroll=0
  fi
  echo -n "n=$n: "
  VEIN_URL="http://127.0.0.1:$PORT" TENANT="$TENANT" ENROLL="$enroll" R="${R:-30}" node tests/bench-identify.mjs
done
