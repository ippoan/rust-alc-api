use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct LineworksChannel {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub bot_config_id: Uuid,
    pub channel_id: String,
    pub title: Option<String>,
    pub channel_type: Option<String>,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// webhook が bot_id から bot_config を解決するための SECURITY DEFINER 関数の戻り値
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BotConfigForWebhook {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub bot_secret_encrypted: Option<String>,
}

#[async_trait]
pub trait LineworksChannelsRepository: Send + Sync {
    /// active = TRUE のチャネルだけを返す
    async fn list_active(&self, tenant_id: Uuid) -> Result<Vec<LineworksChannel>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid)
        -> Result<Option<LineworksChannel>, sqlx::Error>;

    /// internal 経路 (auth-worker 経由の無人送信) が channel 行 id だけから
    /// tenant ごと解決するための RLS バイパス取得
    /// (SECURITY DEFINER 関数 lookup_lineworks_channel_for_send を呼ぶ)
    async fn get_for_send(&self, id: Uuid) -> Result<Option<LineworksChannel>, sqlx::Error>;

    /// webhook の `joined` イベントで呼ばれる upsert
    /// 既存行があれば active=TRUE + joined_at=NOW() に戻す
    async fn upsert_joined(
        &self,
        tenant_id: Uuid,
        bot_config_id: Uuid,
        channel_id: &str,
        channel_type: Option<&str>,
        title: Option<&str>,
    ) -> Result<LineworksChannel, sqlx::Error>;

    /// webhook の `left` イベントで呼ばれる
    async fn mark_left(
        &self,
        tenant_id: Uuid,
        bot_config_id: Uuid,
        channel_id: &str,
    ) -> Result<(), sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error>;

    /// 認証なし webhook が bot_id から bot_config / tenant_id / bot_secret を解決
    /// (SECURITY DEFINER 関数 lookup_bot_config_for_webhook を呼ぶ)
    async fn lookup_bot_config_for_webhook(
        &self,
        bot_id: &str,
    ) -> Result<Option<BotConfigForWebhook>, sqlx::Error>;
}
