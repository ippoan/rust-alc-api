-- 既存テナントの trouble ワークフローに「各非 terminal 状態 → terminal 状態」
-- への遷移を冪等にバックフィルする。
--
-- 背景: デフォルトワークフロー (新規→対応中→解決→完了) では「完了」へは
-- 「解決」からしか遷移できず、新規/対応中のチケットを直接「完了」にできない。
-- `POST /api/trouble/tickets/{id}/transition` が is_transition_allowed=false で
-- 422 を返してしまう。これを「任意の非 terminal 状態 → terminal へ遷移可能」に
-- 揃える (Refs ippoan/nuxt-trouble#122)。
--
-- 新規テナントは setup_defaults (crates/alc-trouble/src/repo/trouble_workflow.rs)
-- の transitions 配列に (new→closed) / (in_progress→closed) を追加済みなので
-- 最初から任意→完了可。本 migration は既存テナント分の補填。
--
-- RLS: trouble_workflow_transitions は ENABLE ROW LEVEL SECURITY のみ
-- (FORCE 無し) なので、テーブル owner (= migration 実行ロール) は RLS を
-- bypass し全テナント横断で INSERT できる。
-- UNIQUE(tenant_id, from_state_id, to_state_id) + NOT EXISTS で冪等性を担保。

INSERT INTO alc_api.trouble_workflow_transitions (tenant_id, from_state_id, to_state_id, label)
SELECT s.tenant_id, s.id, t.id, '完了'
FROM alc_api.trouble_workflow_states s
JOIN alc_api.trouble_workflow_states t
  ON t.tenant_id = s.tenant_id AND t.is_terminal = TRUE
WHERE s.is_terminal = FALSE
  AND NOT EXISTS (
    SELECT 1 FROM alc_api.trouble_workflow_transitions x
    WHERE x.tenant_id = s.tenant_id
      AND x.from_state_id = s.id
      AND x.to_state_id = t.id
  );
