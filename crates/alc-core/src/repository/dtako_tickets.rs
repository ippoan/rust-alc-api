use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{
    DtakoTicket, DtakoTicketCreate, DtakoTicketFilter, DtakoTicketScrapedPatch,
    DtakoTicketsResponse,
};

/// dtako_tickets テーブルの抽象 (Refs ippoan/email-receiver#1)。
///
/// - `create` / `patch_scraped` は email-receiver Worker から internal shared-secret
///   経由で呼ばれる (tenant スコープは X-Tenant-ID で渡る)。
/// - `list` / `get` は nuxt_dtako_logs から JWT 経由で呼ばれる (tenant スコープ)。
/// - `close_by_token` は browser から close_token 1 つで呼ばれる
///   (tenant_id は token から逆引き、SECURITY DEFINER 関数で実装)。
#[async_trait]
pub trait DtakoTicketsRepository: Send + Sync {
    /// 起票。close_token は実装側で URL-safe 32 byte hex を採番する。
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &DtakoTicketCreate,
    ) -> Result<DtakoTicket, sqlx::Error>;

    /// F-VOS3020 scrape 結果を反映。status を 'scraped' に進める。
    /// 該当 id が無ければ Ok(None)。
    async fn patch_scraped(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &DtakoTicketScrapedPatch,
    ) -> Result<Option<DtakoTicket>, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &DtakoTicketFilter,
    ) -> Result<DtakoTicketsResponse, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<DtakoTicket>, sqlx::Error>;

    /// `close_dtako_ticket_by_token(token, closed_by)` SECURITY DEFINER 関数を呼ぶ。
    /// token が見つからない / 既に closed なら Ok(None)。
    async fn close_by_token(
        &self,
        close_token: &str,
        closed_by: Option<&str>,
    ) -> Result<Option<Uuid>, sqlx::Error>;
}
