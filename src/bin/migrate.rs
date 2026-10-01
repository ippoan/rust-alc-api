use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;

    alc_migrations::MIGRATOR.run(&pool).await?;

    println!("Migrations completed successfully");
    Ok(())
}
