#!/bin/bash
# provision-runtime-sa.sh — Cloud Run 専用 runtime SA の作成と grant (Refs #606)
#
# Usage:
#   bash scripts/provision-runtime-sa.sh <staging|production> [--apply]
#
# 既定は dry-run (実行する gcloud コマンドを出力するだけ)。--apply で実際に流す。
#
# 何をするか:
#   1. runtime SA を作る (既にあれば skip)
#   2. その env の render.sh 出力に現れる **全 secret** に per-secret
#      roles/secretmanager.secretAccessor を付ける
#   3. project レベルの role を付ける (下の PROJECT_ROLES)
#   4. CI deployer SA に roles/iam.serviceAccountUser を付ける
#      (非デフォルト SA を指定して deploy するのに actAs が要る)
#
# secret の一覧は **render.sh の出力から機械的に**取る。手打ちしないのは、
# 1 件でも漏れると cutover が `Permission denied on secret: <NAME>` で落ちる
# ため (Refs #391 で実際に踏んだ失敗)。
#
# やらないこと:
#   - rust-alc-api service の run.invoker binding。compute SA は **サービス単位で
#     明示 bind** されており (T-B 実測: 747065218280-compute@ と
#     alc-api-proxy-invoker@)、editor 由来ではないので T-E の editor 剥奪では
#     壊れない。呼び出し側 (rust-ichibanboshi) の SA 移行は T-E の担当で、
#     その際は **新 SA を invoker に足してから compute SA を外す** こと。
#   - compute SA からの accessor 剥奪。rust-alc-api を移しても compute SA は
#     kintai-push-database-url / rust-logi-* / RELEASE_WAVE_GCP_API_KEY 等を
#     他 service のために使い続ける (T-B 実測: accessor 41 件、実使用 27 件)。
set -euo pipefail

ENV="${1:?Usage: provision-runtime-sa.sh <staging|production> [--apply]}"
APPLY=0
[[ "${2:-}" == "--apply" ]] && APPLY=1

case "$ENV" in
  staging|production) ;;
  *) echo "env must be staging or production" >&2; exit 1 ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="cloudsql-sv"
FCM_PROJECT="alc-fcm"

# project レベルの role。
#   - logging.logWriter: Cloud Run の stdout 取り込み自体は service agent 経由なので
#     runtime SA には不要な可能性が高いが、Google の Cloud Run least-privilege 推奨に
#     従って付ける。外せるかは flip 後 24h のログで判断する。
#   - cloudsql.client / cloudsql.instanceUser は **付けない**: render.sh に
#     run.googleapis.com/cloudsql-instances annotation が無く、DB は
#     DATABASE_URL (secret) の接続文字列で直接繋いでいる = Cloud SQL connector
#     経路を使っていない。Recommender に使用実績が出た場合のみ追加すること。
PROJECT_ROLES=(
  roles/logging.logWriter
)

# 別 project alc-fcm 側の role。T-B 実測で compute SA の alc-fcm binding は
# roles/firebase.sdkAdminServiceAgent の 1 本のみ。prod / staging 双方の新 SA に
# 同じものを付ける (FCM_PROJECT_ID=alc-fcm は render.sh が env 分岐の外で
# 無条件に出しており、staging でも FcmSender が生きているため)。
# 実行者が alc-fcm 側にも IAM 編集権限を持っている必要がある。
FCM_ROLE="roles/firebase.sdkAdminServiceAgent"

