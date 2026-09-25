//! 指静脈 (Waveshare Finger Vein Module) の 1:N 照合をサーバーで回すための crate。
//!
//! 照合の本体は XGComApi.dll と一致させた Rust 実装 `vein-match-search`
//! (private repo ippoan/vein-match を git 依存で tag 固定) にあり、この crate は
//! rust-alc-api から見た窓口になる (Refs ippoan/vein-match#20)。
//! 今はまだ route / DB に繋いでいない — git 依存を Cargo / Bazel / CI で
//! 取り込めることの証明だけを持つ。表と API は後続で足す。

use vein_match_search::Library;

/// `n` 人ぶんの 1:N 照合ライブラリ (`XG_CreateVein(&h, n)`) を作れる人数かを返す。
///
/// 範囲は DLL と同じく `1 < n <= 500`。判定は `Library::new` に任せ、
/// ここで範囲を書き写さない (vein-match 側の変更に追従させるため)。
pub fn library_capacity_ok(n: usize) -> bool {
    Library::new(n).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_within_dll_range() {
        assert!(library_capacity_ok(2));
        assert!(library_capacity_ok(500));
    }

    #[test]
    fn rejects_outside_dll_range() {
        assert!(!library_capacity_ok(0));
        assert!(!library_capacity_ok(1));
        assert!(!library_capacity_ok(501));
    }
}
