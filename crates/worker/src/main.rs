use postgresflow::api;
use postgresflow::config;
use postgresflow::db;

use postgresflow::jobs::enqueue_guard::{EnqueueGuard, EnqueueGuardConfig};
use postgresflow::jobs::ingest_decisions::IngestDecisionsRepo;
use postgresflow::jobs::maintenance::{cutoff_days, MaintenanceRepo};
use postgresflow::jobs::metrics::MetricsRepo;
use postgresflow::jobs::retry::RetryConfig;
use postgresflow::jobs::runner::JobRunner;
use postgresflow::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};

use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;
mod handlers;
use handlers::{build_registry, JobContext, JobError};

enum JobExecutionOutcome {
    Succeeded {
        job_id: Uuid,
        attempt_id: Uuid,
        attempt_no: i32,
        latency_ms: i32,
    },
    Failed {
        job_id: Uuid,
        attempt_id: Uuid,
        attempt_no: i32,
        max_attempts: i32,
        latency_ms: i32,
        error_code: String,
        error_message: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::Config::from_env()?;

    let queue = cfg.queue.clone();
    let lease_seconds = cfg.lease_seconds;
    let dequeue_batch_size = cfg.dequeue_batch_size;
    let reap_interval = Duration::from_millis(cfg.reap_interval_ms);
    let verbose_job_logs = cfg.verbose_job_logs;
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
        "pgflow starting... worker_id={} queue={} lease={}s dequeue_batch_size={} reap_interval_ms={} verbose_job_logs={} api={} auth={} migrate_on_startup={} archive_after_days={} prune_history_after_days={} maintenance_interval_secs={}",
        cfg.worker_id,
        queue,
        lease_seconds,
        dequeue_batch_size,
        cfg.reap_interval_ms,
        cfg.verbose_job_logs,
        api_addr.clone().unwrap_or_else(|| "disabled".to_string()),
        if cfg.api_token.is_some() { "enabled" } else { "disabled" },
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
        api_token: cfg.api_token.clone(),
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
    let worker_batch_size = dequeue_batch_size;
    let worker_reap_interval = reap_interval;
    let worker_verbose_job_logs = verbose_job_logs;

    let worker_handle = tokio::spawn(async move {
        let mut last_reap_at = Instant::now() - worker_reap_interval;

        loop {
            // reclaim jobs from dead workers on a fixed interval to avoid hot-loop write load.
            if last_reap_at.elapsed() >= worker_reap_interval {
                let reaped = jobs_repo.reap_expired_locks().await?;
                last_reap_at = Instant::now();
                if reaped > 0 {
                    println!("[{}] reaped {} expired locks", worker_id, reaped);
                }
            }

            let batch = jobs_repo
                .lease_jobs_batch(&worker_queue, &worker_id, lease_seconds, worker_batch_size)
                .await?;

            if batch.is_empty() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            let leased_dataset_id = batch[0].dataset_id.clone();
            if batch.iter().any(|j| j.dataset_id != leased_dataset_id) {
                anyhow::bail!("mixed dataset batch returned from lease query");
            }

            let dataset_ids: Vec<String> = batch.iter().map(|j| j.dataset_id.clone()).collect();
            let job_ids: Vec<Uuid> = batch.iter().map(|j| j.id).collect();

            let started_attempts = attempts_repo
                .start_attempts_batch(&dataset_ids, &job_ids, &worker_id)
                .await?;

            if started_attempts.len() != batch.len() {
                anyhow::bail!(
                    "attempt insert count mismatch: inserted={} leased={}",
                    started_attempts.len(),
                    batch.len()
                );
            }

            let mut attempts_by_job: HashMap<Uuid, (Uuid, i32)> = started_attempts
                .into_iter()
                .map(|(job_id, attempt_id, attempt_no)| (job_id, (attempt_id, attempt_no)))
                .collect();

            let mut join_set = tokio::task::JoinSet::new();
            for job in batch {
                let registry = registry.clone();
                let ctx = ctx.clone();
                let worker_id_for_task = worker_id.clone();
                let verbose_job_logs_for_task = worker_verbose_job_logs;
                let (attempt_id, attempt_no) = attempts_by_job
                    .remove(&job.id)
                    .ok_or_else(|| anyhow::anyhow!("missing started attempt for job {}", job.id))?;

                join_set.spawn(async move {
                    let start = Instant::now();

                    if verbose_job_logs_for_task {
                        println!(
                            "[{}] leased job id={} type={} attempt_no={}",
                            worker_id_for_task, job.id, job.job_type, attempt_no
                        );
                    }

                    let result: Result<(), JobError> = match registry.handler_for(&job.job_type) {
                        Some(entry) => entry.run(&job, &ctx).await,
                        None => Err(JobError::new(
                            "UNKNOWN_JOB_TYPE",
                            format!("no handler for job_type={}", job.job_type),
                        )),
                    };

                    let latency_ms = start.elapsed().as_millis() as i32;
                    let outcome = match result {
                        Ok(()) => JobExecutionOutcome::Succeeded {
                            job_id: job.id,
                            attempt_id,
                            attempt_no,
                            latency_ms,
                        },
                        Err(err) => JobExecutionOutcome::Failed {
                            job_id: job.id,
                            attempt_id,
                            attempt_no,
                            max_attempts: job.max_attempts,
                            latency_ms,
                            error_code: err.code.to_string(),
                            error_message: err.message,
                        },
                    };

                    Ok::<JobExecutionOutcome, anyhow::Error>(outcome)
                });
            }

            let mut succeeded_batch: Vec<(Uuid, Uuid, i32)> = Vec::new();
            let mut failed_batch: Vec<(Uuid, Uuid, i32, i32, i32, String, String)> = Vec::new();

            while let Some(joined) = join_set.join_next().await {
                match joined?? {
                    JobExecutionOutcome::Succeeded {
                        job_id,
                        attempt_id,
                        attempt_no,
                        latency_ms,
                    } => {
                        if worker_verbose_job_logs {
                            println!(
                                "[{}] succeeded job id={} attempt_no={} latency_ms={}",
                                worker_id, job_id, attempt_no, latency_ms
                            );
                        }
                        succeeded_batch.push((job_id, attempt_id, latency_ms));
                    }
                    JobExecutionOutcome::Failed {
                        job_id,
                        attempt_id,
                        attempt_no,
                        max_attempts,
                        latency_ms,
                        error_code,
                        error_message,
                    } => {
                        failed_batch.push((
                            job_id,
                            attempt_id,
                            attempt_no,
                            max_attempts,
                            latency_ms,
                            error_code,
                            error_message,
                        ));
                    }
                }
            }

            runner
                .on_success_batch(&leased_dataset_id, &succeeded_batch, &worker_id)
                .await?;

            for (
                job_id,
                attempt_id,
                attempt_no,
                max_attempts,
                latency_ms,
                error_code,
                error_message,
            ) in failed_batch
            {
                runner
                    .on_failure(
                        job_id,
                        attempt_id,
                        &worker_id,
                        latency_ms,
                        &error_code,
                        &error_message,
                        attempt_no,
                        max_attempts,
                    )
                    .await?;
                if worker_verbose_job_logs {
                    println!(
                        "[{}] failed job id={} attempt_no={} code={} (retry/DLQ rules applied)",
                        worker_id, job_id, attempt_no, error_code
                    );
                }
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
