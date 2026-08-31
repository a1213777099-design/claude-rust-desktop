use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for the memory system.
///
/// Controls storage backend, encryption, embedding, pruning, and scheduling.
/// All fields have sensible defaults — minimal config needed for basic use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Directory for the SQLite database file(s).
    /// Default: `{data_local_dir}/claude-desktop/memories/`
    pub storage_dir: PathBuf,

    /// Database filename (within storage_dir).
    /// Default: `memories.db`
    pub database_name: String,

    /// Enable WAL journal mode for better concurrent reads.
    /// Default: true
    pub wal_mode: bool,

    /// Enable synchronous=NORMAL for balance of safety and speed.
    /// Default: true
    pub synchronous_normal: bool,

    /// Max connection pool size (for future connection pooling).
    /// Default: 4
    pub max_connections: u32,

    /// Encryption passphrase (if empty, encryption is disabled).
    /// Default: empty (no encryption at rest)
    pub encryption_passphrase: String,

    /// Encryption key derivation iterations (PBKDF2).
    /// Default: 600_000 (OWASP recommended)
    pub encryption_iterations: u32,

    /// Backup schedule in cron format.
    /// Default: `0 3 * * *` (daily at 3am)
    pub backup_cron: String,

    /// Max number of backup files to retain.
    /// Default: 7
    pub backup_retention_days: u32,

    /// Backup directory (if different from storage_dir/backups).
    pub backup_dir: Option<PathBuf>,

    /// Enable automatic health checks.
    /// Default: true
    pub health_check_enabled: bool,

    /// Health check interval in seconds.
    /// Default: 3600 (1 hour)
    pub health_check_interval_secs: u64,

    /// Enable automatic pruning of old/low-importance memories.
    /// Default: true
    pub pruning_enabled: bool,

    /// Max memories per workspace before pruning kicks in.
    /// Default: 200
    pub max_memories_per_workspace: usize,

    /// Importance threshold below which memories may be pruned.
    /// Default: 2
    pub pruning_importance_threshold: i32,

    /// Embedding model to use (if empty, embedding is disabled).
    /// Default: `all-MiniLM-L6-v2`
    pub embedding_model: String,

    /// Embedding dimension for the selected model.
    /// Default: 384
    pub embedding_dimension: usize,

    /// Enable FTS5 full-text search indexes.
    /// Default: true
    pub fts5_enabled: bool,

    /// Custom pool configuration — reserved for future use.
    #[serde(skip)]
    pub(crate) _custom_pool_config: Option<serde_json::Value>,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("claude-desktop");

        Self {
            storage_dir: data_dir.join("memories"),
            database_name: "memories.db".to_string(),
            wal_mode: true,
            synchronous_normal: true,
            max_connections: 4,
            encryption_passphrase: String::new(),
            encryption_iterations: 600_000,
            backup_cron: "0 3 * * *".to_string(),
            backup_retention_days: 7,
            backup_dir: None,
            health_check_enabled: true,
            health_check_interval_secs: 3600,
            pruning_enabled: true,
            max_memories_per_workspace: 200,
            pruning_importance_threshold: 2,
            embedding_model: "all-MiniLM-L6-v2".to_string(),
            embedding_dimension: 384,
            fts5_enabled: true,
            _custom_pool_config: None,
        }
    }
}

impl MemoryConfig {
    /// Path to the SQLite database file.
    pub fn database_path(&self) -> PathBuf {
        self.storage_dir.join(&self.database_name)
    }

    /// Path to the backup directory.
    pub fn backup_path(&self) -> PathBuf {
        self.backup_dir
            .clone()
            .unwrap_or_else(|| self.storage_dir.join("backups"))
    }

    /// Whether encryption is enabled.
    pub fn encryption_enabled(&self) -> bool {
        !self.encryption_passphrase.is_empty()
    }

    /// Validate the configuration, returning an error if anything is invalid.
    pub fn validate(&self) -> Result<(), crate::memory::error::MemoryError> {
        use crate::memory::error::MemoryError;

        if self.database_name.trim().is_empty() {
            return Err(MemoryError::config("database_name must not be empty"));
        }
        if self.max_connections == 0 {
            return Err(MemoryError::config("max_connections must be >= 1"));
        }
        if self.backup_retention_days == 0 {
            return Err(MemoryError::config("backup_retention_days must be >= 1"));
        }
        if self.health_check_interval_secs == 0 {
            return Err(MemoryError::config("health_check_interval_secs must be >= 1"));
        }
        if self.pruning_enabled && self.max_memories_per_workspace == 0 {
            return Err(MemoryError::config("max_memories_per_workspace must be >= 1"));
        }
        Ok(())
    }
}
