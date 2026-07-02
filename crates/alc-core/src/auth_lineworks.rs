//! 保存 secret の AES-256-GCM 暗号化 helper (旧 LINE WORKS OAuth モジュール)。
//!
//! LINE WORKS / LINE の OAuth オーケストレーション (authorize URL / code 交換 /
//! profile 取得 / CSRF state) は auth-worker に移管済みのため撤去した (Refs #479)。
//! 残るのは DB 保存 secret (LINE channel secret / LINE WORKS bot secret /
//! SSO client_secret) の暗号化・復号 helper のみ。鍵は SHA-256(SSO_ENCRYPTION_KEY)。

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use sha2::{Digest, Sha256};

/// Encrypt plaintext with AES-256-GCM. Key is SHA-256 hash of key_material.
/// Output: base64(nonce[12] + ciphertext + tag[16])
pub fn encrypt_secret(plaintext: &str, key_material: &str) -> Result<String, String> {
    use ring::rand::{SecureRandom, SystemRandom};

    let mut key_bytes = [0u8; 32];
    let hash = Sha256::digest(key_material.as_bytes());
    key_bytes.copy_from_slice(&hash);

    let unbound_key =
        UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|e| format!("Key error: {e}"))?;
    let key = LessSafeKey::new(unbound_key);

    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)
        .map_err(|e| format!("RNG error: {e}"))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.as_bytes().to_vec();
    let tag_len = aead::AES_256_GCM.tag_len();
    in_out.extend(vec![0u8; tag_len]);

    key.seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out[..plaintext.len()])
        .map(|tag| {
            in_out[plaintext.len()..].copy_from_slice(tag.as_ref());
        })
        .map_err(|e| format!("Encryption error: {e}"))?;

    let mut result = Vec::with_capacity(12 + in_out.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);

    Ok(BASE64.encode(&result))
}

/// Decrypt client_secret stored as AES-256-GCM(base64(nonce + ciphertext + tag))
/// Key is SHA-256 hash of SSO_ENCRYPTION_KEY (Refs #479 PR-1)
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

/// Decrypt a PEM-shaped secret (RSA private key), normalizing legacy rows whose
/// plaintext contains literal `\n` escape sequences instead of real 0x0A newlines.
///
/// jsonwebtoken's `EncodingKey::from_rsa_pem` rejects PEM with escaped newlines
/// as `InvalidKeyFormat`. Some historical writers JSON-escaped the key once too
/// many and persisted the escaped form. Callers that feed the plaintext to a
/// PEM parser should use this helper so those rows keep working without requiring
/// the tenant to re-upload the key.
///
/// Normalization is idempotent and safe: it replaces `\\n` with `\n` only when
/// real newlines are absent, so already-correct PEM plaintext passes through
/// unchanged.
pub fn decrypt_pem_secret(ciphertext_b64: &str, key_material: &str) -> Result<String, String> {
    let plaintext = decrypt_secret(ciphertext_b64, key_material)?;
    Ok(normalize_pem_newlines(plaintext))
}

