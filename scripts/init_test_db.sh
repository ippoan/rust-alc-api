#!/usr/bin/env bash
# テスト DB を初期化する (alc_api schema + roles)。CI の DB テストの job が使う。
#
# init の SQL は **Cargo.toml で固定した rev の ippoan/alc-migrations の `scripts/init_local_db.sql`** を使う。
# この repo の `scripts/init_local_db.sql` は 152 番で止めた写し (追加・変更禁止) で、migration 174 が
# 作り直す `resolve_sso_config` を古い引数名で先に作ってしまうため、rev を 174 以上に上げると
# 全部の DB テストが migration の段で落ちる (42P13)。migration と init は同じ rev から取る。
#
# Usage: scripts/init_test_db.sh [<psql の接続先>]   (省略時は PG* の環境変数)
set -euo pipefail

cd "$(dirname "$0")/.."
rev=$(sed -n 's/^alc-migrations = .*rev = "\([0-9a-f]\{40\}\)".*/\1/p' Cargo.toml)
if [[ -z "$rev" ]]; then
  echo "Cargo.toml から alc-migrations の rev を読めない" >&2
  exit 1
fi
sql=$(curl -fsSL --retry 3 "https://raw.githubusercontent.com/ippoan/alc-migrations/${rev}/scripts/init_local_db.sql")
psql -v ON_ERROR_STOP=1 "$@" <<<"$sql"
