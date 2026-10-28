#!/bin/bash
# ドメイン分割ガード (Refs #513)
#
# alc-core はドメイン非依存の共有基盤 (tenant / auth_middleware / storage /
# 共有 models) に限定する。ドメイン専用の repository trait /
# models / AppState field が alc-core に「再流入」したら CI を loud fail させる。
#
# 分割が進むたび (Phase B: trouble (後に Worker へ移設), Phase C: dtako/notify/carins) に
# 下のドメイン節を追記すること。誤検知した場合はパターンを絞って直す
# (ガードを丸ごと外さない)。
set -u

fail=0
err() {
  echo "::error::$1"
  fail=1
}

guard_repository_modules() {
  # $1: domain 名 / $2: 移設先 / $3...: module 名
  local domain="$1" dest="$2"
  shift 2
  for m in "$@"; do
    if [ -e "crates/alc-core/src/repository/${m}.rs" ]; then
      err "domain-split: alc-core に ${domain} ドメインの repository (${m}.rs) が再流入しています。${dest} に置いてください (Refs #513)"
    fi
  done
}

guard_model_structs() {
  # $1: domain 名 / $2: 移設先 / $3: struct/enum 名 prefix の ERE
  local domain="$1" dest="$2" pattern="$3"
  local hits
  hits=$(grep -nE "pub (struct|enum) (${pattern})" crates/alc-core/src/models.rs || true)
  if [ -n "$hits" ]; then
    echo "$hits"
    err "domain-split: alc-core/src/models.rs に ${domain} ドメインの model が再流入しています。${dest} に置いてください (Refs #513)"
  fi
}

guard_appstate_fields() {
  # $1: domain 名 / $2: 代替 State / $3: field 名の ERE
  local domain="$1" state="$2" pattern="$3"
  local hits
  hits=$(grep -nE "pub (${pattern}):" crates/alc-core/src/lib.rs || true)
  if [ -n "$hits" ]; then
    echo "$hits"
    err "domain-split: AppState に ${domain} ドメインの field が再流入しています。${state} (with_state マウント) を使ってください (Refs #513)"
  fi
}

# --- tenko (Worker へ移した。W7a〜W7d、Refs #680) ---
# 点呼の口 (/api/tenko・/api/tenko-call) と予定超過の確認は全部 ippoan/alc-tenko-worker に移り、点呼の crate は消した。
# 点呼の webhook (alc-core の webhook.rs・WebhookRepository・AppState の欄) も使い手が無くなったので一緒に消した。
# crate の再作成と、alc-core への点呼の repository / model / webhook / AppState の欄の再流入を落とす。
if [ -e crates/alc-tenko ]; then
  err "domain-split: 点呼の crate が再作成されています。点呼は ippoan/alc-tenko-worker に置いてください (Refs #680)"
fi
if [ -e crates/alc-core/src/webhook.rs ]; then
  err "domain-split: alc-core に点呼の webhook.rs が再流入しています。ippoan/alc-tenko-worker に置いてください (Refs #680)"
fi
guard_repository_modules tenko "ippoan/alc-tenko-worker" \
  tenko_call tenko_records tenko_schedules tenko_sessions tenko_webhooks \
  daily_health health_baselines equipment_failures driver_info webhook
guard_model_structs tenko "ippoan/alc-tenko-worker" \
  'Tenko|CreateTenkoSchedule|UpdateTenkoSchedule|EmployeeHealthBaseline|CreateHealthBaseline|UpdateHealthBaseline|EquipmentFailure|CreateEquipmentFailure|UpdateEquipmentFailure|SelfDeclaration|SubmitSelfDeclaration|SafetyJudgment|Webhook|CreateWebhook'
guard_appstate_fields tenko "ippoan/alc-tenko-worker" \
  'tenko_call|tenko_records|tenko_schedules|tenko_sessions|tenko_webhooks|daily_health|health_baselines|equipment_failures|driver_info|webhook'

# --- trouble (Worker へ移した。Refs #747) ---
# trouble は全部 ippoan/alc-trouble-worker に移り、alc-trouble crate は消した。
# crate の再作成と、alc-core への trouble の repository / model / AppState field の
# 再流入を落とす。
if [ -e crates/alc-trouble ]; then
  err "domain-split: crates/alc-trouble が再作成されています。trouble は ippoan/alc-trouble-worker に置いてください (Refs #747)"
fi
guard_repository_modules trouble "ippoan/alc-trouble-worker" \
  trouble_tickets trouble_files trouble_workflow trouble_categories trouble_offices \
  trouble_progress_statuses trouble_notification_prefs trouble_schedules trouble_tasks \
  trouble_task_types trouble_task_statuses trouble_field_layouts
guard_model_structs trouble "ippoan/alc-trouble-worker" \
  'Trouble|CreateTrouble|UpdateTrouble|CreateWorkflowState|CreateWorkflowTransition|TransitionRequest|CreateCustomFieldDef|UpsertNotificationPref'
guard_appstate_fields trouble "ippoan/alc-trouble-worker" \
  'trouble_tickets|trouble_files|trouble_workflow|trouble_categories|trouble_offices|trouble_progress_statuses|trouble_notification_prefs|trouble_schedules|trouble_tasks|trouble_task_types|trouble_task_statuses|trouble_field_layouts|trouble_storage'

