use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::Client;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// LINE WORKS OAuth2 token endpoint
const TOKEN_URL: &str = "https://auth.worksmobile.com/oauth2/v2.0/token";
/// LINE WORKS user info endpoint
const USERINFO_URL: &str = "https://www.worksapis.com/v1.0/users/me";

/// LINE WORKS SSO config from DB
#[derive(Debug, Clone)]
pub struct LineworksSsoConfig {
    pub tenant_id: uuid::Uuid,
    pub client_id: String,
    pub client_secret: String,
    pub external_org_id: String,
    pub woff_id: Option<String>,
}

/// LINE WORKS token exchange response
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    #[serde(deserialize_with = "deserialize_string_or_i64")]
    pub expires_in: i64,
    pub scope: Option<String>,
    pub refresh_token: Option<String>,
}

fn deserialize_string_or_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct StringOrI64;
    impl<'de> de::Visitor<'de> for StringOrI64 {
        type Value = i64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("string or i64")
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> { Ok(v) }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> { Ok(v as i64) }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
            v.parse().map_err(de::Error::custom)
        }
    }
    deserializer.deserialize_any(StringOrI64)
}

/// LINE WORKS user profile response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub user_id: String,
    pub user_name: Option<UserName>,
    pub email: Option<String>,
    pub domain_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserName {
    pub last_name: Option<String>,
    pub first_name: Option<String>,
}

impl UserProfile {
    pub fn display_name(&self) -> String {
        if let Some(name) = &self.user_name {
            let last = name.last_name.as_deref().unwrap_or("");
            let first = name.first_name.as_deref().unwrap_or("");
            let full = format!("{}{}", last, first);
            if full.is_empty() {
                self.user_id.clone()
            } else {
                full
            }
        } else {
            self.user_id.clone()
        }
    }

    pub fn email_or_id(&self) -> String {
        self.email.clone().unwrap_or_else(|| self.user_id.clone())
    }
}

/// Exchange authorization code for access token
pub async fn exchange_code(
    client: &Client,
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, String> {
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {e}"))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("Token exchange failed: {status} {body}"));
    }

    serde_json::from_str::<TokenResponse>(&body)
        .map_err(|e| format!("Token response parse error: {e}, body: {body}"))
}

/// Fetch user profile using access token
pub async fn fetch_user_profile(
    client: &Client,
    access_token: &str,
) -> Result<UserProfile, String> {
    let resp = client
        .get(USERINFO_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("User profile request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("User profile fetch failed: {status} {body}"));
    }

    resp.json::<UserProfile>()
        .await
        .map_err(|e| format!("User profile parse error: {e}"))
}

/// Decrypt client_secret stored as AES-256-GCM(base64(nonce + ciphertext + tag))
/// Key is SHA-256 hash of JWT_SECRET (same as rust-logi)
pub fn decrypt_secret(ciphertext_b64: &str, key_material: &str) -> Result<String, String> {
    let mut key_bytes = [0u8; 32];
    let hash = Sha256::digest(key_material.as_bytes());
    key_bytes.copy_from_slice(&hash);

    let unbound_key =
        UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|e| format!("Key error: {e}"))?;
    let key = LessSafeKey::new(unbound_key);

    let data = BASE64
        .decode(ciphertext_b64)
        .map_err(|e| format!("Base64 decode error: {e}"))?;

    if data.len() < 12 + aead::AES_256_GCM.tag_len() {
        return Err("Ciphertext too short".to_string());
    }

    let (nonce_bytes, ciphertext_and_tag) = data.split_at(12);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes.try_into().unwrap());

    let mut in_out = ciphertext_and_tag.to_vec();
    let plaintext = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| format!("Decryption error: {e}"))?;

    String::from_utf8(plaintext.to_vec()).map_err(|e| format!("UTF-8 error: {e}"))
}

/// Build LINE WORKS authorize URL
pub fn authorize_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    format!(
        "https://auth.worksmobile.com/oauth2/v2.0/authorize?\
         client_id={client_id}\
         &redirect_uri={redirect_uri}\
         &response_type=code\
         &scope=user.profile.read\
         &state={state}"
    )
}

/// HMAC-SHA256 state signing for CSRF protection
pub mod state {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use serde::{Deserialize, Serialize};

    type HmacSha256 = Hmac<Sha256>;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct StatePayload {
        pub redirect_uri: String,
        pub nonce: String,
        pub provider: String,
        pub external_org_id: String,
    }

    pub fn sign(payload: &StatePayload, secret: &str) -> String {
        let json = serde_json::to_string(payload).unwrap();
        let payload_b64 = URL_SAFE_NO_PAD.encode(json.as_bytes());

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload_b64.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

        format!("{payload_b64}.{sig}")
    }

    pub fn verify(state: &str, secret: &str) -> Result<StatePayload, String> {
        let parts: Vec<&str> = state.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Err("Invalid state format".into());
        }
        let (payload_b64, sig_b64) = (parts[0], parts[1]);

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload_b64.as_bytes());
        let expected_sig = URL_SAFE_NO_PAD
            .decode(sig_b64)
            .map_err(|_| "Invalid signature encoding")?;
        mac.verify_slice(&expected_sig)
            .map_err(|_| "State signature verification failed")?;

        let json = URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| "Invalid payload encoding")?;
        serde_json::from_slice(&json).map_err(|e| format!("State payload parse error: {e}"))
    }
}
