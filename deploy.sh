#!/bin/bash
set -e

PROJECT_ID="cloudsql-sv"
REGION="asia-northeast1"
SERVICE_NAME="rust-alc-api"
REPOSITORY="alc-app"
IMAGE="$REGION-docker.pkg.dev/$PROJECT_ID/$REPOSITORY/$SERVICE_NAME"

echo "=== Building Docker image ==="
docker build -t $IMAGE:latest .

echo "=== Pushing to Artifact Registry ==="
docker push $IMAGE:latest

echo "=== Deploying to Cloud Run ==="
gcloud run deploy $SERVICE_NAME \
  --image $IMAGE:latest \
  --region $REGION \
  --platform managed \
  --allow-unauthenticated \
  --add-cloudsql-instances cloudsql-sv:asia-northeast1:postgres-prod \
  --set-secrets "DATABASE_URL=alc-app-database-url:latest,GOOGLE_CLIENT_ID=GOOGLE_CLIENT_ID:latest,JWT_SECRET=JWT_SECRET:latest" \
  --set-env-vars "GCS_BUCKET=alc-face-photos" \
  --port 8080 \
  --max-instances 3

echo "=== Deploy complete ==="
SERVICE_URL=$(gcloud run services describe $SERVICE_NAME --region $REGION --format 'value(status.url)')
echo "Service URL: $SERVICE_URL"
