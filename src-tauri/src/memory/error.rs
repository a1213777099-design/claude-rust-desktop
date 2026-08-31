use thiserror::Error;

/// Unified error type for the memory system.
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Backup error: {0}")]
    Backup(String),

    #[error("Health check failed: {0}")]
    HealthCheck(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl MemoryError {
    /// Create a storage error with a message.
    pub fn storage(msg: impl Into<String>) -> Self {
        Self::Storage(msg.into())
    }

    /// Create a not-found error.
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::NotFound(id.into())
    }

    /// Create a backup error.
    pub fn backup(msg: impl Into<String>) -> Self {
        Self::Backup(msg.into())
    }

    /// Create a health-check error.
    pub fn health_check(msg: impl Into<String>) -> Self {
        Self::HealthCheck(msg.into())
    }

    /// Create a config error.
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }
}

/// Convenience alias for memory operation results.
pub type MemoryResult<T> = Result<T, MemoryError>;
