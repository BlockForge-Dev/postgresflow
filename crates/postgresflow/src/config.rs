// Clone: lets you safely duplicate the config

#[derive(Clone, Debug)]

// Config is a central place for runtime configuration

// It loads values from environment variables

// It gives you a typed, validated struct instead of raw strings everywhere
pub struct Config {
    pub database_url: String,
    pub worker_id: String,
    pub queue: String,
    pub lease_seconds: i64,
    pub dequeue_batch_size: i64,
    pub reap_interval_ms: u64,
    pub verbose_job_logs: bool,
    pub admin_addr: Option<String>,
    pub api_token: Option<String>,
    pub migrate_on_startup: bool,
    pub max_payload_bytes: usize,
    pub max_enqueues_per_minute_per_queue: i64,
}

impl Config {
    //Result<String, VarError>
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL is missing"))?;
        //.map_err(...) converts that error into an anyhow::Error
        //std::env::var returns Result<String, VarError>

        let worker_id = env_or_fallback("PGFLOW_WORKER_ID", "WORKER_ID")
            .or_else(|| std::env::var("HOSTNAME").ok())
            .unwrap_or_else(|| "worker-1".to_string());

        let queue =
            env_or_fallback("PGFLOW_QUEUE", "QUEUE").unwrap_or_else(|| "default".to_string());

        let lease_seconds = env_or_fallback("PGFLOW_LEASE_SECONDS", "LEASE_SECONDS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let dequeue_batch_size = env_or_fallback("PGFLOW_DEQUEUE_BATCH_SIZE", "DEQUEUE_BATCH_SIZE")
            .and_then(|s| s.parse().ok())
            .unwrap_or(256)
            .clamp(1, 4096);

        let reap_interval_ms = env_or_fallback("PGFLOW_REAP_INTERVAL_MS", "REAP_INTERVAL_MS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(5_000)
            .clamp(250, 60_000);

        let verbose_job_logs = env_bool("PGFLOW_VERBOSE_JOB_LOGS").unwrap_or(false);

        let admin_addr = env_or_fallback("PGFLOW_ADMIN_ADDR", "ADMIN_ADDR")
            .and_then(|s| normalize_optional_addr(&s));

        let api_token = env_or_fallback("PGFLOW_API_TOKEN", "API_TOKEN");

        let migrate_on_startup = env_bool("PGFLOW_MIGRATE_ON_STARTUP").unwrap_or(false);

        let max_payload_bytes = env_or_fallback("PGFLOW_MAX_PAYLOAD_BYTES", "MAX_PAYLOAD_BYTES")
            .and_then(|s| s.parse().ok())
            .unwrap_or(256 * 1024);

        let max_enqueues_per_minute_per_queue =
            env_or_fallback("PGFLOW_MAX_ENQUEUE_PER_MINUTE", "MAX_ENQUEUE_PER_MINUTE")
                .and_then(|s| s.parse().ok())
                .unwrap_or(10_000);

        Ok(Self {
            database_url,
            worker_id,
            queue,
            lease_seconds,
            dequeue_batch_size,
            reap_interval_ms,
            verbose_job_logs,
            admin_addr,
            api_token,
            migrate_on_startup,
            max_payload_bytes,
            max_enqueues_per_minute_per_queue,
        })
    }

    //   Construct a Config

    // Wrap it in Ok

    // Return it to the caller
}

fn env_or_fallback(primary: &str, fallback: &str) -> Option<String> {
    std::env::var(primary)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var(fallback)
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
}

fn env_bool(key: &str) -> Option<bool> {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

fn normalize_optional_addr(value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    if matches!(v.to_lowercase().as_str(), "0" | "off" | "false" | "none") {
        return None;
    }
    Some(v.to_string())
}
