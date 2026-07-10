#!/bin/bash
# gateway + per-domain (tenko/carins/dtako/trouble/camera) への実アクセスを検知して
# メール通知する Cloud Logging アラートを作成する (Refs ippoan/rust-alc-api#556)。
#
# これらの service は廃止予定で本番・staging とも休眠のはず。この安全網は
# 「万一 実リクエストが来たら」だけ通知する (no news = good news)。日次ダイジェスト
# ではなく access-on-fire 方式 = ゼロが続く前提の廃止判断に最適。
# 廃止 (Cloud Run service 削除) 完了後は policy + channel ごと消してよい:
#   gcloud alpha monitoring policies list --project="$PROJECT" \
#     --filter='displayName:"rust-alc-api gateway/per-domain access detected"'
#   gcloud alpha monitoring policies delete <POLICY_ID> --project="$PROJECT"
#
# 使い方:
#   EMAIL=you@example.com bash cloudrun/monitoring/setup-gateway-access-alert.sh
#
# べき等: 同名 channel / policy が既にあれば再利用し重複作成しない。
set -euo pipefail

PROJECT="${PROJECT:-cloudsql-sv}"
EMAIL="${EMAIL:-m.tama.ramu@gmail.com}"
CHANNEL_NAME="gateway-retire-alert"
POLICY_NAME="rust-alc-api gateway/per-domain access detected"
HERE="$(cd "$(dirname "$0")" && pwd)"

echo "project=$PROJECT  email=$EMAIL"

# 1) email 通知チャネル (同 display-name があれば再利用)
CHANNEL=$(gcloud beta monitoring channels list --project="$PROJECT" \
  --filter="displayName=\"$CHANNEL_NAME\"" --format='value(name)' | head -n1)
if [[ -z "$CHANNEL" ]]; then
  CHANNEL=$(gcloud beta monitoring channels create --project="$PROJECT" \
    --display-name="$CHANNEL_NAME" --type=email \
    --channel-labels=email_address="$EMAIL" --format='value(name)')
  echo "created channel: $CHANNEL"
else
  echo "reuse channel:   $CHANNEL"
fi

# 2) アラートポリシー (同 display-name があれば skip)
EXISTING=$(gcloud alpha monitoring policies list --project="$PROJECT" \
  --filter="displayName=\"$POLICY_NAME\"" --format='value(name)' | head -n1)
if [[ -n "$EXISTING" ]]; then
  echo "policy already exists: $EXISTING (skip)"
  exit 0
fi

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
# JSON テンプレの channel placeholder を実 channel に差し替え
sed "s#__NOTIFICATION_CHANNEL__#${CHANNEL}#" \
  "$HERE/gateway-access-alert.json" > "$TMP"

gcloud alpha monitoring policies create --project="$PROJECT" --policy-from-file="$TMP"
echo "policy created."
