use postgresflow::jobs::Job;
use serde::Deserialize;
use sqlx::PgPool;
use std::{collections::HashMap, pin::Pin, sync::Arc, time::Duration};
use tokio::{sync::Semaphore, time::timeout};

pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;
type HandlerFn = dyn for<'a> Fn(&'a Job, &'a JobContext) -> BoxFuture<'a, Result<(), JobError>>
    + Send
    + Sync;

#[derive(Debug)]
pub struct JobError {
    pub code: &'static str,
    pub message: String,
}

impl JobError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone)]
pub struct JobContext {
    pub db: PgPool,
    pub worker_id: String,
}

#[derive(Clone)]
pub struct HandlerEntry {
    pub handler: Arc<HandlerFn>,
    pub semaphore: Option<Arc<Semaphore>>,
    pub timeout: Option<Duration>,
}

#[derive(Clone)]
pub struct HandlerRegistry {
    handlers: HashMap<String, HandlerEntry>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, job_type: &str, handler: F)
    where
        F: for<'a> Fn(&'a Job, &'a JobContext) -> BoxFuture<'a, Result<(), JobError>>
            + Send
            + Sync
            + 'static,
    {
        self.register_with_options(job_type, handler, HandlerOptions::new());
    }

    pub fn register_with_limit<F>(
        &mut self,
        job_type: &str,
        handler: F,
        max_concurrency: usize,
    )
    where
        F: for<'a> Fn(&'a Job, &'a JobContext) -> BoxFuture<'a, Result<(), JobError>>
            + Send
            + Sync
            + 'static,
    {
        self.register_with_options(
            job_type,
            handler,
            HandlerOptions::new().max_concurrency(max_concurrency),
        );
    }

    pub fn register_with_timeout<F>(
        &mut self,
        job_type: &str,
        handler: F,
        timeout_dur: Duration,
    ) where
        F: for<'a> Fn(&'a Job, &'a JobContext) -> BoxFuture<'a, Result<(), JobError>>
            + Send
            + Sync
            + 'static,
    {
        self.register_with_options(job_type, handler, HandlerOptions::new().timeout(timeout_dur));
    }

    pub fn register_with_options<F>(
        &mut self,
        job_type: &str,
        handler: F,
        opts: HandlerOptions,
    ) where
        F: for<'a> Fn(&'a Job, &'a JobContext) -> BoxFuture<'a, Result<(), JobError>>
            + Send
            + Sync
            + 'static,
    {
        let semaphore = opts
            .max_concurrency
            .map(|n| Arc::new(Semaphore::new(n.max(1))));
        self.handlers.insert(
            job_type.to_string(),
            HandlerEntry {
                handler: Arc::new(handler),
                semaphore,
                timeout: opts.timeout,
            },
        );
    }

    pub fn handler_for(&self, job_type: &str) -> Option<HandlerEntry> {
        self.handlers.get(job_type).cloned()
    }
}

#[derive(Clone, Debug)]
pub struct HandlerOptions {
    max_concurrency: Option<usize>,
    timeout: Option<Duration>,
}

impl HandlerOptions {
    pub fn new() -> Self {
        Self {
            max_concurrency: None,
            timeout: None,
        }
    }

    pub fn max_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = Some(n);
        self
    }

    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(dur);
        self
    }
}

impl HandlerEntry {
    pub async fn run(&self, job: &Job, ctx: &JobContext) -> Result<(), JobError> {
        let _permit = if let Some(sem) = &self.semaphore {
            Some(
                sem.clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| JobError::new("WORKER_SHUTDOWN", "handler semaphore closed"))?,
            )
        } else {
            None
        };

        let fut = (self.handler)(job, ctx);
        let res = if let Some(dur) = self.timeout {
            match timeout(dur, fut).await {
                Ok(inner) => inner,
                Err(_) => Err(JobError::new(
                    "TIMEOUT",
                    format!("handler timeout after {}ms", dur.as_millis()),
                )),
            }
        } else {
            fut.await
        };

        drop(_permit);
        res
    }
}

#[derive(Deserialize)]
struct EmailSendPayload {
    user_id: i64,
    template: Option<String>,
}

fn parse_payload<T: for<'de> Deserialize<'de>>(job: &Job) -> Result<T, JobError> {
    serde_json::from_value(job.payload_json.clone())
        .map_err(|e| JobError::new("BAD_PAYLOAD", e.to_string()))
}

fn boxed<'a, T>(
    fut: impl std::future::Future<Output = T> + Send + 'a,
) -> BoxFuture<'a, T> {
    Box::pin(fut)
}

pub fn build_registry() -> Arc<HandlerRegistry> {
    let mut registry = HandlerRegistry::new();

    // Demo handlers. Replace these with your real handlers.
    registry.register_with_timeout(
        "demo_ok",
        |_job, _ctx| {
            boxed(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                Ok(())
            })
        },
        Duration::from_secs(5),
    );
    registry.register_with_timeout(
        "fail_me",
        |_job, _ctx| {
            boxed(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                Err(JobError::new("TIMEOUT", "simulated timeout"))
            })
        },
        Duration::from_secs(5),
    );

    // Example handler with payload validation.
    registry.register_with_options(
        "email_send",
        |job, _ctx| {
            boxed(async move {
                let payload: EmailSendPayload = parse_payload(job)?;
                let _ = payload.user_id;
                let _ = payload.template;
                Ok(())
            })
        },
        HandlerOptions::new()
            .max_concurrency(50)
            .timeout(Duration::from_secs(10)),
    );

    Arc::new(registry)
}
