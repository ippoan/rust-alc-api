//! kiosk 端末 re-pair (再認証) の判定ロジック (Refs #495)。
//!
//! DB / ネットワーク非依存の pure fn。設計 SoT: `docs/plan-device-repair.md`。
//! 判定順序: status → admin window → cooldown → TOFU hardware bind →
//! (任意) settings_token co-factor。

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

/// `evaluate_re_pair_request` への入力 (DB から取得した現在状態)。
#[derive(Debug, Clone)]
pub struct RePairContext {
    pub status: String,
    pub authorized_until: Option<DateTime<Utc>>,
    pub last_re_pair_at: Option<DateTime<Utc>>,
    pub bound_hardware_id: Option<String>,
    pub requested_hardware_id: Option<String>,
    pub provided_settings_token: Option<Uuid>,
    pub stored_settings_token: Option<Uuid>,
}

/// hardening flag / 閾値 (env var から解決される)。
#[derive(Debug, Clone, Copy)]
pub struct RePairPolicy {
    pub require_admin_window: bool,
    pub require_settings_token: bool,
    pub cooldown_secs: i64,
}

/// 判定結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RePairDecision {
    /// 許可。`bind_hardware_id` が Some なら今回のリクエストの hardware_id を
    /// 初回 bind として記録する必要がある。
    Allow { bind_hardware_id: Option<String> },
    /// 拒否 (理由非開示、呼び出し元は 404 を返す)。
    DenyNotFound,
    /// cooldown 中 (呼び出し元は 429 を返す)。
    DenyTooManyRequests,
}