# ---------------------------------------------------------------------------
# render.sh から runtime SA と secret 一覧を取り出す (single source of truth)
# ---------------------------------------------------------------------------
RENDER_ARGS=(backend "$ENV" dummy)
if [[ "$ENV" == "staging" ]]; then
  RENDER_ARGS+=(--staging-url https://example.invalid --db-image dummy)
fi
YAML="$(bash "$ROOT/cloudrun/render.sh" "${RENDER_ARGS[@]}")"

RUNTIME_SA="$(printf '%s\n' "$YAML" | awk '/serviceAccountName:/{print $2; exit}')"
[[ -n "$RUNTIME_SA" ]] || { echo "failed to resolve runtime SA from render.sh" >&2; exit 1; }

# secretKeyRef: の 2 行下が secret 名 (key: latest を挟む)。
mapfile -t SECRETS < <(printf '%s\n' "$YAML" \
  | awk '/secretKeyRef:/{f=3} f&&/^ *name: /{print $2; f=0} f{f--}' \
  | sort -u)
(( ${#SECRETS[@]} > 0 )) || { echo "no secretKeyRef found in rendered YAML" >&2; exit 1; }

# production の Cloud Run jobs (rust-alc-api-migrate / -archive) が参照する secret。
# service 側の一覧に含まれているはずだが、含まれていなければ足す (drift 検出も兼ねる)。
if [[ "$ENV" == "production" ]]; then
  for s in alc-app-database-url dtako-r2-access-key dtako-r2-secret-key; do
    if ! printf '%s\n' "${SECRETS[@]}" | grep -qx "$s"; then
      echo "::warning:: job secret '$s' が service の secretKeyRef に無い — 追加して grant する"
      SECRETS+=("$s")
    fi
  done
fi

SA_ID="${RUNTIME_SA%%@*}"

echo "=== env=$ENV  runtime SA=$RUNTIME_SA  secrets=${#SECRETS[@]} ==="
printf '  - %s\n' "${SECRETS[@]}"
echo

run() {
  if (( APPLY )); then
    echo "+ $*"
    "$@"
  else
    printf '  '; printf '%q ' "$@"; printf '\n'
  fi
}

(( APPLY )) || echo "--- DRY RUN (--apply で実行) ---"

# 1. SA
if gcloud iam service-accounts describe "$RUNTIME_SA" --project "$PROJECT" &>/dev/null; then
  echo "service account already exists: $RUNTIME_SA"
else
  run gcloud iam service-accounts create "$SA_ID" \
    --project "$PROJECT" \
    --display-name "rust-alc-api Cloud Run runtime ($ENV)"
fi

# 2. per-secret secretAccessor
for s in "${SECRETS[@]}"; do
  run gcloud secrets add-iam-policy-binding "$s" \
    --project "$PROJECT" \
    --member "serviceAccount:${RUNTIME_SA}" \
    --role roles/secretmanager.secretAccessor \
    --condition=None
done

# 3. project roles
for r in "${PROJECT_ROLES[@]}"; do
  run gcloud projects add-iam-policy-binding "$PROJECT" \
    --member "serviceAccount:${RUNTIME_SA}" \
    --role "$r" \
    --condition=None
done

# 4. deployer に actAs。DEPLOYER_SA は GCP_SA_KEY の client_email。
#    これが無いと `gcloud run services replace` が
#    "Permission 'iam.serviceaccounts.actAs' denied" で落ちる。
#    T-B 実測では GCP_SA_KEY = staging-deploy@ で、この SA は既に **project
#    レベルで** roles/iam.serviceAccountUser を持っているため要件は充足済み。
#    以下は SA 単位で明示的に付け直す冪等な念押し (project レベルの grant を
#    将来絞った時に落ちないようにする) なので、任意。
if [[ -n "${DEPLOYER_SA:-}" ]]; then
  run gcloud iam service-accounts add-iam-policy-binding "$RUNTIME_SA" \
    --project "$PROJECT" \
    --member "serviceAccount:${DEPLOYER_SA}" \
    --role roles/iam.serviceAccountUser \
    --condition=None
else
  echo
  echo "DEPLOYER_SA 未設定 — SA 単位の actAs grant は skip (staging-deploy@ が"
  echo "project レベルで iam.serviceAccountUser を持つため要件自体は充足済み)。"
fi

# 5. 別 project alc-fcm 側の FCM 送信 role
run gcloud projects add-iam-policy-binding "$FCM_PROJECT" \
  --member "serviceAccount:${RUNTIME_SA}" \
  --role "$FCM_ROLE" \
  --condition=None

cat <<MSG

--- 手作業で残っているもの ---
- FCM は送信に失敗しても push が届かないだけでエラーが目立たない。cutover 後に
  実機で push 到達を明示的に確認すること。
- rust-alc-api service の run.invoker binding はこの script では触らない。
  compute SA はサービス単位で明示 bind されており editor 剥奪では壊れないが、
  **この binding を消すと即死する**。呼び出し側の SA 移行 (T-E) では新 SA を
  invoker に足してから compute SA を外すこと。
MSG
