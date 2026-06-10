#!/bin/bash
# render.sh — Single source of truth for Cloud Run service configuration.
# Generates a Cloud Run service YAML for any service × environment combination.
#
# Usage: bash cloudrun/render.sh <service> <environment> <image_sha> [options]
#   service:     backend | gateway | tenko | carins | dtako | trouble | camera
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

# Required env vars (fed from GitHub Actions `vars.*`)
# Only the services that actually emit SCRAPER_URL need the var
if [[ "$SERVICE" == "backend" || "$SERVICE" == "dtako" ]]; then
  : "${ENV_SCRAPER_URL:?ENV_SCRAPER_URL not set (expected GitHub vars.SCRAPER_URL)}"
fi

STAGING_URL=""
DB_IMAGE=""
# production no-traffic deploy 用 (Refs #137 Phase 5 / ci-dashboard#157):
#   --pin-traffic-revision <rev>  現在 100% を受けている revision。指定すると新
#       revision は 0% で deploy され、traffic は <rev> に残る (= release は flip
#       しない、切替は Release Wave flip に委ねる)。空なら従来どおり latest に 100%。
#   --pending-tag <tag>  新 revision に付ける Cloud Run revision tag。Release Wave
#       flip が `--to-revision-tag <tag>` で切替対象にする (例: pending-v1-42-0)。
PIN_TRAFFIC_REVISION=""
PENDING_TAG=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --staging-url) STAGING_URL="$2"; shift 2 ;;
    --db-image) DB_IMAGE="$2"; shift 2 ;;
    --pin-traffic-revision) PIN_TRAFFIC_REVISION="$2"; shift 2 ;;
    --pending-tag) PENDING_TAG="$2"; shift 2 ;;
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
  camera)   SUFFIX="-camera";  BIN="alc-camera-api" ;;
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
  # Refs #218: auth-worker 側は staging / prod 共に CF Secrets Store の
  # `JWT_SECRET` 同 entry を bind しており (= 環境統合)、rust-alc-api だけ
  # staging 専用 `alc-api-staging-jwt-secret` を見ていたため必ず drift していた
  # (jwt_secret_drift probe で 401 検出)。auth-worker と環境統合 intent を
  # 揃え、prod / staging とも同じ GCP `JWT_SECRET` を見る。
  echo "JWT_SECRET"
}

notify_worker_secret_name() {
  if [[ "$ENV" == "staging" ]]; then echo "notify-worker-secret-staging"
  else echo "notify-worker-secret"; fi
}

notify_redact_broadcast_url() {
  if [[ "$ENV" == "staging" ]]; then
    echo "https://realtime.notify-staging.ippoan.org/broadcast"
  else
    echo "https://realtime.notify.ippoan.org/broadcast"
  fi
}

notify_redact_broadcast_secret_name() {
  if [[ "$ENV" == "staging" ]]; then echo "notify-redact-broadcast-secret-staging"
  else echo "notify-redact-broadcast-secret"; fi
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
            - name: NOTIFY_R2_BUCKET
              value: "$( [[ "$ENV" == "staging" ]] && echo "notify-files-staging" || echo "notify-files" )"
            - name: NOTIFY_FRONTEND_URL
              value: "$( [[ "$ENV" == "staging" ]] && echo "https://notify-staging.ippoan.org" || echo "https://notify.ippoan.org" )"
            - name: NOTIFY_REDACT_BROADCAST_URL
              value: "$(notify_redact_broadcast_url)"
            - name: NOTIFY_REDACT_2STAGE
              value: "1"
            - name: SCRAPER_URL
              value: "${ENV_SCRAPER_URL:?ENV_SCRAPER_URL not set (GitHub vars.SCRAPER_URL)}"
            - name: FCM_PROJECT_ID
              value: "alc-fcm"
            - name: STAGING_MODE
              value: "$( [[ "$ENV" == "staging" ]] && echo "true" || echo "false" )"
            - name: RUST_LOG
              value: "info"
YAML
  # staging のみ: export/import の opt-in 認証 key を注入 (Refs #391)。runtime SA
  # 747065218280-compute@ は project-level secretAccessor で ALC_STAGING_API_KEY も
  # 解決できる前提 (既存 JWT_SECRET 等と同経路)。新 revision 起動で X-Staging-Key 必須化。
  if [[ "$ENV" == "staging" ]]; then
    cat <<YAML
            - name: DATABASE_URL
              value: "${db_url}"
            - name: STAGING_API_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: ALC_STAGING_API_KEY
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
            - name: GOOGLE_DEVICE_CLIENT_ID
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: GOOGLE_DEVICE_CLIENT_ID
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
            - name: NOTIFY_R2_ACCESS_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: carins-r2-access-key
            - name: NOTIFY_R2_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: carins-r2-secret-key
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
            - name: DEVELOPER_EMAILS
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: developer-emails
            - name: SSO_ENCRYPTION_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: sso-encryption-key
            - name: NOTIFY_WORKER_SECRET
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: $(notify_worker_secret_name)
            - name: NOTIFY_REDACT_BROADCAST_SECRET
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: $(notify_redact_broadcast_secret_name)
            - name: GEMINI_API_KEY
              valueFrom:
                secretKeyRef:
                  key: latest
                  name: gemini-api-key
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
            - name: CAMERA_API_URL
              value: "PLACEHOLDER_CAMERA_URL"
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
            - name: SCRAPER_URL
              value: "${ENV_SCRAPER_URL:?ENV_SCRAPER_URL not set (GitHub vars.SCRAPER_URL)}"
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

emit_env_camera() {
  cat <<YAML
            - name: RUST_LOG
              value: "alc_camera_api=info,alc_camera=info"
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
  ENV_TROUBLE_R2_BUCKET="trouble-files-staging"
  ENV_CARINS_R2_ACCOUNT_ID="24b45709d060d957340180e995f0d373"
  ENV_DTAKO_R2_ACCOUNT_ID="24b45709d060d957340180e995f0d373"
  INGRESS="all"
  MAX_SCALE="1"
  MIN_SCALE="0"
else
  ENV_R2_BUCKET="alc-face-photos"
  ENV_CARINS_R2_BUCKET="carins-files"
  ENV_DTAKO_R2_BUCKET="ohishi-dtako"
  ENV_TROUBLE_R2_BUCKET="trouble-files"
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
  camera)  MEMORY="256Mi"; CPU="1"   ;;
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

# ---------------------------------------------------------------------------
# Traffic block (production no-traffic release)
#
# PIN_TRAFFIC_REVISION が指定されている時のみ emit する。新 revision (latest) を
# 0% + pending tag で deploy し、traffic は現行 revision に 100% 残す。これで
# `gcloud run services replace` が release で勝手にフリップするのを防ぎ、実際の
# 切替は Release Wave flip (= revision tag 指定の update-traffic) に委ねる。
#   - PIN_TRAFFIC_REVISION 空 (初回 deploy 等) → traffic block を出さない =
#     Cloud Run default の「latest に 100%」に従う (= 初回は flip 許容)。
#   - staging では指定しない (= staging は latest が常に live で良い)。
# Refs ippoan/ci-dashboard#137 Phase 5 / #157。
# ---------------------------------------------------------------------------
if [[ -n "$PIN_TRAFFIC_REVISION" ]]; then
  cat <<YAML
  traffic:
    - revisionName: ${PIN_TRAFFIC_REVISION}
      percent: 100
    - latestRevision: true
      percent: 0$( [[ -n "$PENDING_TAG" ]] && printf '\n      tag: %s' "$PENDING_TAG" )
YAML
fi
