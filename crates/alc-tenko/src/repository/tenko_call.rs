use async_trait::async_trait;

/// ドライバー登録の結果 (driver_id, call_number)
#[derive(sqlx::FromRow)]
pub struct RegisterDriverResult {
    pub driver_id: i32,
    pub call_number: Option<String>,
}

/// 点呼送信時のドライバー情報
#[derive(sqlx::FromRow)]
pub struct DriverInfo {
    pub id: i32,
    pub call_number: Option<String>,
    pub tenant_id: String,
}

/// 電話番号マスタ行
#[derive(sqlx::FromRow)]
pub struct TenkoCallNumberRow {
    pub id: i32,
    pub call_number: String,
    pub tenant_id: String,
    pub label: Option<String>,
    pub created_at: String,
}

/// 登録ドライバー行
#[derive(sqlx::FromRow)]
pub struct TenkoCallDriverRow {
    pub id: i32,
    pub phone_number: String,
    pub driver_name: String,
    pub call_number: Option<String>,
    pub tenant_id: String,
    pub employee_code: Option<String>,
    pub created_at: String,
}

#[async_trait]
pub trait TenkoCallRepository: Send + Sync {
    /// ドライバー登録 (トランザクション内で call_number 検証 + upsert)
    /// call_number が未登録の場合は Ok(None) を返す
    async fn register_driver(
        &self,
        call_number: &str,
        phone_number: &str,
        driver_name: &str,
        employee_code: Option<&str>,
    ) -> Result<Option<RegisterDriverResult>, sqlx::Error>;

    /// 点呼送信 (トランザクション内でドライバー検索 + ログ挿入)
    /// ドライバー未登録の場合は Ok(None) を返す
    async fn record_tenko(
        &self,
        phone_number: &str,
        driver_name: &str,
        latitude: f64,
        longitude: f64,
    ) -> Result<Option<DriverInfo>, sqlx::Error>;

    /// 電話番号マスタ一覧 (テナント認証なし、直接 pool 参照)
    async fn list_numbers(&self) -> Result<Vec<TenkoCallNumberRow>, sqlx::Error>;

    /// 電話番号マスタ追加
    async fn create_number(
        &self,
        call_number: &str,
        tenant_id: &str,
        label: Option<&str>,
    ) -> Result<i32, sqlx::Error>;

    /// 電話番号マスタ削除
    async fn delete_number(&self, id: i32) -> Result<(), sqlx::Error>;

    /// 登録ドライバー一覧
    async fn list_drivers(&self) -> Result<Vec<TenkoCallDriverRow>, sqlx::Error>;
}
