#!/bin/bash
set -e

BINARY="${APP_BINARY:-${1:-rust-alc-api}}"

# Wait for postgres sidecar to be ready
echo "Waiting for PostgreSQL..."
until pg_isready -h localhost -p 5432 -U postgres 2>/dev/null; do
  sleep 1
done
echo "PostgreSQL is ready"

# Run migrations
echo "Running migrations..."
DATABASE_URL="postgresql://postgres:staging@localhost:5432/postgres?options=-c search_path=alc_api" \
  /usr/local/bin/migrate

# Seed staging default tenant.
#
# Cloud Run staging は揮発 DB (sidecar postgres + emptyDir、`minScale: 0` で
# アイドル ~15min 後に消える)。コールドスタート時に `tenants` が空になるため、
# email-receiver 等から固定 `tenant_id` でテーブル INSERT すると FK 違反で 500
# (Refs ippoan/email-receiver#1 — 2026-06-16 epic e2e で踏んだ)。
# マイグレーションは 1 回しか走らないので seed には不適、ここで毎 cold start
# 投入する。`ON CONFLICT DO NOTHING` で idempotent。
#
# staging-only: 本 entrypoint.sh は `Dockerfile.app` から build される staging
# multi-container image でのみ使われるため、本番 (Supabase) には触れない。
echo "Seeding staging default tenant..."
PGPASSWORD=staging psql -h localhost -p 5432 -U postgres -d postgres -v ON_ERROR_STOP=1 <<'SQL'
INSERT INTO alc_api.tenants (id, name, slug, email_domain, created_at)
VALUES (
  '11111111-1111-1111-1111-111111111111',
  'Staging Default',
  'staging-default',
  'example.com',
  NOW()
)
ON CONFLICT (id) DO NOTHING;
SQL
echo "Staging default tenant seeded"

echo "Migrations completed, starting ${BINARY}..."

# Start the app with DATABASE_URL pointing to local postgres
export DATABASE_URL="postgresql://postgres:staging@localhost:5432/postgres?options=-c search_path=alc_api"
exec /usr/local/bin/"${BINARY}"
