// src/jobs/error_codes.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Timeout,
    DbDeadlock,
    Serialization,
    RateLimit,
    Panic,
    BadPayload,
    DependencyDown,
    Unknown,
}

impl ErrorCode {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_uppercase().as_str() {
            "TIMEOUT" => Self::Timeout,
            "DB_DEADLOCK" => Self::DbDeadlock,
            "SERIALIZATION" => Self::Serialization,
            "RATE_LIMIT" => Self::RateLimit,
            "PANIC" => Self::Panic,
            "BAD_PAYLOAD" => Self::BadPayload,
            "DEPENDENCY_DOWN" => Self::DependencyDown,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Timeout => "TIMEOUT",
            Self::DbDeadlock => "DB_DEADLOCK",
            Self::Serialization => "SERIALIZATION",
            Self::RateLimit => "RATE_LIMIT",
            Self::Panic => "PANIC",
            Self::BadPayload => "BAD_PAYLOAD",
            Self::DependencyDown => "DEPENDENCY_DOWN",
            Self::Unknown => "UNKNOWN",
        }
    }
}

pub fn suggested_action(code: &str) -> &'static str {
    match ErrorCode::from_str(code) {
        ErrorCode::Timeout => {
            "Increase timeout OR reduce payload/work. Check downstream latency and retries."
        }
        ErrorCode::DbDeadlock => {
            "Retry is OK. Reduce lock contention: consistent row ordering, smaller transactions."
        }
        ErrorCode::Serialization => {
            "Retry is OK. Use SERIALIZABLE retry loop / reduce concurrent writes / use lower isolation if acceptable."
        }
        ErrorCode::RateLimit => {
            "Back off. Add client-side rate limiting, respect Retry-After, lower concurrency."
        }
        ErrorCode::Panic => {
            "Investigate crash. Capture panic info, add safeguards, consider marking non-retryable if deterministic."
        }
        ErrorCode::BadPayload => {
            "Non-retryable. Validate payload schema/fields. Fix producer or add transform step."
        }
        ErrorCode::DependencyDown => {
            "Retry later. Check dependency health, circuit-break, alerting, fallback path."
        }
        ErrorCode::Unknown => {
            "Inspect error_message + logs. Decide if retryable; add mapping once understood."
        }
    }
}
