mod config;
mod db;
mod jobs;

use jobs::JobsRepo;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::Config::from_env()?;

    let queue = std::env::var("QUEUE").unwrap_or_else(|_| "default".to_string());
    let lease_seconds: i64 = std::env::var("LEASE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    println!(
        "pgflow worker starting... worker_id={} queue={} lease={}s",
        cfg.worker_id, queue, lease_seconds
    );

    let pool = db::make_pool(&cfg.database_url).await?;
    let repo = JobsRepo::new(pool);

    loop {
        // 1) reclaim jobs from dead workers
        let reaped = repo.reap_expired_locks().await?;
        if reaped > 0 {
            println!("[{}] reaped {} expired locks", cfg.worker_id, reaped);
        }

        // 2) lease one job
        if let Some(job) = repo
            .lease_one_job(&queue, &cfg.worker_id, lease_seconds)
            .await?
        {
            println!(
                "[{}] leased job id={} type={} run_at={}",
                cfg.worker_id, job.id, job.job_type, job.run_at
            );

            // 3) simulate work (kill the process to test lock expiry recovery)
            tokio::time::sleep(Duration::from_secs(3)).await;

            // 4) mark succeeded
            repo.mark_succeeded(job.id, &cfg.worker_id).await?;
            println!("[{}] succeeded job id={}", cfg.worker_id, job.id);
        } else {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}