/// re-pair リクエストを評価する。
pub fn evaluate_re_pair_request(
    ctx: &RePairContext,
    policy: &RePairPolicy,
    now: DateTime<Utc>,
) -> RePairDecision {
    if ctx.status != "active" {
        return RePairDecision::DenyNotFound;
    }

    if policy.require_admin_window {
        match ctx.authorized_until {
            Some(until) if until > now => {}
            _ => return RePairDecision::DenyNotFound,
        }
    }

    if let Some(last) = ctx.last_re_pair_at {
        if now - last < Duration::seconds(policy.cooldown_secs) {
            return RePairDecision::DenyTooManyRequests;
        }
    }

    let bind_hardware_id = match (&ctx.bound_hardware_id, &ctx.requested_hardware_id) {
        (Some(bound), Some(requested)) if bound != requested => {
            return RePairDecision::DenyNotFound;
        }
        (None, Some(requested)) => Some(requested.clone()),
        _ => None,
    };

    if policy.require_settings_token {
        if let Some(provided) = &ctx.provided_settings_token {
            match &ctx.stored_settings_token {
                Some(stored) if stored == provided => {}
                _ => return RePairDecision::DenyNotFound,
            }
        }
        // 未提示は許可 (ratchet: 成功時に呼び出し元が rotate する)
    }

    RePairDecision::Allow { bind_hardware_id }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_ctx() -> RePairContext {
        RePairContext {
            status: "active".to_string(),
            authorized_until: None,
            last_re_pair_at: None,
            bound_hardware_id: None,
            requested_hardware_id: None,
            provided_settings_token: None,
            stored_settings_token: None,
        }
    }

    fn base_policy() -> RePairPolicy {
        RePairPolicy {
            require_admin_window: true,
            require_settings_token: false,
            cooldown_secs: 600,
        }
    }

    #[test]
    fn denies_when_status_not_approved() {
        let mut ctx = base_ctx();
        ctx.status = "disabled".to_string();
        ctx.authorized_until = Some(Utc::now() + Duration::minutes(10));
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), Utc::now());
        assert_eq!(decision, RePairDecision::DenyNotFound);
    }

    #[test]
    fn denies_when_window_absent() {
        let ctx = base_ctx();
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), Utc::now());
        assert_eq!(decision, RePairDecision::DenyNotFound);
    }

    #[test]
    fn denies_when_window_expired() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now - Duration::seconds(1));
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), now);
        assert_eq!(decision, RePairDecision::DenyNotFound);
    }

    #[test]
    fn allows_when_window_active() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), now);
        assert_eq!(
            decision,
            RePairDecision::Allow {
                bind_hardware_id: None
            }
        );
    }

    #[test]
    fn skips_window_check_when_admin_not_required() {
        let ctx = base_ctx();
        let mut policy = base_policy();
        policy.require_admin_window = false;
        let decision = evaluate_re_pair_request(&ctx, &policy, Utc::now());
        assert_eq!(
            decision,
            RePairDecision::Allow {
                bind_hardware_id: None
            }
        );
    }

    #[test]
    fn denies_too_many_requests_within_cooldown() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        ctx.last_re_pair_at = Some(now - Duration::seconds(1));
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), now);
        assert_eq!(decision, RePairDecision::DenyTooManyRequests);
    }

    #[test]
    fn allows_after_cooldown_elapsed() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        ctx.last_re_pair_at = Some(now - Duration::seconds(601));
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), now);
        assert_eq!(
            decision,
            RePairDecision::Allow {
                bind_hardware_id: None
            }
        );
    }

    #[test]
    fn binds_hardware_id_on_first_request() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        ctx.requested_hardware_id = Some("hw-1".to_string());
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), now);
        assert_eq!(
            decision,
            RePairDecision::Allow {
                bind_hardware_id: Some("hw-1".to_string())
            }
        );
    }

    #[test]
    fn allows_matching_bound_hardware_id() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        ctx.bound_hardware_id = Some("hw-1".to_string());
        ctx.requested_hardware_id = Some("hw-1".to_string());
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), now);
        assert_eq!(
            decision,
            RePairDecision::Allow {
                bind_hardware_id: None
            }
        );
    }

    #[test]
    fn denies_mismatched_bound_hardware_id() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        ctx.bound_hardware_id = Some("hw-1".to_string());
        ctx.requested_hardware_id = Some("hw-2".to_string());
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), now);
        assert_eq!(decision, RePairDecision::DenyNotFound);
    }

    #[test]
    fn allows_when_hardware_id_not_requested_even_if_bound() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        ctx.bound_hardware_id = Some("hw-1".to_string());
        let decision = evaluate_re_pair_request(&ctx, &base_policy(), now);
        assert_eq!(
            decision,
            RePairDecision::Allow {
                bind_hardware_id: None
            }
        );
    }

    #[test]
    fn settings_token_not_provided_is_allowed_when_required() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        ctx.stored_settings_token = Some(Uuid::new_v4());
        let mut policy = base_policy();
        policy.require_settings_token = true;
        let decision = evaluate_re_pair_request(&ctx, &policy, now);
        assert_eq!(
            decision,
            RePairDecision::Allow {
                bind_hardware_id: None
            }
        );
    }

    #[test]
    fn settings_token_mismatch_denied_when_required() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        ctx.stored_settings_token = Some(Uuid::new_v4());
        ctx.provided_settings_token = Some(Uuid::new_v4());
        let mut policy = base_policy();
        policy.require_settings_token = true;
        let decision = evaluate_re_pair_request(&ctx, &policy, now);
        assert_eq!(decision, RePairDecision::DenyNotFound);
    }

    #[test]
    fn settings_token_match_allowed_when_required() {
        let mut ctx = base_ctx();
        let now = Utc::now();
        let token = Uuid::new_v4();
        ctx.authorized_until = Some(now + Duration::minutes(5));
        ctx.stored_settings_token = Some(token);
        ctx.provided_settings_token = Some(token);
        let mut policy = base_policy();
        policy.require_settings_token = true;
        let decision = evaluate_re_pair_request(&ctx, &policy, now);
        assert_eq!(
            decision,
            RePairDecision::Allow {
                bind_hardware_id: None
            }
        );
    }
}
