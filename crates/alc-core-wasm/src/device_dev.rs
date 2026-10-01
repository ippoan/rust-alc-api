//! dev端末 (開発用の鍵) の印 (Refs ippoan/alc-app#387)。
//!
//! auth-worker は検証済みの要求にだけ `X-Device-Dev: 1` を付けて backend へ渡す。
//! backend はその印を DB の接続の設定 `app.device_dev` に通し、列の既定値と RLS が
//! dev の行と本番の行を分ける (alc-migrations 153)。
//!
//! ここに置くのは tokio に依らない部分だけ — ヘッダを読む唯一の関数と、
//! 要求 1 件ぶんの印の入れ物。入れ物を要求のスコープ (task-local) に結ぶのは
//! `alc-core` の `device_dev`。

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use axum::http::HeaderMap;

pub const DEVICE_DEV_HEADER: &str = "X-Device-Dev";

/// `X-Device-Dev` を読む唯一の関数。値がちょうど `"1"` のときだけ dev。
///
/// `true` / `0` / 空 / 欄なしは dev でない。同名の欄が 2 つ以上あるときも dev でない。
/// **認証を通した後にだけ呼ぶこと** — 公開 route でこの欄を信じてはいけない。
pub fn device_dev_from_headers(headers: &HeaderMap) -> bool {
    let mut values = headers.get_all(DEVICE_DEV_HEADER).iter();
    matches!((values.next(), values.next()), (Some(v), None) if v.as_bytes() == b"1")
}

/// 要求 1 件ぶんの印の入れ物。初期値は dev でない。
///
/// 認証 middleware が、認証を通した後に書く。clone は同じ中身を指す。
#[derive(Clone, Debug, Default)]
pub struct DeviceDevSlot(Arc<AtomicBool>);

impl DeviceDevSlot {
    pub fn set(&self, dev: bool) {
        self.0.store(dev, Ordering::Relaxed);
    }

    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(values: &[&str]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for v in values {
            h.append(DEVICE_DEV_HEADER, v.parse().unwrap());
        }
        h
    }

    #[test]
    fn only_exactly_one_is_dev() {
        assert!(device_dev_from_headers(&headers(&["1"])));
        for v in ["true", "0", "", " 1", "1 ", "11", "yes"] {
            assert!(!device_dev_from_headers(&headers(&[v])), "{v:?}");
        }
        assert!(!device_dev_from_headers(&headers(&[])));
    }

    #[test]
    fn repeated_header_is_not_dev() {
        assert!(!device_dev_from_headers(&headers(&["1", "1"])));
        assert!(!device_dev_from_headers(&headers(&["0", "1"])));
    }

    #[test]
    fn slot_defaults_to_not_dev_and_clones_share() {
        let slot = DeviceDevSlot::default();
        assert!(!slot.get());
        let other = slot.clone();
        other.set(true);
        assert!(slot.get());
        slot.set(false);
        assert!(!other.get());
    }
}
