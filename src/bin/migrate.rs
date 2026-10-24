use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;

    // 空の DB (staging の postgres sidecar) は schema とロールから作る。初期化の SQL は migration と同じ rev の
    // `alc_migrations::INIT_LOCAL_DB` で、冪等でない (`CREATE ROLE`) ので `alc_api` schema が無いときだけ流す。
    // 本番 (Supabase) は schema が在るので何もしない
    let initialized: bool = sqlx::query_scalar("SELECT to_regnamespace('alc_api') IS NOT NULL")
        .fetch_one(&pool)
        .await?;
    if !initialized {
        sqlx::raw_sql(alc_migrations::INIT_LOCAL_DB)
            .execute(&pool)
            .await?;
        println!("Initialized empty database (alc_api schema and roles)");
    }

    alc_migrations::MIGRATOR.run(&pool).await?;

    println!("Migrations completed successfully");
    Ok(())
}
