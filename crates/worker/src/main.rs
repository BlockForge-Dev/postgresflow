use postgresflow::api;
use postgresflow::config;
use postgresflow::db;

use postgresflow::jobs::ingest_decisions::IngestDecisionsRepo;
use postgresflow::jobs::enqueue_guard::{EnqueueGuard, EnqueueGuardConfig};
use postgresflow::jobs::maintenance::{cutoff_days, MaintenanceRepo};
use postgresflow::jobs::metrics::MetricsRepo;
use postgresflow::jobs::retry::RetryConfig;
use postgresflow::jobs::runner::JobRunner;
use postgresflow::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};

use std::time::{Duration, Instant};
mod handlers;
use handlers::{build_registry, JobContext, JobError};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::Config::from_env()?;

    let queue = cfg.queue.clone();
    let lease_seconds = cfg.lease_seconds;
    let api_addr = cfg.admin_addr.clone();

    // Maintenance envs
    let archive_after_days: i64 = std::env::var("ARCHIVE_SUCCEEDED_AFTER_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);
    let prune_history_after_days: i64 = std::env::var("PRUNE_HISTORY_AFTER_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);
    let maintenance_interval_secs: u64 = std::env::var("MAINTENANCE_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);

    println!(
        "pgflow starting... worker_id={} queue={} lease={}s api={} migrate_on_startup={} archive_after_days={} prune_history_after_days={} maintenance_interval_secs={}",
        cfg.worker_id,
        queue,
        lease_seconds,
        api_addr.clone().unwrap_or_else(|| "disabled".to_string()),
        cfg.migrate_on_startup,
        archive_after_days,
        prune_history_after_days,
        maintenance_interval_secs
    );

    let pool = db::make_pool(&cfg.database_url).await?;
    if cfg.migrate_on_startup {
        db::run_migrations(&pool).await?;
    }

    let jobs_repo = JobsRepo::new(pool.clone());
    let attempts_repo = AttemptsRepo::new(pool.clone());
    let policy_decisions_repo = PolicyDecisionsRepo::new(pool.clone());
    let ingest_decisions_repo = IngestDecisionsRepo::new(pool.clone());
    let maintenance_repo = MaintenanceRepo::new(pool.clone());
    let metrics_repo = MetricsRepo::new(pool.clone());
    let enqueue_guard = EnqueueGuard::new(
        pool.clone(),
        ingest_decisions_repo.clone(),
        EnqueueGuardConfig {
            max_payload_bytes: cfg.max_payload_bytes,
            max_enqueues_per_minute_per_queue: cfg.max_enqueues_per_minute_per_queue,
        },
    );

    let runner = JobRunner::new(
        jobs_repo.clone(),
        attempts_repo.clone(),
        RetryConfig::default(),
    );
    let registry = build_registry();
    let ctx = JobContext {
        db: pool.clone(),
        worker_id: cfg.worker_id.clone(),
    };

    // ---- API task ----
    let api_state = api::ApiState {
        jobs: jobs_repo.clone(),
        attempts: attempts_repo.clone(),
        policy_decisions: policy_decisions_repo.clone(),
        ingest_decisions: ingest_decisions_repo.clone(),
        metrics: metrics_repo.clone(),
        enqueue_guard: enqueue_guard.clone(),
    };
    let app = api::router(api_state);

    let api_handle = tokio::spawn(async move {
        if let Some(addr) = api_addr {
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            println!("admin api listening on http://{addr}");
            axum::serve(listener, app).await?;
        } else {
            std::future::pending::<()>().await;
        }
        Ok::<(), anyhow::Error>(())
    });

    // ---- Maintenance task ----
    let maintenance_handle = {
        let maintenance = maintenance_repo.clone();
        tokio::spawn(async move {
            loop {
                // 1) archive succeeded jobs older than N days
                let cutoff_archive = cutoff_days(archive_after_days);
                match maintenance
                    .archive_succeeded_older_than(cutoff_archive, 500)
                    .await
                {
                    Ok(n) if n > 0 => println!("[maintenance] archived {} succeeded jobs", n),
                    Ok(_) => {}
                    Err(e) => eprintln!("[maintenance] archive error: {e}"),
                }

                // 2) prune history for succeeded jobs older than N days
                let cutoff_prune = cutoff_days(prune_history_after_days);
                match maintenance
                    .delete_history_for_succeeded_older_than(cutoff_prune, 500)
                    .await
                {
                    Ok((a, p)) if a > 0 || p > 0 => println!(
                        "[maintenance] deleted attempts={} policy_decisions={}",
                        a, p
                    ),
                    Ok(_) => {}
                    Err(e) => eprintln!("[maintenance] prune error: {e}"),
                }

                tokio::time::sleep(Duration::from_secs(maintenance_interval_secs)).await;
            }
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        })
    };

    // ---- Worker loop task ----
    let worker_id = cfg.worker_id.clone();
    let worker_queue = queue.clone();

    let worker_handle = tokio::spawn(async move {
        loop {
            // reclaim jobs from dead workers
            let reaped = jobs_repo.reap_expired_locks().await?;
            if reaped > 0 {
                println!("[{}] reaped {} expired locks", worker_id, reaped);
            }

            // lease one job (storm-control may throttle -> returns None)
            if let Some(job) = jobs_repo
                .lease_one_job(&worker_queue, &worker_id, lease_seconds)
                .await?
            {
                let attempt = attempts_repo.start_attempt(job.id, &worker_id).await?;
                let start = Instant::now();

                println!(
                    "[{}] leased job id={} type={} attempt_no={}",
                    worker_id, job.id, job.job_type, attempt.attempt_no
                );

                let result: Result<(), JobError> = match registry.handler_for(&job.job_type) {
                    Some(entry) => entry.run(&job, &ctx).await,
                    None => Err(JobError::new(
                        "UNKNOWN_JOB_TYPE",
                        format!("no handler for job_type={}", job.job_type),
                    )),
                };

                let latency_ms = start.elapsed().as_millis() as i32;

                match result {
                    Ok(()) => {
                        runner
                            .on_success(job.id, attempt.id, &worker_id, latency_ms)
                            .await?;
                        println!(
                            "[{}] succeeded job id={} attempt_no={} latency_ms={}",
                            worker_id, job.id, attempt.attempt_no, latency_ms
                        );
                    }
                    Err(err) => {
                        runner
                            .on_failure(
                                job.id,
                                attempt.id,
                                &worker_id,
                                latency_ms,
                                err.code,
                                &err.message,
                                attempt.attempt_no,
                                job.max_attempts,
                            )
                            .await?;
                        println!(
                            "[{}] failed job id={} attempt_no={} code={} (retry/DLQ rules applied)",
                            worker_id, job.id, attempt.attempt_no, err.code
                        );
                    }
                }
            } else {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }

        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });

    tokio::select! {
        res = api_handle => res??,
        res = worker_handle => res??,
        res = maintenance_handle => res??,
    }

    Ok(())
}
