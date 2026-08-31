use crate::memory::error::MemoryResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// A single memory record used across all storage backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub workspace_path: String,
    pub conversation_id: String,
    pub summary: String,
    pub tags: String,
    pub memory_type: String,
    pub importance: i32,
    pub created_at: String,
    /// Optional embedding vector (f32 normalized).
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

/// Memory search query parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// Full-text search query string.
    pub query: String,
    /// Optional workspace filter.
    pub workspace_path: Option<String>,
    /// Optional memory type filter.
    pub memory_type: Option<String>,
    /// Minimum importance filter (1–5).
    pub min_importance: Option<i32>,
    /// Max results to return.
    pub limit: usize,
    /// If true, sort by importance first; otherwise by recency.
    pub sort_by_importance: bool,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            workspace_path: None,
            memory_type: None,
            min_importance: None,
            limit: 100,
            sort_by_importance: true,
        }
    }
}

/// Health status for the storage backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub database_size_bytes: u64,
    pub total_memories: u64,
    pub wal_size_bytes: Option<u64>,
    pub last_backup_time: Option<String>,
    pub last_health_check: String,
    pub errors: Vec<String>,
}

/// Storage statistics / metrics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_memories: u64,
    pub total_workspaces: u64,
    pub memory_type_distribution: std::collections::HashMap<String, u64>,
    pub importance_distribution: std::collections::HashMap<i32, u64>,
    pub database_size_bytes: u64,
    pub average_importance: f64,
}

/// **MemoryStorage trait** — the core storage abstraction.
///
/// All storage backends (SQLite, mock, future cloud backends) implement this.
/// Methods are async to support future non-blocking backends.
#[async_trait]
pub trait MemoryStorage: Debug + Send + Sync + 'static {
    /// Initialize the storage backend (create tables, indices, etc.).
    async fn initialize(&self) -> MemoryResult<()>;

    /// Insert a new memory record.
    /// Returns an error if a record with the same `id` already exists.
    async fn insert(&self, record: MemoryRecord) -> MemoryResult<()>;

    /// Insert a memory record, replacing any existing record with the same `id`.
    async fn upsert(&self, record: MemoryRecord) -> MemoryResult<()>;

    /// Get a single memory record by ID.
    async fn get(&self, id: &str) -> MemoryResult<Option<MemoryRecord>>;

    /// Update the summary and/or importance of an existing memory.
    async fn update(
        &self,
        id: &str,
        summary: Option<&str>,
        tags: Option<&str>,
        importance: Option<i32>,
    ) -> MemoryResult<()>;

    /// Delete a memory record by ID.
    async fn delete(&self, id: &str) -> MemoryResult<bool>;

    /// Search memories using the given query.
    async fn search(&self, query: &MemoryQuery) -> MemoryResult<Vec<MemoryRecord>>;

    /// List recent memories for a workspace, ordered by importance then recency.
    async fn list_recent(
        &self,
        workspace_path: &str,
        limit: usize,
    ) -> MemoryResult<Vec<MemoryRecord>>;

    /// List all memories across all workspaces (for admin/backup).
    async fn list_all(&self, limit: usize) -> MemoryResult<Vec<MemoryRecord>>;

    /// Get high-importance memories (importance >= 4) for a workspace.
    async fn get_important(
        &self,
        workspace_path: &str,
        limit: usize,
    ) -> MemoryResult<Vec<MemoryRecord>>;

    /// Count memories for a workspace.
    async fn count(&self, workspace_path: &str) -> MemoryResult<u64>;

    /// Count all memories across all workspaces.
    async fn count_all(&self) -> MemoryResult<u64>;

    /// Collect storage statistics.
    async fn stats(&self) -> MemoryResult<StorageStats>;

    /// Run a health check, returning the health status.
    async fn health_check(&self) -> MemoryResult<HealthStatus>;

    /// Create a backup of the storage to the given path.
    /// Returns the path to the backup file.
    async fn backup(&self, backup_path: &str) -> MemoryResult<String>;

    /// Restore from a backup file. **Destructive** — replaces current data.
    async fn restore(&self, backup_path: &str) -> MemoryResult<()>;

    /// Prune old / low-importance memories beyond the configured limit.
    /// Returns the number of memories removed.
    async fn prune(
        &self,
        workspace_path: &str,
        max_memories: usize,
        importance_threshold: i32,
    ) -> MemoryResult<u64>;

    /// Consolidate duplicate memories for a workspace.
    /// Returns the number of duplicates removed.
    async fn consolidate(&self, workspace_path: &str) -> MemoryResult<u64>;

    /// Run a raw SQL query (for admin / debugging purposes).
    /// Only supported on SQL-based backends; may return an error on others.
    async fn raw_query(&self, sql: &str) -> MemoryResult<Vec<serde_json::Value>>;
}
