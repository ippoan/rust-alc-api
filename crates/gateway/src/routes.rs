/// public ルートかどうかを判定
///
/// src/routes/mod.rs の public_routes セクションに対応。
/// これらのルートは JWT 検証をスキップしてそのまま backend に proxy する。
pub fn is_public_route(path: &str) -> bool {
    // /api プレフィックスを除去して判定
    let api_path = path.strip_prefix("/api").unwrap_or(path);

    matches!(api_path, "/health" | "/health/")
        || api_path.starts_with("/auth/")
        || api_path.starts_with("/tenko-call/register")
        || api_path.starts_with("/tenko-call/tenko")
        || api_path.starts_with("/devices/register/request")
        || api_path.starts_with("/devices/register/status/")
        || api_path.starts_with("/devices/register/claim")
        || api_path.starts_with("/staging/")
        || api_path.starts_with("/notify/line-webhook")
        || api_path.starts_with("/notify/read/")
        || api_path.starts_with("/access-requests")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_routes() {
        // health
        assert!(is_public_route("/api/health"));
        assert!(is_public_route("/api/health/"));

        // auth (all sub-routes are public: login, callback, refresh, etc.)
        assert!(is_public_route("/api/auth/google"));
        assert!(is_public_route("/api/auth/google/code"));
        assert!(is_public_route("/api/auth/refresh"));
        assert!(is_public_route("/api/auth/lineworks/redirect"));
        assert!(is_public_route("/api/auth/lineworks/callback"));
        assert!(is_public_route("/api/auth/line/redirect"));
        assert!(is_public_route("/api/auth/tenants"));

        // tenko-call public
        assert!(is_public_route("/api/tenko-call/register"));
        assert!(is_public_route("/api/tenko-call/tenko"));

        // devices public
        assert!(is_public_route("/api/devices/register/request"));
        assert!(is_public_route("/api/devices/register/status/abc123"));
        assert!(is_public_route("/api/devices/register/claim"));

        // staging
        assert!(is_public_route("/api/staging/export"));
        assert!(is_public_route("/api/staging/import"));

        // notify public
        assert!(is_public_route("/api/notify/line-webhook"));
        assert!(is_public_route("/api/notify/read/abc"));

        // access-requests (public POST)
        assert!(is_public_route("/api/access-requests"));
    }

    #[test]
    fn test_protected_routes() {
        // JWT protected
        assert!(!is_public_route("/api/sso/configs"));
        assert!(!is_public_route("/api/bot-admin/bots"));
        assert!(!is_public_route("/api/tenant-users"));

        // Tenant protected
        assert!(!is_public_route("/api/employees"));
        assert!(!is_public_route("/api/measurements"));
        assert!(!is_public_route("/api/tenko-schedules"));
        assert!(!is_public_route("/api/devices"));
        assert!(!is_public_route("/api/car-inspections/current"));
        assert!(!is_public_route("/api/dtako-logs"));
        assert!(!is_public_route("/api/timecard/cards"));

        // devices tenant routes (not register/*)
        assert!(!is_public_route("/api/devices/pending"));
    }
}
