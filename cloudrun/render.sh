#!/bin/bash
# render.sh — Single source of truth for Cloud Run service configuration.
# Generates a Cloud Run service YAML for any service × environment combination.
#
# Usage: bash cloudrun/render.sh <service> <environment> <image_sha> [options]
#   service:     backend | gateway | tenko | carins | dtako
#   environment: staging | production
#   image_sha:   Docker image SHA tag
#
# Options:
#   --staging-url <url>   Staging API URL (staging only)
#   --db-image <image>    PostgreSQL sidecar image (staging only)
#
# Output: Cloud Run service YAML to stdout
set -euo pipefail

SERVICE="${1:?Usage: render.sh <service> <environment> <image_sha>}"
ENV="${2:?Usage: render.sh <service> <environment> <image_sha>}"
IMAGE_SHA="${3:?Usage: render.sh <service> <environment> <image_sha>}"
shift 3

STAGING_URL=""
DB_IMAGE=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --staging-url) STAGING_URL="$2"; shift 2 ;;
    --db-image) DB_IMAGE="$2"; shift 2 ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

REPO="ghcr.io/ippoan/rust-alc-api"
AR_PREFIX="asia-northeast1-docker.pkg.dev/cloudsql-sv/ghcr"
REGION="asia-northeast1"

# ---------------------------------------------------------------------------
# Service name and image
# ---------------------------------------------------------------------------
case "$SERVICE" in
  backend)  SUFFIX="";         BIN="rust-alc-api" ;;
  gateway)  SUFFIX="-gateway"; BIN="gateway" ;;
  tenko)    SUFFIX="-tenko";   BIN="tenko-api" ;;
  carins)   SUFFIX="-carins";  BIN="carins-api" ;;
  dtako)    SUFFIX="-dtako";   BIN="dtako-api" ;;
  trouble)  SUFFIX="-trouble"; BIN="trouble-api" ;;
  *) echo "Unknown service: $SERVICE" >&2; exit 1 ;;
esac

if [[ "$ENV" == "staging" ]]; then
  SERVICE_NAME="rust-alc-api-staging${SUFFIX}"
  # Gateway has no sidecar, so use the production image directly
  if [[ "$SERVICE" == "gateway" ]]; then
    IMAGE="${AR_PREFIX}/${REPO}${SUFFIX}:${IMAGE_SHA}"
  else
    IMAGE="${AR_PREFIX}/${REPO}${SUFFIX}-staging:${IMAGE_SHA}"
  fi
else
  SERVICE_NAME="rust-alc-api${SUFFIX}"
  IMAGE="${AR_PREFIX}/${REPO}${SUFFIX}:${IMAGE_SHA}"
fi

# ---------------------------------------------------------------------------
# Shared secrets (same Secret Manager names for staging and production)
# ---------------------------------------------------------------------------
jwt_secret_name() {
  if [[ "$ENV" == "staging" ]]; then echo "alc-api-staging-jwt-secret"
  else echo "JWT_SECRET"; fi
}

# ---------------------------------------------------------------------------
# Per-service env vars and secrets — THE SINGLE SOURCE OF TRUTH
# ---------------------------------------------------------------------------
emit_env_backend() {
  local db_url
  if [[ "$ENV" == "staging" ]]; then
    db_url="postgresql://postgres:staging@localhost:5432/postgres?options=-c search_path=alc_api"
  fi

  cat <<YAML
            - name: STORAGE_BACKEND
              value: "r2"
            - name: R2_BUCKET
              value: "${ENV_R2_BUCKET:-alc-face-photos}"
            - name: R2_ACCOUNT_ID
              value: "24b45709d060d957340180e995f0d373"
            - name: API_ORIGIN
              value: "${STAGING_URL:-https://alc-api.ippoan.org}"
            - name: CARINS_R2_BUCKET
              value: "${ENV_CARINS_R2_BUCKET:-carins-files}"
            - name: DTAKO_R2_BUCKET
              value: "${ENV_DTAKO_R2_BUCKET:-ohishi-dtako}"
            - name: FCM_PROJECT_ID
              value: "alc-fcm"
            - name: STAGING_MODE
              value: "$( [[ "$ENV" == "staging" ]] && echo "true" || echo "false" )"
            - name: RUST_LOG
              value: "info"
YAML
  if [[ "$ENV" == "staging" ]]; then
    cat <<YAML
            - name: DATABASE_URL
              value: "${db_url}"
YAML
  else
    cat <<YAML
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: alc-app-database-url
YAML
  fi
  cat <<YAML
            - name: JWT_SECRET
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: $(jwt_secret_name)
            - name: GOOGLE_CLIENT_ID
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: GOOGLE_CLIENT_ID
            - name: GOOGLE_CLIENT_SECRET
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: GOOGLE_CLIENT_SECRET
            - name: R2_ACCESS_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: alc-r2-access-key
            - name: R2_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: alc-r2-secret-key
            - name: OAUTH_STATE_SECRET
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: alc-oauth-state-secret
            - name: CARINS_R2_ACCESS_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: carins-r2-access-key
            - name: CARINS_R2_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: carins-r2-secret-key
            - name: DTAKO_R2_ACCESS_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: dtako-r2-access-key
            - name: DTAKO_R2_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: dtako-r2-secret-key
            - name: TROUBLE_R2_BUCKET
              value: "${ENV_TROUBLE_R2_BUCKET:-trouble-files}"
            - name: TROUBLE_R2_ACCESS_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: trouble-r2-access-key
            - name: TROUBLE_R2_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: trouble-r2-secret-key
            - name: LINE_LOGIN_CHANNEL_ID
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: line-login-channel-id
            - name: LINE_LOGIN_CHANNEL_SECRET
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: line-login-channel-secret
YAML
}

