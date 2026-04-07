use alc_core::storage::StorageBackend;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

fn usage() {
    eprintln!("Usage: archive <command> [options]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  logi-dump [--dry-run]");
    eprintln!("    Dump all logi schema tables to R2 (one-time)");
    eprintln!();
    eprintln!("  dtako-archive [--dry-run]");
    eprintln!("    Archive unarchived dtakologs dates to R2, DELETE 7+ day old rows");
    eprintln!();
    eprintln!("  dtako-restore --tenant-id <UUID> --date <YYYY-MM-DD>");
    eprintln!("    Restore archived dtakologs from R2 to DB");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        usage();
        std::process::exit(1);
    }

    let command = args[1].as_str();
    let dry_run = args.iter().any(|a| a == "--dry-run");

    // Connect to DB (superuser for cross-tenant access, no search_path restriction)
    let database_url =
        std::env::var("ARCHIVE_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL"))?;

    // Strip search_path option if present (archive needs cross-schema access)
    let clean_url = if let Some(pos) = database_url.find("?options=") {
        database_url[..pos].to_string()
    } else {
        database_url.clone()
    };

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&clean_url)
        .await?;

    // Init R2 storage (ohishi-dtako bucket)
    let storage = init_r2_storage()?;

    match command {
        "logi-dump" => {
            rust_alc_api::archive::logi::logi_dump(&pool, storage.as_ref(), dry_run).await?;
        }
        "dtako-archive" => {
            rust_alc_api::archive::dtako::dtako_archive(&pool, storage.as_ref(), dry_run).await?;
        }
        "dtako-restore" => {
            let tenant_id = get_arg(&args, "--tenant-id")
                .ok_or_else(|| anyhow::anyhow!("--tenant-id required"))?;
            let date = get_arg(&args, "--date")
                .ok_or_else(|| anyhow::anyhow!("--date required (YYYY-MM-DD)"))?;
            rust_alc_api::archive::dtako::dtako_restore(&pool, storage.as_ref(), &tenant_id, &date)
                .await?;
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            usage();
            std::process::exit(1);
        }
    }

    Ok(())
}

fn init_r2_storage() -> anyhow::Result<Arc<dyn StorageBackend>> {
    let bucket = std::env::var("DTAKO_R2_BUCKET").unwrap_or_else(|_| "ohishi-dtako".to_string());
    let account_id = std::env::var("R2_ACCOUNT_ID")?;
    let access_key = std::env::var("DTAKO_R2_ACCESS_KEY")?;
    let secret_key = std::env::var("DTAKO_R2_SECRET_KEY")?;

    let backend =
        rust_alc_api::storage::R2Backend::new(bucket, account_id, access_key, secret_key, None)?;

    Ok(Arc::new(backend))
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
