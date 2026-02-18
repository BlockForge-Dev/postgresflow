use rand::Rng;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub base_seconds: i64,
    pub max_seconds: i64,
    pub jitter_pct: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            base_seconds: 2,
            max_seconds: 15 * 60,
            jitter_pct: 0.20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Retryable,
    NonRetryable,
}

pub fn classify_error(code: &str) -> ErrorClass {
    match code {
        "TIMEOUT" | "DEPENDENCY_DOWN" | "RATE_LIMIT" | "DB_DEADLOCK" | "SERIALIZATION" => {
            ErrorClass::Retryable
        }
        "BAD_PAYLOAD" | "UNKNOWN_JOB_TYPE" => ErrorClass::NonRetryable,
        _ => ErrorClass::Retryable,
    }
}

pub fn next_delay_seconds(attempt_no: i32, cfg: &RetryConfig, rng: &mut impl Rng) -> i64 {
    let attempt_no = attempt_no.max(1) as u32;

    // exponent = attempt_no - 1
    let exp = attempt_no.saturating_sub(1);

    // Compute 2^exp safely. If exp is too large, treat multiplier as huge and let cap handle it.
    let pow2 = 1_i64.checked_shl(exp).unwrap_or(i64::MAX);

    // base * 2^(attempt_no-1) with overflow protection
    let mut delay = cfg.base_seconds.saturating_mul(pow2);

    // cap
    if delay > cfg.max_seconds {
        delay = cfg.max_seconds;
    }

    // jitter in range [-jitter_pct, +jitter_pct]
    let jitter_range = (delay as f64) * cfg.jitter_pct;
    let jitter = rng.gen_range(-jitter_range..=jitter_range);

    let jittered = (delay as f64 + jitter).round() as i64;
    jittered.clamp(0, cfg.max_seconds)
}
