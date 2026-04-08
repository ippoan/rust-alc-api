#!/bin/bash
set -e

PROJECT_ID="cloudsql-sv"
REGION="asia-northeast1"
SERVICE_NAME="rust-alc-api"
REPOSITORY="alc-app"
IMAGE="$REGION-docker.pkg.dev/$PROJECT_ID/$REPOSITORY/$SERVICE_NAME"
ARCHIVE_JOB_NAME="rust-alc-api-archive"

# Usage: ./archive.sh dtako-archive --dry-run
#        ./archive.sh dtako-restore --tenant-id <UUID> --date 2025-01-01
COMMAND="${1:-dtako-archive}"
shift 2>/dev/null || true
ARGS="$*"

echo "=== Archive Job: $COMMAND $ARGS ==="

# Build args list for gcloud (comma-separated)
if [ -n "$ARGS" ]; then
  GCLOUD_ARGS="$COMMAND,$ARGS"
else
  GCLOUD_ARGS="$COMMAND"
fi

# Create or update the job
if gcloud run jobs describe $ARCHIVE_JOB_NAME --region $REGION --project $PROJECT_ID &>/dev/null; then
  gcloud run jobs update $ARCHIVE_JOB_NAME \
    --region $REGION \
    --project $PROJECT_ID \
    --image $IMAGE:latest \
    --args "$GCLOUD_ARGS"
else
  gcloud run jobs create $ARCHIVE_JOB_NAME \
    --region $REGION \
    --project $PROJECT_ID \
    --image $IMAGE:latest \
    --set-secrets "DATABASE_URL=alc-app-database-url:latest,DTAKO_R2_ACCESS_KEY=dtako-r2-access-key:latest,DTAKO_R2_SECRET_KEY=dtako-r2-secret-key:latest" \
    --set-env-vars "R2_ACCOUNT_ID=24b45709d060d957340180e995f0d373,DTAKO_R2_BUCKET=ohishi-dtako" \
    --command "archive" \
    --args "$GCLOUD_ARGS" \
    --memory 1Gi \
    --task-timeout 600s \
    --max-retries 0
fi

echo "=== Executing: archive $COMMAND $ARGS ==="
gcloud run jobs execute $ARCHIVE_JOB_NAME \
  --region $REGION \
  --project $PROJECT_ID \
  --wait

echo "=== Done ==="
