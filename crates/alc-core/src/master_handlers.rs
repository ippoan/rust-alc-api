//! [`crate::master_data`] を使う axum ハンドラの共通部分 (Refs ippoan/rust-alc-api#651)。
//!
//! router の組み立て (パス文字列) はエンティティごとに異なるので各利用者に残す。
//! ここに寄せるのは、リクエスト DTO とハンドラの間で逐語複製されていた部分だけ。

use serde::Deserialize;

/// `PUT /.../{id}` (sort_order 変更) の request body。
/// ts-rs export はしていない (元から内部専用の DTO) — 4 エンティティで
/// 逐語複製されていたのをここへ寄せた。
#[derive(Debug, Deserialize)]
pub struct UpdateSortOrder {
    pub sort_order: i32,
}

/// create ハンドラの 409 判定: unique/exclusion 制約違反かどうか。
/// 呼び出し側は `false` の場合だけ `tracing::error!` でログしてから 500 を返す
/// (制約違反はユーザー起因で、サーバエラーとしてログる対象ではないため)。
pub fn is_conflict(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db_err) if db_err.constraint().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_sort_order_deserializes_sort_order_field() {
        let body: UpdateSortOrder = serde_json::from_str(r#"{"sort_order": 3}"#).unwrap();
        assert_eq!(body.sort_order, 3);
    }

    #[test]
    fn is_conflict_false_for_non_database_error() {
        let e = sqlx::Error::RowNotFound;
        assert!(!is_conflict(&e));
    }
}
