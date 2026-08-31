use crate::clipboard::ClipboardManager;
use crate::config::ConfigManager;
use crate::db::DbManager;
use crate::engine::EnginePool;
use crate::logger::Logger;
use crate::mcp::McpServerManager;
use crate::native_engine::NativeEngine;
use crate::notification::NotificationManager;
use crate::process::ProcessManager;
use crate::research::ResearchEvent;
use crate::skills::SkillsManager;
use crate::streaming::StreamManager;
use crate::task::TaskExecutor;
use crate::terminal::PtyManager;
use crate::watcher::FileWatcher;
use std::collections::HashMap;
use crate::memory::embedding::EmbeddingEngine;
use crate::memory::tencentdb_client::TdaiClient;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// Research task handle for active deep-research pipelines.
pub struct ResearchTask {
    pub handle: tokio::task::JoinHandle<()>,
    pub event_tx: broadcast::Sender<ResearchEvent>,
}

/// Shared application state passed to all Axum route handlers.
///
/// Replaces the previous 17-element tuple with named fields
/// for readability and maintainability.
#[derive(Clone)]
pub struct AppState {
    pub engine_pool: Arc<Mutex<EnginePool>>,
    pub mcp_server_manager: Arc<McpServerManager>,
    pub stream_manager: Arc<Mutex<StreamManager>>,
    pub research_mode: Arc<Mutex<HashMap<String, bool>>>,
    pub config_manager: Arc<Mutex<Option<ConfigManager>>>,
    pub skill_manager: Arc<Mutex<SkillsManager>>,
    pub db_manager: Arc<DbManager>,
    pub task_executor: Arc<Mutex<Option<TaskExecutor>>>,
    pub process_manager: Arc<Mutex<ProcessManager>>,
    pub terminal_manager: Arc<Mutex<PtyManager>>,
    pub file_watcher: Arc<Mutex<FileWatcher>>,
    pub clipboard_manager: Arc<Mutex<ClipboardManager>>,
    pub notification_manager: Arc<Mutex<NotificationManager>>,
    pub logger: Arc<Mutex<Logger>>,
    pub native_engine: Arc<Mutex<Option<NativeEngine>>>,
    pub active_research: Arc<Mutex<HashMap<String, ResearchTask>>>,
    pub embedding_engine: Arc<EmbeddingEngine>,
    pub tdai_client: Arc<TdaiClient>,
}