emit_env_gateway() {
  # Gateway URLs differ: staging uses Cloud Run service URLs, production discovers them
  cat <<YAML
            - name: BACKEND_URL
              value: "PLACEHOLDER_BACKEND_URL"
            - name: TENKO_API_URL
              value: "PLACEHOLDER_TENKO_URL"
            - name: CARINS_API_URL
              value: "PLACEHOLDER_CARINS_URL"
            - name: DTAKO_API_URL
              value: "PLACEHOLDER_DTAKO_URL"
            - name: TROUBLE_API_URL
              value: "PLACEHOLDER_TROUBLE_URL"
            - name: JWT_SECRET
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: $(jwt_secret_name)
            - name: RUST_LOG
              value: "gateway=info,tower_http=info"
YAML
}

emit_env_tenko() {
  cat <<YAML
            - name: RUST_LOG
              value: "tenko_api=info"
YAML
  emit_database_url
}

emit_env_carins() {
  cat <<YAML
            - name: STORAGE_BACKEND
              value: "r2"
            - name: CARINS_R2_BUCKET
              value: "${ENV_CARINS_R2_BUCKET:-rust-logi-files}"
            - name: CARINS_R2_ACCOUNT_ID
              value: "${ENV_CARINS_R2_ACCOUNT_ID:-8556e484b273a868db8ec6800b074834}"
            - name: CARINS_R2_ACCESS_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: carins-r2-access-key
            - name: CARINS_R2_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: carins-r2-secret-key
            - name: RUST_LOG
              value: "carins_api=info"
YAML
  emit_database_url
}

emit_env_dtako() {
  cat <<YAML
            - name: STORAGE_BACKEND
              value: "r2"
            - name: DTAKO_R2_BUCKET
              value: "${ENV_DTAKO_R2_BUCKET:-ohishi-dtako}"
            - name: DTAKO_R2_ACCOUNT_ID
              value: "${ENV_DTAKO_R2_ACCOUNT_ID:-8556e484b273a868db8ec6800b074834}"
            - name: DTAKO_R2_ACCESS_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: dtako-r2-access-key
            - name: DTAKO_R2_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: dtako-r2-secret-key
            - name: RUST_LOG
              value: "dtako_api=info"
YAML
  emit_database_url
}

emit_env_trouble() {
  cat <<YAML
            - name: RUST_LOG
              value: "trouble_api=info"
            - name: R2_ACCOUNT_ID
              value: "24b45709d060d957340180e995f0d373"
            - name: TROUBLE_R2_BUCKET
              value: "${ENV_TROUBLE_R2_BUCKET:-trouble-files}"
            - name: TROUBLE_R2_ACCESS_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: trouble-r2-access-key
            - name: TROUBLE_R2_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: trouble-r2-secret-key
YAML
  emit_database_url
}

# Shared helper: emit DATABASE_URL (staging=localhost, production=Secret Manager)
emit_database_url() {
  if [[ "$ENV" == "staging" ]]; then
    cat <<YAML
            - name: DATABASE_URL
              value: "postgresql://postgres:staging@localhost:5432/postgres?options=-c search_path=alc_api"
YAML
  else
    cat <<YAML
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: alc-app-database-url
YAML
  fi
}

