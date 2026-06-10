//! shared secret / 署名の定数時間比較ヘルパ。
//!
//! 文字列 `==` 比較は不一致バイトで早期 return するためタイミング側チャネルの
//! 余地がある。ヘッダ shared secret (X-Internal-Secret 等) の検証はこちらを使う
//! (Refs #393 M-2)。HMAC 署名の検証は `Mac::verify_slice` を直接使うこと。

/// 定数時間比較。タイミング攻撃で長さや位置を漏らさないため、長さ一致時は最後まで XOR する。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_basic() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }
}
