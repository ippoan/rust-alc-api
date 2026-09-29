//! staging の到達面の栓 (Refs #691)。
//!
//! staging の Worker はテストから叩くため `workers_dev = true` で公開している。この Worker は
//! JWT を検証せず `X-Tenant-ID` を信頼するので、素で公開すると #556 と同じ穴になる。そこで
//! secret `STAGING_TEST_SECRET` が定義されている env では、同じ値の `X-Staging-Test-Secret`
//! ヘッダーが無いリクエストを最前段ですべて断る。
//!
//! var `STAGING_GATE = "required"` の env (staging) で secret を入れ忘れたら、全部断る
//! (開けっ放しにしない)。どちらも無い env (本番) では何もしない — 本番は公開せず
//! (`workers_dev = false`、route 無し)、auth-worker の Service Binding からしか届かない。
//! `workers_dev = true` の env に必ずこの 2 つの設定があることは
//! scripts/check-exposure.sh が CI で検査する。

use axum::http::HeaderMap;
use worker::Env;

pub const SECRET_BINDING: &str = "STAGING_TEST_SECRET";
pub const HEADER: &str = "x-staging-test-secret";
const GATE_VAR: &str = "STAGING_GATE";

pub fn allows(env: &Env, headers: &HeaderMap) -> bool {
    match env.secret(SECRET_BINDING) {
        Ok(secret) => check(&secret.to_string(), headers),
        Err(_) => !env.var(GATE_VAR).is_ok_and(|v| v.to_string() == "required"),
    }
}

fn check(secret: &str, headers: &HeaderMap) -> bool {
    // 空の secret で全開きにならないように
    if secret.is_empty() {
        return false;
    }
    match headers.get(HEADER) {
        Some(v) => constant_time_eq(v.as_bytes(), secret.as_bytes()),
        None => false,
    }
}

/// 長さ以外の情報を比較時間から漏らさない比較 (一致する接頭辞の長さで早抜けしない)
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(v: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(v) = v {
            h.insert(HEADER, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn matches_only_exact_secret() {
        assert!(check("s3cret", &headers(Some("s3cret"))));
        assert!(!check("s3cret", &headers(Some("s3cre"))));
        assert!(!check("s3cret", &headers(Some("s3cret!"))));
        assert!(!check("s3cret", &headers(Some("S3cret"))));
        assert!(!check("s3cret", &headers(None)));
    }

    #[test]
    fn empty_secret_rejects_everything() {
        assert!(!check("", &headers(Some(""))));
        assert!(!check("", &headers(None)));
    }
}
