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
  --set-secrets "DATABASE_URL=alc-app-database-url:latest,GOOGLE_CLIENT_ID=GOOGLE_CLIENT_ID:latest,GOOGLE_CLIENT_SECRET=GOOGLE_CLIENT_SECRET:latest,JWT_SECRET=JWT_SECRET:latest,R2_ACCESS_KEY=alc-r2-access-key:latest,R2_SECRET_KEY=alc-r2-secret-key:latest,OAUTH_STATE_SECRET=alc-oauth-state-secret:latest" \
  --set-env-vars "STORAGE_BACKEND=r2,R2_BUCKET=alc-face-photos,R2_ACCOUNT_ID=24b45709d060d957340180e995f0d373,FCM_PROJECT_ID=alc-fcm,API_ORIGIN=https://rust-alc-api-747065218280.asia-northeast1.run.app" \
  --port 8080 \
  --max-instances 3

echo "=== Deploy complete ==="
SERVICE_URL=$(gcloud run services describe $SERVICE_NAME --region $REGION --format 'value(status.url)')
echo "Service URL: $SERVICE_URL"
