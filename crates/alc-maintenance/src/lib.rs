//! 車両整備記録 (maintenance) ドメイン。Refs ippoan/rust-alc-api#651。
//!
//! 遠隔点呼の国交省要件 2-四-ト「運行に使用する事業用自動車の整備状況」を満たすための
//! **車両本体**の整備記録。`alc-tenko` の `equipment_failures` (キオスク端末・顔認証・
//! ALC センサー等 alc-app 自体の機器故障) や `alc-trouble` (チケットと車両の FK が無い
//! 自由テキスト照合) とは別物なので、それらとは独立した crate に置く。
//!
//! **車両 identity の正本は `maintenance_vehicles`。電子車検証 (car_inspection) は
//! 従属する任意のリンク** — car_inspection の UNIQUE は車検証 1 枚ごとの履歴表 (`(tenant_id,
//! "ElectCertMgNo", "GrantdateE","GrantdateY","GrantdateM","GrantdateD")`、
//! migrations/048:232) で `(tenant_id, "CarId")` は index のみ (048:236) のため
//! FK の参照先にできず、かつ car_inspection に行があるのは一部テナントだけ (Refs #651)。
//!
//! このタスクでは車両マスタ API (`vehicles`) のみを持つ。整備カテゴリ / 整備記録 /
//! 添付ファイルの handler は後続タスクが足す (テーブルは migrations/147 で先に用意済み)。

pub mod files;
pub mod models;
pub mod records;
pub mod vehicles;

use std::sync::Arc;

use alc_core::repository::car_inspections::CarInspectionRepository;
use alc_core::storage::StorageBackend;

use crate::files::MaintenanceFilesRepository;
use crate::records::RecordsRepository;
use crate::vehicles::VehiclesRepository;

/// maintenance 用の最小 State。モノリスでは `.with_state()` 経由でマウントする
/// (`alc-trouble::TroubleState` と同じ形、Refs #513 Phase B)。repo は trait object
/// で持つ (テスト側が mock 実装に差し替えられるようにするため)。
#[derive(Clone)]
pub struct MaintenanceState {
    pub vehicles: Arc<dyn VehiclesRepository>,
    /// 整備記録 (`maintenance_records`) の CRUD + 一覧フィルタ (Refs #651)。
    pub records: Arc<dyn RecordsRepository>,
    /// 電子車検証の照合 (`lookup_expiry`) を in-process で呼ぶための port。
    /// `alc-carins` の `PgCarInspectionRepository` を呼び出し元 (main.rs / テスト) が
    /// 注入する — `alc-maintenance` は `alc-carins` に依存しない (trait は `alc-core`)。
    pub car_inspections: Arc<dyn CarInspectionRepository>,
    /// 整備カテゴリの読み書き (Refs ippoan/rust-alc-api#651 — generic master の
    /// 5 番目の利用者)。
    pub categories: Arc<dyn categories::MaintenanceCategoriesRepository>,
    /// 整備記録の添付ファイル (Refs #651)。
    pub files: Arc<dyn MaintenanceFilesRepository>,
    /// 写真添付用の storage。新しい `*_R2_BUCKET` は足さず、`src/main.rs` で
    /// 組み立てている既定の共有 storage に prefix で相乗りする
    /// (`crates/alc-trouble` の `trouble_storage` と同じ `Option` 設計 — 未設定なら
    /// handler 側で 503 fail-closed にする)。
    pub storage: Option<Arc<dyn StorageBackend>>,
}

pub mod categories;
