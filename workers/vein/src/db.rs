//! postgres への接続 (Refs #691 / #695)。**DB に繋ぐのはこの 1 か所だけ**で、env の binding で経路を出し分ける
//! (上から順に見て、最初にあったものを使う)。どの経路も間に transaction mode のプーラーが入る
//! (repo の RLS はそれを前提にしている):
//!
//! | env | 経路 | プーラー |
//! |---|---|---|
//! | staging (一時、#695) | Workers VPC の binding `VEIN_DB_VPC` (VPC Service 型、宛先の host:port は Service 側で固定) へ TCP → 既存の Tunnel → 手元の docker の 6432 | 手元の Container image 内の PgBouncer |
//! | staging (fallback) | Durable Object `VEIN_DB` へ TCP (`Stub::connect`) → DO が Container の 6432 へ中継 ([`crate::vein_db`]) | Container 内の PgBouncer |
//! | 本番 | secret `DATABASE_URL` の host:port へ Worker の TCP (STARTTLS) | Supabase のプーラー (6543) |
//! | ローカル | `DATABASE_URL` (`sslmode=disable`) + var `ALLOW_INSECURE_DB = "1"` のときだけ手元の PgBouncer へ平文 (tests/run-local.sh) | 手元の PgBouncer |
//!
//! 本番の接続文字列の設定とデプロイは別タスク。binding も secret も無ければ
//! [`ConnectError::NotConfigured`] (fetch が 503 にする)。
//! binding は secret より先に見るので、平文に落ちる binding (`VEIN_DB_VPC` / `VEIN_DB`) を本番
//! (トップレベル) に置かないことを scripts/check-exposure.sh が検査する。
//!
//! `Socket` は `!Send` 相当 (JS の値) なので、この関数は handler (axum が Send を要求する側)
//! の外 = fetch から呼ぶ。

use tokio_postgres::config::SslMode;
use tokio_postgres::{Client, Config, NoTls};
use worker::postgres_tls::PassthroughTls;
use worker::{console_error, Env, SecureTransport, Socket};

use crate::tcp::TcpPort;

/// staging の Workers VPC binding (VPC Service 型、#695)。宛先 (手元の PgBouncer) は Service 側で固定
pub const VEIN_DB_VPC_BINDING: &str = "VEIN_DB_VPC";
/// staging の DO binding。1 個の DO (= 1 個の Container) に全リクエストを集める
pub const VEIN_DB_BINDING: &str = "VEIN_DB";
const VEIN_DB_NAME: &str = "vein-db";
/// Container 内の PgBouncer のポート (workers/vein/container/pgbouncer.ini)
pub const PGBOUNCER_PORT: u16 = 6432;
/// 本番の接続文字列 (secret)。Supabase のプーラー (transaction mode) を指す
const DATABASE_URL_SECRET: &str = "DATABASE_URL";
/// 平文 (`sslmode=disable`) を許すローカル専用のフラグ。`wrangler dev --var ALLOW_INSECURE_DB:1`
/// か `.dev.vars` にだけ置く (wrangler.toml に書かないことを scripts/check-exposure.sh が検査する)
const ALLOW_INSECURE_DB_VAR: &str = "ALLOW_INSECURE_DB";

#[derive(Debug)]
pub enum ConnectError {
    /// どの経路の binding も secret も無い
    NotConfigured,
    Other(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(
                f,
                "no {VEIN_DB_VPC_BINDING} / {VEIN_DB_BINDING} binding and no {DATABASE_URL_SECRET} secret"
            ),
            Self::Other(e) => f.write_str(e),
        }
    }
}

fn other(what: &str) -> impl Fn(worker::Error) -> ConnectError + '_ {
    move |e| ConnectError::Other(format!("{what}: {e}"))
}

pub async fn connect(env: &Env) -> Result<Client, ConnectError> {
    if let Ok(vpc) = env.get_binding::<TcpPort>(VEIN_DB_VPC_BINDING) {
        return connect_vpc(&vpc).await;
    }
    if let Ok(ns) = env.durable_object(VEIN_DB_BINDING) {
        return connect_container(&ns).await;
    }
    if let Ok(url) = env.secret(DATABASE_URL_SECRET) {
        let allow_insecure = env
            .var(ALLOW_INSECURE_DB_VAR)
            .is_ok_and(|v| v.to_string() == "1");
        return connect_url(&url.to_string(), allow_insecure).await;
    }
    Err(ConnectError::NotConfigured)
}