# ---------------------------------------------------------------------------
# Environment-specific values
# ---------------------------------------------------------------------------
if [[ "$ENV" == "staging" ]]; then
  ENV_R2_BUCKET="alc-face-photos-staging"
  ENV_CARINS_R2_BUCKET="carins-files-staging"
  ENV_DTAKO_R2_BUCKET="ohishi-dtako-staging"
  ENV_CARINS_R2_ACCOUNT_ID="24b45709d060d957340180e995f0d373"
  ENV_DTAKO_R2_ACCOUNT_ID="24b45709d060d957340180e995f0d373"
  INGRESS="all"
  MAX_SCALE="1"
  MIN_SCALE="0"
else
  ENV_R2_BUCKET="alc-face-photos"
  ENV_CARINS_R2_BUCKET="rust-logi-files"
  ENV_DTAKO_R2_BUCKET="ohishi-dtako"
  ENV_CARINS_R2_ACCOUNT_ID="8556e484b273a868db8ec6800b074834"
  ENV_DTAKO_R2_ACCOUNT_ID="8556e484b273a868db8ec6800b074834"
  INGRESS="internal"
  MAX_SCALE="5"
  MIN_SCALE="0"
fi

# Resource limits per service
case "$SERVICE" in
  backend) MEMORY="512Mi"; CPU="1"   ;;
  gateway) MEMORY="256Mi"; CPU="1"   ;;
  tenko)   MEMORY="256Mi"; CPU="1"   ;;
  carins)  MEMORY="256Mi"; CPU="1"   ;;
  dtako)   MEMORY="512Mi"; CPU="1"   ;;
  trouble) MEMORY="256Mi"; CPU="1"   ;;
esac

# Port
case "$SERVICE" in
  gateway) PORT="8080" ;;
  *)       PORT="8080" ;;
esac

# Health check path
case "$SERVICE" in
  backend) HEALTH_PATH="/api/health" ;;
  *)       HEALTH_PATH="/health" ;;
esac

# Gateway and backend are public, others are internal
if [[ "$SERVICE" == "gateway" || "$SERVICE" == "backend" ]]; then
  INGRESS="all"
fi

# ---------------------------------------------------------------------------
# Generate YAML
# ---------------------------------------------------------------------------

# Sidecar annotations
SIDECAR_ANNOTATIONS=""
if [[ "$ENV" == "staging" && "$SERVICE" != "gateway" ]]; then
  SIDECAR_ANNOTATIONS="
        run.googleapis.com/container-dependencies: '{\"app\":[\"postgres\"]}'"
fi

LAUNCH_STAGE=""
if [[ "$ENV" == "staging" && "$SERVICE" != "gateway" ]]; then
  LAUNCH_STAGE="
    run.googleapis.com/launch-stage: BETA"
fi

cat <<YAML
apiVersion: serving.knative.dev/v1
kind: Service
metadata:
  name: ${SERVICE_NAME}
  labels:
    cloud.googleapis.com/location: ${REGION}
  annotations:${LAUNCH_STAGE}
    run.googleapis.com/ingress: ${INGRESS}
spec:
  template:
    metadata:
      annotations:${SIDECAR_ANNOTATIONS}
        autoscaling.knative.dev/maxScale: "${MAX_SCALE}"
        autoscaling.knative.dev/minScale: "${MIN_SCALE}"
    spec:
      containerConcurrency: 80
      timeoutSeconds: 300
      containers:
        - name: app
          image: ${IMAGE}
          ports:
            - containerPort: ${PORT}
          env:
$(emit_env_${SERVICE})
          resources:
            limits:
              memory: ${MEMORY}
              cpu: "${CPU}"
          startupProbe:
            httpGet:
              path: ${HEALTH_PATH}
              port: ${PORT}
            initialDelaySeconds: 3
            periodSeconds: 2
            failureThreshold: 15
YAML

# Sidecar container (staging only, not for gateway)
if [[ "$ENV" == "staging" && "$SERVICE" != "gateway" ]]; then
  cat <<YAML
        - name: postgres
          image: ${DB_IMAGE}
          env:
            - name: POSTGRES_PASSWORD
              value: "staging"
            - name: POSTGRES_HOST_AUTH_METHOD
              value: "trust"
          resources:
            limits:
              memory: 512Mi
              cpu: "1"
          startupProbe:
            tcpSocket:
              port: 5432
            initialDelaySeconds: 2
            periodSeconds: 2
            failureThreshold: 15
          volumeMounts:
            - name: pg-data
              mountPath: /var/lib/postgresql/data
      volumes:
        - name: pg-data
          emptyDir:
            sizeLimit: 1Gi
YAML
fi
