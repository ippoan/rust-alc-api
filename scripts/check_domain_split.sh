#!/bin/bash
# ドメイン分割ガード (Refs #513)
#
# alc-core はドメイン非依存の共有基盤 (tenant / auth_middleware / storage /
# webhook generic / 共有 models) に限定する。ドメイン専用の repository trait /
# models / AppState field が alc-core に「再流入」したら CI を loud fail させる。
#
# 分割が進むたび (Phase B: trouble, Phase C: dtako/notify/carins) に
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

# --- tenko (Phase A) ---
guard_repository_modules tenko "crates/alc-tenko/src/repository/" \
  tenko_call tenko_records tenko_schedules tenko_sessions tenko_webhooks \
  daily_health health_baselines equipment_failures driver_info
guard_model_structs tenko "crates/alc-tenko/src/models.rs" \
  'Tenko|CreateTenkoSchedule|UpdateTenkoSchedule|EmployeeHealthBaseline|CreateHealthBaseline|UpdateHealthBaseline|EquipmentFailure|CreateEquipmentFailure|UpdateEquipmentFailure|SelfDeclaration|SubmitSelfDeclaration|SafetyJudgment'
guard_appstate_fields tenko "alc_tenko::TenkoState" \
  'tenko_call|tenko_records|tenko_schedules|tenko_sessions|tenko_webhooks|daily_health|health_baselines|equipment_failures|driver_info'

# --- trouble (Phase B) ---
guard_repository_modules trouble "crates/alc-trouble/src/repository/" \
  trouble_tickets trouble_files trouble_workflow trouble_categories trouble_offices \
  trouble_progress_statuses trouble_notification_prefs trouble_schedules trouble_tasks \
  trouble_task_types trouble_task_statuses trouble_field_layouts
guard_model_structs trouble "crates/alc-trouble/src/models.rs" \
  'Trouble|CreateTrouble|UpdateTrouble|CreateWorkflowState|CreateWorkflowTransition|TransitionRequest|CreateCustomFieldDef|UpsertNotificationPref'
guard_appstate_fields trouble "alc_trouble::TroubleState" \
  'trouble_tickets|trouble_files|trouble_workflow|trouble_categories|trouble_offices|trouble_progress_statuses|trouble_notification_prefs|trouble_schedules|trouble_tasks|trouble_task_types|trouble_task_statuses|trouble_field_layouts|trouble_storage'

# --- (Phase C 以降はここに追記: dtako / notify / carins) ---

if [ "$fail" != 0 ]; then
  echo "::error::domain split guard failed — ドメインコードは alc-core ではなく各ドメイン crate に追加してください (設計: issue #513)"
  exit 1
fi
echo "domain split guard OK (tenko, trouble)"
