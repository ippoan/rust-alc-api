//! dev端末 (開発用の鍵) の印を、要求のスコープに結ぶ (Refs ippoan/alc-app#387)。
//!
//! 認証 middleware (`require_tenant_header` / `require_internal_shared_secret`) が
//! 認証を通した要求についてだけ [`scope`] を張り、`tenant::set_current_tenant` が
//! [`is_device_dev`] を見て DB の接続の設定 `app.device_dev` を立てる。
//! `set_current_tenant` / `TenantConn::acquire` の引数を増やさないための task-local。
//!
//! **スコープの外は dev でない** — 公開 route、認証前、バッチ、`tokio::spawn` の先。
//! spawn の先で書く行は本番の行になるので、dev かどうかは spawn の前に取ること。

use std::future::Future;

pub use alc_core_wasm::device_dev::{
    device_dev_from_headers, device_tenko_manager_from_headers, DeviceDevSlot, DEVICE_DEV_HEADER,
    DEVICE_ROLE_HEADER, DEVICE_ROLE_TENKO_MANAGER,
};

tokio::task_local! {
    static DEVICE_DEV: DeviceDevSlot;
}

/// いまの要求が dev端末のものか。スコープの外では常に false。
pub fn is_device_dev() -> bool {
    DEVICE_DEV.try_with(DeviceDevSlot::get).unwrap_or(false)
}

/// いまの要求が **運行管理者用の鍵** のものか (dev かどうかは問わない)。スコープの外では常に false。
///
/// キオスクの鍵、管理者ログインは false。dev の鍵かどうかは [`is_device_dev`] で別に見る。
pub fn is_tenko_manager_key() -> bool {
    DEVICE_DEV
        .try_with(DeviceDevSlot::is_tenko_manager)
        .unwrap_or(false)
}

/// `fut` を、入れ物 `slot` を印とする要求のスコープで走らせる。
pub async fn scope<F: Future>(slot: DeviceDevSlot, fut: F) -> F::Output {
    DEVICE_DEV.scope(slot, fut).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn outside_scope_is_not_dev() {
        assert!(!is_device_dev());
    }

    #[tokio::test]
    async fn scope_follows_the_slot() {
        let slot = DeviceDevSlot::default();
        let seen = scope(slot.clone(), async {
            let before = is_device_dev();
            slot.set(true);
            (before, is_device_dev())
        })
        .await;
        assert_eq!(seen, (false, true));
        assert!(!is_device_dev());
    }

    #[tokio::test]
    async fn outside_scope_is_not_tenko_manager_key() {
        assert!(!is_tenko_manager_key());
    }

    #[tokio::test]
    async fn tenko_manager_key_ignores_the_dev_mark() {
        for (dev, manager) in [(false, false), (true, false), (false, true), (true, true)] {
            let slot = DeviceDevSlot::default();
            slot.set(dev);
            slot.set_tenko_manager(manager);
            let seen = scope(slot, async { (is_device_dev(), is_tenko_manager_key()) }).await;
            assert_eq!(seen, (dev, manager), "dev={dev} manager={manager}");
        }
        assert!(!is_tenko_manager_key());
    }

    #[tokio::test]
    async fn spawned_task_loses_the_mark() {
        let slot = DeviceDevSlot::default();
        slot.set(true);
        let in_spawn = scope(slot, async {
            assert!(is_device_dev());
            tokio::spawn(async { is_device_dev() }).await.unwrap()
        })
        .await;
        assert!(!in_spawn);
    }
}
