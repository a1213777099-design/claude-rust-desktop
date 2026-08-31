/// Tool error classification for retry decisions.
///
/// Transient errors can be retried; permanent errors should not.
use std::fmt;

/// Classified tool execution error.
#[derive(Debug, Clone)]
pub enum ToolError {
    /// Transient errors that may succeed on retry (network, timeout, rate limit).
    Transient(String),
    /// Permanent errors that won't succeed on retry (file not found, validation, auth).
    Permanent(String),
    /// Permission errors requiring user input.
    Permission(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::Transient(msg) => write!(f, "Transient: {}", msg),
            ToolError::Permanent(msg) => write!(f, "Permanent: {}", msg),
            ToolError::Permission(msg) => write!(f, "Permission: {}", msg),
        }
    }
}

impl ToolError {
    /// Classify a raw error message into the appropriate category.
    pub fn classify(error_msg: &str) -> Self {
        let lower = error_msg.to_lowercase();

        // Permission errors
        if lower.contains("permission denied")
            || lower.contains("access denied")
            || lower.contains("not authorized")
            || lower.contains("forbidden")
            || lower.contains("403")
        {
            return ToolError::Permission(error_msg.to_string());
        }

        // Transient errors (retryable)
        if lower.contains("timeout")
            || lower.contains("timed out")
            || lower.contains("connection refused")
            || lower.contains("connection reset")
            || lower.contains("network")
            || lower.contains("429")
            || lower.contains("rate limit")
            || lower.contains("too many requests")
            || lower.contains("502")
            || lower.contains("503")
            || lower.contains("504")
            || lower.contains("server error")
            || lower.contains("eof")
            || lower.contains("broken pipe")
        {
            return ToolError::Transient(error_msg.to_string());
        }

        // Everything else is permanent
        ToolError::Permanent(error_msg.to_string())
    }

    /// Whether this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ToolError::Transient(_))
    }
}

/// Retry configuration for tool execution.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: usize,
    /// Base delay in milliseconds for exponential backoff.
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds (cap for backoff).
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            base_delay_ms: 500,
            max_delay_ms: 4000,
        }
    }
}

impl RetryConfig {
    /// Calculate the delay for a given attempt number using exponential backoff.
    pub fn delay_for_attempt(&self, attempt: usize) -> u64 {
        let delay = self.base_delay_ms * (1u64 << attempt as u32);
        delay.min(self.max_delay_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_timeout() {
        let err = ToolError::classify("Connection timed out after 30s");
        assert!(err.is_retryable());
        assert!(matches!(err, ToolError::Transient(_)));
    }

    #[test]
    fn test_classify_rate_limit() {
        let err = ToolError::classify("429 Too Many Requests");
        assert!(err.is_retryable());
    }

    #[test]
    fn test_classify_permission() {
        let err = ToolError::classify("Permission denied: /etc/shadow");
        assert!(!err.is_retryable());
        assert!(matches!(err, ToolError::Permission(_)));
    }

    #[test]
    fn test_classify_file_not_found() {
        let err = ToolError::classify("No such file or directory");
        assert!(!err.is_retryable());
        assert!(matches!(err, ToolError::Permanent(_)));
    }

    #[test]
    fn test_classify_503() {
        let err = ToolError::classify("HTTP 503 Service Unavailable");
        assert!(err.is_retryable());
    }

    #[test]
    fn test_retry_delay() {
        let config = RetryConfig::default();
        assert_eq!(config.delay_for_attempt(0), 500);
        assert_eq!(config.delay_for_attempt(1), 1000);
        assert_eq!(config.delay_for_attempt(2), 2000);
        assert_eq!(config.delay_for_attempt(3), 4000); // capped
        assert_eq!(config.delay_for_attempt(10), 4000); // still capped
    }
}