/// Replace literal `\n` sequences with real newlines when the input has none.
/// Exposed for reuse in other storage paths that keep a decrypted PEM.
pub fn normalize_pem_newlines(s: String) -> String {
    if s.contains("\\n") && !s.contains('\n') {
        s.replace("\\n", "\n")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_pem_newlines_fixes_escaped_input() {
        let escaped =
            "-----BEGIN PRIVATE KEY-----\\nMIIE...\\n-----END PRIVATE KEY-----".to_string();
        let fixed = normalize_pem_newlines(escaped);
        assert!(fixed.contains('\n'), "should have real newlines");
        assert!(!fixed.contains("\\n"), "should not have literal \\n");
        assert_eq!(fixed.lines().count(), 3);
    }

    #[test]
    fn normalize_pem_newlines_passes_valid_pem_unchanged() {
        let valid = "-----BEGIN PRIVATE KEY-----\nMIIE...\n-----END PRIVATE KEY-----".to_string();
        let expected = valid.clone();
        assert_eq!(normalize_pem_newlines(valid), expected);
    }

    #[test]
    fn normalize_pem_newlines_leaves_mixed_input_alone() {
        // Real newlines present → don't touch (could be legitimate inside PEM body)
        let mixed = "-----BEGIN PRIVATE KEY-----\nAB\\nCD\n-----END PRIVATE KEY-----".to_string();
        let out = normalize_pem_newlines(mixed.clone());
        assert_eq!(out, mixed);
    }

    #[test]
    fn decrypt_pem_secret_normalizes_escaped_plaintext() {
        let key_material = "test-key-material-32chars!!!";
        // Encrypt a PEM string that contains literal \n (the bug)
        let escaped_pem =
            "-----BEGIN PRIVATE KEY-----\\nABC\\n-----END PRIVATE KEY-----".to_string();
        let ciphertext = encrypt_secret(&escaped_pem, key_material).expect("encrypt");
        let decrypted = decrypt_pem_secret(&ciphertext, key_material).expect("decrypt");
        assert!(decrypted.contains('\n'));
        assert!(!decrypted.contains("\\n"));
        assert_eq!(decrypted.lines().count(), 3);
    }

    #[test]
    fn decrypt_pem_secret_passes_through_valid_pem() {
        let key_material = "another-32-byte-test-key-!!!!!";
        let valid_pem = "-----BEGIN PRIVATE KEY-----\nABC\n-----END PRIVATE KEY-----".to_string();
        let ciphertext = encrypt_secret(&valid_pem, key_material).expect("encrypt");
        let decrypted = decrypt_pem_secret(&ciphertext, key_material).expect("decrypt");
        assert_eq!(decrypted, valid_pem);
    }

    #[test]
    fn decrypt_pem_secret_propagates_decrypt_errors() {
        let result = decrypt_pem_secret("not-base64!!", "key");
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        test_group!("secret 暗号化");
        test_case!(
            "encrypt_secret → decrypt_secret ラウンドトリップ",
            {
                let key_material = "test-encryption-key-for-roundtrip";
                let plaintext = "my-secret-client-key";

                let ciphertext_b64 = encrypt_secret(plaintext, key_material).unwrap();
                let decrypted = decrypt_secret(&ciphertext_b64, key_material).unwrap();
                assert_eq!(decrypted, plaintext);
            }
        );
    }

    #[test]
    fn test_decrypt_secret_wrong_key() {
        test_group!("secret 暗号化");
        test_case!("不正なキーで復号失敗", {
            use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
            use ring::rand::{SecureRandom, SystemRandom};

            let key_material = "correct-key";
            let plaintext = "secret";
            let mut key_bytes = [0u8; 32];
            let hash = sha2::Sha256::digest(key_material.as_bytes());
            key_bytes.copy_from_slice(&hash);
            let unbound = UnboundKey::new(&AES_256_GCM, &key_bytes).unwrap();
            let key = LessSafeKey::new(unbound);
            let rng = SystemRandom::new();
            let mut nonce_bytes = [0u8; 12];
            rng.fill(&mut nonce_bytes).unwrap();
            let nonce = Nonce::assume_unique_for_key(nonce_bytes);
            let mut in_out = plaintext.as_bytes().to_vec();
            key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
                .unwrap();
            let mut data = nonce_bytes.to_vec();
            data.extend_from_slice(&in_out);
            let ciphertext_b64 = BASE64.encode(&data);

            // wrong key → decryption error
            assert!(decrypt_secret(&ciphertext_b64, "wrong-key").is_err());
        });
    }

    #[test]
    fn test_decrypt_secret_invalid_base64() {
        test_group!("secret 暗号化");
        test_case!("不正なBase64で復号失敗", {
            assert!(decrypt_secret("not-base64!!!", "key").is_err());
        });
    }

    #[test]
    fn test_decrypt_secret_too_short() {
        test_group!("secret 暗号化");
        test_case!("短すぎる暗号文で復号失敗", {
            let short = base64::engine::general_purpose::STANDARD.encode(b"short");
            assert!(decrypt_secret(&short, "key").is_err());
        });
    }
}