/// staging: DO への TCP をそのまま postgres のソケットとして使う (DO が Container の
/// PgBouncer へバイトを中継する)。DO → Container は Cloudflare 内なので平文。
/// PgBouncer は trust 認証で、RLS が効く `alc_api_app` で繋ぐ。
async fn connect_container(ns: &worker::ObjectNamespace) -> Result<Client, ConnectError> {
    let stub = ns
        .get_by_name(VEIN_DB_NAME)
        .map_err(other("vein-db stub"))?;
    let socket = stub
        .connect(&format!("{VEIN_DB_NAME}:{PGBOUNCER_PORT}"))
        .map_err(other("vein-db connect"))?;
    handshake(pgbouncer_config(), socket, NoTls).await
}

/// staging (#695): Workers VPC の binding へ TCP を開き、そのまま postgres のソケットとして使う。
/// VPC Service 型は宛先の host:port を Service 側で固定するので、`connect()` に渡すアドレスは
/// 名目の値 (宛先は binding 側で固定され、この文字列は使われない。IP はコードに書かない)。Tunnel の区間は Cloudflare が暗号化し、ホスト内の
/// PgBouncer までは平文・trust 認証 (Container 経路と同じ)。
async fn connect_vpc(vpc: &TcpPort) -> Result<Client, ConnectError> {
    let raw = vpc
        .connect(&format!("{VEIN_DB_NAME}:{PGBOUNCER_PORT}"))
        .map_err(|e| ConnectError::Other(format!("vein-db vpc connect: {e:?}")))?;
    handshake(pgbouncer_config(), Socket::from(raw), NoTls).await
}

/// `container/` の PgBouncer (trust 認証) へ、RLS が効く `alc_api_app` で繋ぐ設定
fn pgbouncer_config() -> Config {
    let mut config = Config::new();
    config
        .user("alc_api_app")
        .dbname("postgres")
        .ssl_mode(SslMode::Disable);
    config
}

/// 本番: 接続文字列の host:port へ STARTTLS で繋ぐ (postgres の SSLRequest の後に
/// `PassthroughTls` が Workers の `startTls()` を呼ぶ)。
///
/// 平文で繋ぐのは、接続文字列が `sslmode=disable` で**かつ** ローカル専用フラグ
/// `ALLOW_INSECURE_DB = "1"` があるときだけ (ローカルの `wrangler dev` から手元の PgBouncer へ繋ぐ
/// tests/run-local.sh 用)。フラグが無ければ `sslmode=disable` でも TLS を強制する — 本番の接続文字列に
/// `sslmode=disable` が紛れ込んでも平文には落ちない。
async fn connect_url(url: &str, allow_insecure: bool) -> Result<Client, ConnectError> {
    let mut config: Config = url
        .parse()
        .map_err(|e| ConnectError::Other(format!("parse {DATABASE_URL_SECRET}: {e}")))?;
    let host = match config.get_hosts().first() {
        Some(tokio_postgres::config::Host::Tcp(h)) => h.clone(),
        _ => {
            return Err(ConnectError::Other(format!(
                "{DATABASE_URL_SECRET} has no tcp host"
            )))
        }
    };
    let port = config.get_ports().first().copied().unwrap_or(5432);
    if allow_insecure && config.get_ssl_mode() == SslMode::Disable {
        let socket = Socket::builder()
            .connect(host, port)
            .map_err(other("socket"))?;
        return handshake(config, socket, NoTls).await;
    }
    config.ssl_mode(SslMode::Require);
    let socket = Socket::builder()
        .secure_transport(SecureTransport::StartTls)
        .connect(host, port)
        .map_err(other("socket"))?;
    handshake(config, socket, PassthroughTls).await
}

async fn handshake<T>(config: Config, socket: Socket, tls: T) -> Result<Client, ConnectError>
where
    T: tokio_postgres::tls::TlsConnect<Socket>,
    T::Stream: Send + 'static,
{
    let (client, connection) = config.connect_raw(socket, tls).await.map_err(|e| {
        // tokio_postgres::Error の Display は "db error" だけなので、DB の message も載せる
        let detail = e
            .as_db_error()
            .map(|db| format!("{} ({})", db.message(), db.code().code()))
            .unwrap_or_else(|| e.to_string());
        ConnectError::Other(format!("postgres handshake: {detail}"))
    })?;
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = connection.await {
            console_error!("postgres connection: {e}");
        }
    });
    Ok(client)
}
