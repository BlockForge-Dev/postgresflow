use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

pub async fn make_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let max_connections = std::env::var("PGFLOW_DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(4)
        .clamp(1, 32);

    let acquire_timeout_secs = std::env::var("PGFLOW_DB_ACQUIRE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(1, 60);

    let disable_sync_commit = env_bool("PGFLOW_DISABLE_SYNC_COMMIT", false);
    let disable_jit = env_bool("PGFLOW_DISABLE_JIT", true);

    let mut opts = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(acquire_timeout_secs));

    opts = opts.after_connect(move |conn, _meta| {
        Box::pin(async move {
            if disable_sync_commit {
                sqlx::query("SET synchronous_commit = OFF")
                    .execute(&mut *conn)
                    .await?;
            }
            if disable_jit {
                sqlx::query("SET jit = OFF").execute(&mut *conn).await?;
            }
            Ok(())
        })
    });

    let pool = opts.connect(database_url).await?;

    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
