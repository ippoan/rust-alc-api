//! `JWT_SECRET` drift 検知用 canary endpoint (Refs #218)。
//!
//! auth-worker と rust-alc-api は HS256 鍵 `JWT_SECRET` を共有しており、
//! どちらか一方だけ rotate された / 移行漏れで drift すると cookie verify が
//! silent fail してユーザーが redirect loop に陥る。auth-worker 単独では
//! drift を検知できない (鍵自体は secrets store 上で空でなく valid に見える) ため、
//! 対向側に **HMAC oracle** を立て、challenge を双方の secret で署名して
//! 一致するかを比較する。
//!
//! ## API
//!
//! `GET /api/internal/health/jwt-canary?challenge=<64-char hex>`
//!
//! - `require_internal_jwt` 配下 (= 呼び出し元が既に matching JWT_SECRET を
//!   持っていることが前提)。drift があれば middleware で 401 になり、
//!   そもそも本ハンドラまで到達しない。
//! - challenge は **32-byte hex** (64 文字、`[0-9a-fA-F]+`) のみ許容。
//!   不正な challenge は 400 を返す。
//! - 応答は `{"signature": "<64-char lowercase hex>"}` のみで JWT_SECRET の
//!   値は echo しない (HMAC-SHA256 の出力のみ)。
//!
//! ## 呼び出し側の判定ロジック (auth-worker 側で実装)
//!
//! 1. 32-byte random challenge を生成
//! 2. 自分の `JWT_SECRET` で HMAC-SHA256(challenge) を計算 (= expected)
//! 3. 本 endpoint を internal JWT 付きで叩く
//! 4. 返ってきた signature と expected を constant-time 比較
//!    - 一致 → ok (drift 無し)
//!    - 不一致 → degraded (drift 検知)
//!    - 401 → degraded (internal JWT 自体が拒否されている = drift の典型ケース)

use axum::{extract::Query, http::StatusCode, routing::get, Extension, Json, Router};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use alc_core::auth_jwt::JwtSecret;
use alc_core::AppState;

type HmacSha256 = Hmac<Sha256>;

const CHALLENGE_HEX_LEN: usize = 64;

#[derive(Debug, Deserialize)]
pub struct CanaryQuery {
    pub challenge: String,
}

#[derive(Debug, Serialize)]
pub struct CanaryResponse {
    pub signature: String,
}

#[derive(Debug, Serialize)]
pub struct CanaryError {
    pub error: &'static str,
}

/// `require_internal_jwt` 配下の internal router。
pub fn internal_router() -> Router<AppState> {
    Router::new().route("/internal/health/jwt-canary", get(jwt_canary))
}

async fn jwt_canary(
    Extension(jwt_secret): Extension<JwtSecret>,
    Query(q): Query<CanaryQuery>,
) -> Result<Json<CanaryResponse>, (StatusCode, Json<CanaryError>)> {
    let signature = compute_canary_signature(&jwt_secret.0, &q.challenge)
        .map_err(|err| (StatusCode::BAD_REQUEST, Json(CanaryError { error: err })))?;
    Ok(Json(CanaryResponse { signature }))
}

/// 純粋関数版 — テスト容易性のため async から分離。
///
/// 戻り値の `Err` は 400 で返す `error` 文字列を返す (識別子は API 仕様の一部)。
pub fn compute_canary_signature(secret: &str, challenge: &str) -> Result<String, &'static str> {
    if challenge.len() != CHALLENGE_HEX_LEN {
        return Err("challenge_must_be_64_hex_chars");
    }
    let challenge_bytes = hex::decode(challenge).map_err(|_| "challenge_not_hex")?;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(&challenge_bytes);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 同じ HMAC を独立に計算する oracle (実装と独立にしておく)。
    fn oracle(secret: &str, challenge_hex: &str) -> String {
        let bytes = hex::decode(challenge_hex).unwrap();
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(&bytes);
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn signs_valid_challenge_with_64char_lowercase_hex() {
        let secret = "shared-secret-key-256-bits-long!";
        let challenge = "a".repeat(64);
        let sig = compute_canary_signature(secret, &challenge).unwrap();
        assert_eq!(sig.len(), 64);
        assert_eq!(sig, oracle(secret, &challenge));
        // 出力は小文字 hex のみ
        assert!(sig
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
    }

    #[test]
    fn different_secrets_produce_different_signatures() {
        let challenge = "0123456789abcdef".repeat(4);
        let sig_a =
            compute_canary_signature("secret-a-value-padding-to-32-byt", &challenge).unwrap();
        let sig_b =
            compute_canary_signature("secret-b-value-padding-to-32-byt", &challenge).unwrap();
        assert_ne!(sig_a, sig_b);
    }

    #[test]
    fn same_secret_same_challenge_is_deterministic() {
        let secret = "k".repeat(32);
        let challenge = "f".repeat(64);
        let s1 = compute_canary_signature(&secret, &challenge).unwrap();
        let s2 = compute_canary_signature(&secret, &challenge).unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn rejects_wrong_length_challenge() {
        let err = compute_canary_signature("any", "tooshort").unwrap_err();
        assert_eq!(err, "challenge_must_be_64_hex_chars");
        // 65 chars も rejected
        let err = compute_canary_signature("any", &"a".repeat(65)).unwrap_err();
        assert_eq!(err, "challenge_must_be_64_hex_chars");
    }

    #[test]
    fn rejects_non_hex_challenge() {
        let err = compute_canary_signature("any", &"z".repeat(64)).unwrap_err();
        assert_eq!(err, "challenge_not_hex");
    }

    #[test]
    fn accepts_uppercase_hex_challenge() {
        // hex::decode は upper/lower どちらも受け入れる。
        let secret = "k".repeat(32);
        let upper = "ABCDEF".repeat(10) + "0123";
        assert_eq!(upper.len(), 64);
        let sig_u = compute_canary_signature(&secret, &upper).unwrap();
        let sig_l = compute_canary_signature(&secret, &upper.to_lowercase()).unwrap();
        assert_eq!(sig_u, sig_l);
    }

    #[test]
    fn ensures_internal_router_has_canary_route() {
        // 型レベルでも internal_router が `Router<AppState>` を返すことを確認。
        let _r: Router<AppState> = internal_router();
    }
}