# --- notify (Worker へ移した口。Refs #747) ---
# 宛先・グループ・LINE の設定の口は ippoan/alc-notify-worker に移った。
# Cloud Run に残る notify の口 (文書・ingest・viewer・LINE の webhook 等) は使わない。
guard_repository_modules notify "ippoan/alc-notify-worker" \
  notify_groups notify_line_config
guard_appstate_fields notify "ippoan/alc-notify-worker" \
  'notify_groups|notify_line_config'

# --- dtako (Worker へ移した。Refs #680) ---
# dtako の口は全部 ippoan/alc-dtako-worker に移り (最後が W5c-5 の起票 dtako_tickets)、alc-dtako crate は消した。
# crate の再作成と、alc-core への起票の repository / model / AppState field の再流入を落とす
# (alc-core に残る Dtako の型 = DtakologRow・DtakoDailyWorkHours 等はアーカイブの Job と点呼が使う)。
if [ -e crates/alc-dtako ]; then
  err "domain-split: crates/alc-dtako が再作成されています。dtako は ippoan/alc-dtako-worker に置いてください (Refs #680)"
fi
guard_repository_modules dtako "ippoan/alc-dtako-worker" dtako_tickets
guard_model_structs dtako "ippoan/alc-dtako-worker" 'DtakoTicket'
guard_appstate_fields dtako "ippoan/alc-dtako-worker" 'dtako_tickets|dtako_storage'

# --- hub (端末の測定・打刻。Worker へ移した。W6a、Refs #680) ---
# `/api/hub/measurements` は ippoan/alc-people-worker に移り、打刻の照合のためだけに残していた timecard の
# repository も消した。alc-core への repository / model / AppState field の再流入を落とす。
guard_repository_modules hub "ippoan/alc-people-worker" hub_measurements timecard
guard_model_structs hub "ippoan/alc-people-worker" 'HubMeasurement|TimecardCard'
guard_appstate_fields hub "ippoan/alc-people-worker" 'hub_measurements|timecard'

# --- devices (Worker へ移した。W6b、Refs #680) ---
# 端末の登録・管理・再ペアリング・FCM の口は全部 ippoan/alc-devices-worker に移り、alc-devices crate は消した。
# crate の再作成と、alc-core への devices の repository・FCM・再ペアリングの client・AppState の欄の再流入を落とす。
if [ -e crates/alc-devices ]; then
  err "domain-split: crates/alc-devices が再作成されています。devices は ippoan/alc-devices-worker に置いてください (Refs #680)"
fi
guard_repository_modules devices "ippoan/alc-devices-worker" devices
for m in fcm device_pair_client; do
  if [ -e "crates/alc-core/src/${m}.rs" ]; then
    err "domain-split: alc-core に devices の ${m}.rs が再流入しています。ippoan/alc-devices-worker に置いてください (Refs #680)"
  fi
done
guard_appstate_fields devices "ippoan/alc-devices-worker" 'devices|fcm|device_pair_client'

# --- 会社の管理とログインの口 (Worker へ移した。W2b・W2c、Refs #680) ---
# me・logout・my-orgs (alc-auth crate) と、ユーザー・招待・SSO 設定・参加の申請・テナントの slug は ippoan/alc-auth-worker、
# Bot 設定の一覧・保存・削除は ippoan/alc-lineworks-worker に移った (rust に残るのは開発者だけの Bot 設定の export)。
# crate の再作成と、alc-core への repository・AppState の欄の再流入を落とす。
if [ -e crates/alc-auth ]; then
  err "domain-split: crates/alc-auth が再作成されています。me・logout・my-orgs は ippoan/alc-auth-worker に置いてください (Refs #680)"
fi
guard_repository_modules admin "ippoan/alc-auth-worker" sso_admin tenant_users
guard_appstate_fields admin "ippoan/alc-auth-worker" 'sso_admin|tenant_users'

# --- carins・maintenance (Worker へ移した。W3、Refs #680) ---
# 車検証・ファイル・NFC タグ・車両整備の口は全部 ippoan/alc-carins-worker に移り、alc-carins・alc-maintenance crate は消した。
# crate の再作成と、alc-core への carins の repository / model / AppState field の再流入を落とす。
for c in alc-carins alc-maintenance; do
  if [ -e "crates/$c" ]; then
    err "domain-split: crates/$c が再作成されています。carins・maintenance は ippoan/alc-carins-worker に置いてください (Refs #680)"
  fi
done
guard_repository_modules carins "ippoan/alc-carins-worker" car_inspections carins_files nfc_tags
guard_model_structs carins "ippoan/alc-carins-worker" "NfcTag|CarInspection"
guard_appstate_fields carins "ippoan/alc-carins-worker" "car_inspections|carins_files|nfc_tags|carins_storage"

if [ "$fail" != 0 ]; then
  echo "::error::domain split guard failed — ドメインコードは alc-core ではなく各ドメイン crate に追加してください (設計: issue #513)"
  exit 1
fi
echo "domain split guard OK (tenko, trouble, notify, dtako, hub, devices, admin, carins)"
