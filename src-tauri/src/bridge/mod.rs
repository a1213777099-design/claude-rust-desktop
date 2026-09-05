pub mod state;
pub mod memory_handlers_v2;

use crate::clipboard::ClipboardManager;
use crate::config::{AppConfig, ConfigManager};
use crate::db::DbManager;
use crate::engine::{EnginePool};
use crate::git::GitIntegration;
use crate::logger::Logger;

macro_rules! log_to_file {
    ($($arg:tt)*) => {
        tracing::debug!(target: "chat", $($arg)*);
    };
}

use crate::mcp::{McpServerManager, McpServerConfig};
use crate::native_engine::{NativeEngine, ProviderManager};
use crate::notification::NotificationManager;
use crate::permissions::{AuditLogger, PermissionManager, PermissionMode};
use crate::process::ProcessManager;
use crate::research::{ResearchEvent, ResearchOrchestrator, ResearchRequest};
use crate::multiagent::{MultiAgentOrchestrator as PipelineOrchestrator, OrchestratorConfig};
use crate::skills::{Skill, SkillsManager, SkillExecutionContext};
use crate::streaming::{StreamManager};
use crate::task::{TaskExecutor, TaskRequest, TaskResult};
use crate::terminal::PtyManager;
use crate::updater::AutoUpdater;
use crate::watcher::FileWatcher;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State, Multipart, DefaultBodyLimit},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tower_http::cors::{CorsLayer, AllowOrigin};
use axum::response::IntoResponse;
use axum::http::header::{HeaderName, ORIGIN, CONTENT_TYPE, AUTHORIZATION, ACCEPT};
use axum::http::Method;

use crate::tools::{execute_tool, get_tool_definitions, ToolDefinition};
pub use state::{AppState, ResearchTask};

#[derive(Clone)]
pub struct BridgeServer {
    engine_pool: Arc<Mutex<EnginePool>>,
    native_engine: Arc<Mutex<Option<NativeEngine>>>,
    mcp_server_manager: Arc<McpServerManager>,
    stream_manager: Arc<Mutex<StreamManager>>,
    research_mode: Arc<Mutex<HashMap<String, bool>>>,
    config_manager: Arc<Mutex<Option<ConfigManager>>>,
    skill_manager: Arc<Mutex<SkillsManager>>,
    db_manager: Arc<DbManager>,
    task_executor: Arc<Mutex<Option<TaskExecutor>>>,
    process_manager: Arc<Mutex<ProcessManager>>,
    terminal_manager: Arc<Mutex<PtyManager>>,
    file_watcher: Arc<Mutex<FileWatcher>>,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
    notification_manager: Arc<Mutex<NotificationManager>>,
    logger: Arc<Mutex<Logger>>,
    active_research: Arc<Mutex<HashMap<String, ResearchTask>>>,
    tdai_client: Arc<crate::memory::tencentdb_client::TdaiClient>,
}


#[derive(Serialize, Deserialize, Clone)]
pub struct ChatRequest {
    pub conversation_id: String,
    pub message: Option<String>,
    pub model: String,
    pub user_mode: Option<String>,
    pub env_token: Option<String>,
    pub env_base_url: Option<String>,
    pub research_mode: Option<bool>,
    pub enable_streaming: Option<bool>,
    pub custom_system_prompt: Option<String>,
    pub permission_mode: Option<String>,
    pub web_search_enabled: Option<bool>,
    pub reasoning_effort: Option<String>,
    pub extended_thinking: Option<bool>,
}

impl ChatRequest {
    pub fn single_message(&self) -> Option<serde_json::Value> {
        self.message.as_ref().map(|msg| serde_json::json!({
            "role": "user",
            "content": msg
        }))
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ToolRequest {
    pub name: String,
    pub input: serde_json::Value,
    pub cwd: Option<String>,
}

#[derive(Serialize)]
pub struct SystemStatus {
    pub platform: String,
    pub git_bash: GitBashStatus,
}

#[derive(Serialize)]
pub struct GitBashStatus {
    pub required: bool,
    pub found: bool,
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct StreamQuery {
    pub conversation_id: String,
    pub model: String,
    pub user_mode: Option<String>,
    pub env_token: Option<String>,
    pub env_base_url: Option<String>,
    pub research_mode: Option<bool>,
    pub messages: Option<String>,
}

impl BridgeServer {
    pub fn new(data_dir: PathBuf) -> Self {
        let _skills_dir = data_dir.join("skills");
        let log_dir = data_dir.join("logs");

        let skill_manager = SkillsManager::new();
        if let Err(e) = skill_manager.install_bundled_skills() {
            tracing::error!(target: "bridge", "Failed to install bundled skills: {}", e);
        }

        let db_path = data_dir.join("claude_desktop.db");
        let db_manager = DbManager::new(db_path.clone()).expect("Failed to initialize database");
        db_manager.init().expect("Failed to initialize database schema");
        tracing::info!(target: "bridge", "[Bridge] Database initialized at {:?}", db_path);
        tracing::info!(target: "bridge", "[Bridge] Running migration check...");
        {
            let data_dir_ref = &data_dir;
            db_manager.with_conn(|conn| {
                if let Err(e) = crate::db::migration::check_and_migrate(data_dir_ref, conn) {
                    tracing::warn!(target: "bridge", "Migration warning: {}", e);
                }
            }).ok();
        }
        tracing::info!(target: "bridge", "[Bridge] Migration check completed");
        let db_manager = Arc::new(db_manager);
        let logger = Logger::new(log_dir);
        let file_watcher = FileWatcher::new();

        let config_dir = data_dir.clone();
        let config_manager = ConfigManager::new(config_dir.clone());
        tracing::info!(target: "bridge", "[Bridge] ConfigManager initialized at {:?}", data_dir.display());
        let config_manager = Arc::new(Mutex::new(Some(config_manager)));

        let provider_manager = Arc::new(Mutex::new(ProviderManager::new(
            data_dir.join("providers.json")
        )));
        let task_executor = TaskExecutor::new_with_provider_manager(
            provider_manager.clone(),
            db_manager.clone(),
        );
        
        let audit_logger = Arc::new(AuditLogger::new(1000));
        let permission_manager = Arc::new(PermissionManager::new(audit_logger));
        
        let tdai_client = Arc::new(crate::memory::tencentdb_client::TdaiClient::new(
            db_manager
                .with_conn(|c| crate::memory::tencentdb_client::load_config(c))
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default()
        ));
        // One-time legacy -> tiered memory migration (dedup makes reruns safe)
        match db_manager.with_conn(|c| crate::memory::tiered::migrate_legacy_memories(c)) {
            Ok(Ok(n)) if n > 0 => tracing::info!(target: "bridge", "[Bridge] Migrated {} legacy memories into tiered store", n),
            Ok(_) => {}
            Err(e) => tracing::warn!(target: "bridge", "[Bridge] Legacy memory migration failed: {}", e),
        }
        let native_engine = Arc::new(Mutex::new(Some(NativeEngine::new(
            provider_manager,
            db_manager.clone(),
            data_dir.join("workspaces"),
            permission_manager,
            tdai_client.clone(),
        ))));
        tracing::info!(target: "bridge", "[Bridge] NativeEngine initialized");

        let config_path = std::path::Path::new("config/orchestration.toml");
                tracing::info!(target: "bridge", "[Bridge] MultiAgentOrchestrator initialized");

        Self {
            engine_pool: Arc::new(Mutex::new(EnginePool::new())),
            native_engine,
            mcp_server_manager: Arc::new(McpServerManager::new(config_dir.join("mcp-servers.json"))),
            stream_manager: Arc::new(Mutex::new(StreamManager::new())),
            research_mode: Arc::new(Mutex::new(HashMap::new())),
            config_manager,
            skill_manager: Arc::new(Mutex::new(skill_manager)),
            db_manager,
            task_executor: Arc::new(Mutex::new(Some(task_executor))),
            process_manager: Arc::new(Mutex::new(ProcessManager::new())),
            terminal_manager: Arc::new(Mutex::new(PtyManager::new())),
            file_watcher: Arc::new(Mutex::new(file_watcher)),
            clipboard_manager: Arc::new(Mutex::new(ClipboardManager::new())),
            notification_manager: Arc::new(Mutex::new(NotificationManager::new())),
            logger: Arc::new(Mutex::new(logger)),
            active_research: Arc::new(Mutex::new(HashMap::new())),
            tdai_client,
        }
    }

    /// Try ports starting from base, using SO_REUSEADDR to recover from zombies.
    pub async fn start_with_fallback(&self, base_port: u16) -> Result<()> {
        for offset in 0..10 {
            let port = base_port + offset;
            tracing::info!(target: "bridge", "Trying port {}...", port);
            match self.start(port).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!(target: "bridge", "Port {} failed: {}", port, e);
                }
            }
        }
        Err(anyhow::anyhow!("All ports exhausted"))
    }

    pub async fn start(&self, port: u16) -> Result<()> {
        if let Err(e) = self.mcp_server_manager.initialize().await {
            tracing::error!(target: "bridge", "Failed to initialize MCP server manager: {}", e);
        }

        // Initialize embedding engine (auto-detect: fastembed ONNX → TF-IDF fallback)
        let embedding_engine = Arc::new(crate::memory::embedding::EmbeddingEngine::new_auto(384).await);
        // Use the shared TdaiClient instance (initialized in BridgeServer::new)
        let tdai_client = self.tdai_client.clone();
        let state = AppState {
            engine_pool: self.engine_pool.clone(),
            mcp_server_manager: self.mcp_server_manager.clone(),
            stream_manager: self.stream_manager.clone(),
            research_mode: self.research_mode.clone(),
            config_manager: self.config_manager.clone(),
            skill_manager: self.skill_manager.clone(),
            db_manager: self.db_manager.clone(),
            task_executor: self.task_executor.clone(),
            process_manager: self.process_manager.clone(),
            terminal_manager: self.terminal_manager.clone(),
            file_watcher: self.file_watcher.clone(),
            clipboard_manager: self.clipboard_manager.clone(),
            notification_manager: self.notification_manager.clone(),
            logger: self.logger.clone(),
            native_engine: self.native_engine.clone(),
            active_research: self.active_research.clone(),
            embedding_engine,
            tdai_client,
        };
        tracing::info!(target: "bridge", "Database manager ready");

        let allowed_origins = vec![
            "tauri://localhost".parse::<axum::http::HeaderValue>().unwrap(),
            "https://tauri.localhost".parse::<axum::http::HeaderValue>().unwrap(),
            "http://tauri.localhost".parse::<axum::http::HeaderValue>().unwrap(),
            "http://localhost:1420".parse::<axum::http::HeaderValue>().unwrap(),
            "http://localhost:3456".parse::<axum::http::HeaderValue>().unwrap(),
            "http://localhost:5173".parse::<axum::http::HeaderValue>().unwrap(),
            "http://localhost:5175".parse::<axum::http::HeaderValue>().unwrap(),
            "http://127.0.0.1:1420".parse::<axum::http::HeaderValue>().unwrap(),
            "http://127.0.0.1:3456".parse::<axum::http::HeaderValue>().unwrap(),
            "http://127.0.0.1:5173".parse::<axum::http::HeaderValue>().unwrap(),
            "http://127.0.0.1:5175".parse::<axum::http::HeaderValue>().unwrap(),
            "null".parse::<axum::http::HeaderValue>().unwrap(),
        ];

        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::list(allowed_origins))
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE, Method::OPTIONS])
            .allow_headers([
                CONTENT_TYPE,
                AUTHORIZATION,
                ACCEPT,
                ORIGIN,
                HeaderName::from_static("x-conversation-id"),
            ]);

        let app = Router::new()
            .route("/api/system-status", get(system_status))
            .route("/api/browser/view", get(browser_view_handler))
            .route("/api/browser/status", get(browser_status_handler))
            .route("/api/browser/navigate", post(browser_navigate_handler))
            .route("/api/browser/interact", post(browser_interact_handler))
            .route("/api/browser/snapshot", get(browser_snapshot_handler))
            .route("/api/open-folder", post(open_folder_handler))
            .route("/api/workspace-config", get(workspace_config_get))
            .route("/api/workspace-config", post(workspace_config_set))
            .route("/api/chat", post(chat_handler))
            .route("/api/chat/stream", get(chat_stream_handler))
            .route("/api/tools", post(tools_handler))
            .route("/api/tools/list", get(tools_list_handler))
            .route("/api/tools/execute", post(tool_execute_handler))
            .route("/api/conversations", get(conversations_list))
            .route("/api/conversations", post(conversations_create))
            .route("/api/conversations/{id}", get(conversation_get))
            .route("/api/conversations/{id}", post(conversation_update))
            .route("/api/conversations/{id}", patch(conversation_patch))
            .route("/api/conversations/{id}", delete(conversation_delete))
            .route("/api/conversations/{id}/messages", get(conversation_messages))
            .route("/api/conversations/{id}/messages/{mid}", delete(conversation_message_delete))
            .route("/api/conversations/{id}/messages-tail/{count}", delete(conversation_messages_tail_delete))
            .route("/api/conversations/{id}/branch", post(conversation_branch_handler))
            .route("/api/conversations/{id}/answer", post(conversation_answer_handler))
            .route("/api/conversations/{id}/permission", post(conversation_permission_handler))
            .route("/api/conversations/{id}/warm", post(conversation_warm_handler))
            .route("/api/conversations/{id}/context-size", get(context_size_handler))
            .route("/api/conversations/{id}/stream-status", get(conversation_stream_status_handler))
            .route("/api/conversations/{id}/reconnect", get(conversation_reconnect_handler))
            .route("/api/conversations/{id}/compact", post(compact_handler))
            .route("/api/projects", get(projects_list).post(projects_create))
            .route("/api/projects/{id}", get(projects_get).patch(projects_update).delete(projects_delete))
            .route("/api/projects/{id}/conversations", get(project_conversations_list).post(project_conversation_create))
            .route("/api/projects/{id}/files", post(project_file_upload))
            .route("/api/projects/{id}/files/{file_id}", delete(project_file_delete))
            .route("/api/upload", post(upload_handler))
            .route("/api/uploads/{id}/raw", get(upload_get_handler))
            .route("/api/uploads/{id}", delete(upload_delete_handler))
            .route("/api/providers", get(providers_list))
            .route("/api/providers", post(providers_create))
            .route("/api/providers/models", get(providers_models_list))
            .route("/api/providers/{id}", patch(providers_patch))
            .route("/api/providers/{id}", delete(providers_delete))
            .route("/api/providers/{id}/test-websearch", post(providers_test_websearch))
            .route("/api/config", get(config_get))
            .route("/api/config", post(config_update))
            .route("/api/skills", get(skills_list))
            .route("/api/skills", post(skills_create))
            .route("/api/skills/{name}", get(skill_get))
            .route("/api/skills/{name}", put(skill_update))
            .route("/api/skills/{name}", delete(skill_delete))
            .route("/api/skills/{name}/enable", post(skill_enable))
            .route("/api/skills/{name}/execute", post(skill_execute))
            .route("/api/skills/match", post(skills_match))
                                                                        .route("/api/workflow/v2/stream", post(metagpt_workflow_stream))
            .route("/api/tasks", post(task_execute))
            .route("/api/tasks/{id}/status", get(task_status))
            .route("/api/tasks/{id}/cancel", post(task_cancel))
            .route("/api/mcp/servers", get(mcp_servers_list))
            .route("/api/mcp/servers", post(mcp_servers_update))
            .route("/api/mcp/tools", get(mcp_all_tools))
            .route("/api/mcp/servers/{name}", put(mcp_server_update_one).delete(mcp_server_delete))
            .route("/api/mcp/servers/{name}/toggle", patch(mcp_server_toggle))
            .route("/api/mcp/servers/{name}/start", post(mcp_server_start))
            .route("/api/mcp/servers/{name}/stop", post(mcp_server_stop))
            .route("/api/mcp/servers/{name}/restart", post(mcp_server_restart))
            .route("/api/mcp/servers/{name}/tools", get(mcp_tools_list))
            .route("/api/mcp/servers/{name}/resources", get(mcp_resources_list))
            .route("/api/mcp/servers/{name}/resources/{uri}", get(mcp_resource_read))
            .route("/api/mcp/servers/{name}/resources/{uri}/monitor", post(mcp_resource_monitor))
            .route("/api/mcp/servers/{name}/connect", post(mcp_connect_handler))
            .route("/api/mcp/servers/{name}/disconnect", post(mcp_disconnect_handler))
            .route("/api/engines", get(engine_status_handler))
            .route("/api/engines/spawn", post(engine_spawn_handler))
            .route("/api/engines/{conv_id}", delete(engine_kill_handler))
            .route("/api/streams/{conv_id}", get(stream_events_handler))
            .route("/api/research/start", post(research_start_handler))
            .route("/api/research/{id}/stop", post(research_stop_handler))
            .route("/api/research/status/{id}", get(research_status_handler))
            .route("/api/research/{id}/events", get(research_events_handler))
            .route("/api/multiagent/research", post(multiagent_research_handler))
            .route("/api/computer-use/screen-info", get(computer_use_screen_info))
            .route("/api/computer-use/execute", post(computer_use_execute))
            .route("/api/computer-use/screenshot", get(computer_use_screenshot))
            .route("/api/git/status", get(git_status_handler))
            .route("/api/git/log", get(git_log_handler))
            .route("/api/git/diff", get(git_diff_handler))
            .route("/api/git/commit", post(git_commit_handler))
            .route("/api/git/push", post(git_push_handler))
            .route("/api/git/pull", post(git_pull_handler))
            .route("/api/terminals", post(terminal_create))
            .route("/api/terminals", get(terminal_list))
            .route("/api/terminals/{id}/write", post(terminal_write))
            .route("/api/terminals/{id}/resize", post(terminal_resize))
            .route("/api/terminals/{id}", delete(terminal_close))
            .route("/api/terminals/{id}/stream", get(terminal_stream))
            .route("/api/process/spawn", post(process_spawn))
            .route("/api/process/{pid}", delete(process_kill))
            .route("/api/process/list", get(process_list))
            .route("/api/clipboard/read", get(clipboard_read))
            .route("/api/clipboard/write", post(clipboard_write))
            .route("/api/notification/show", post(notification_show))
            .route("/api/logs", get(logs_read))
            .route("/api/logs/clear", post(logs_clear))
            .route("/api/watcher/start", post(watcher_start))
            .route("/api/watcher/watch", post(watcher_watch))
            .route("/api/watcher/unwatch", post(watcher_unwatch))
            .route("/api/update/check", get(update_check))
            .route("/api/update/download", post(update_download))
            .route("/api/worktrees", get(worktree_list))
            .route("/api/worktrees", post(worktree_create))
            .route("/api/worktrees/sync", post(worktree_sync))
            .route("/api/worktrees/{id}", get(worktree_get))
            .route("/api/worktrees/{id}", delete(worktree_remove))
            .route("/api/worktrees/merge", post(worktree_merge))
            .route("/api/agents", get(agent_list))
            .route("/api/agents/{id}", get(agent_get))
            .route("/api/agents/{id}/cancel", post(agent_cancel))
            .route("/api/ide/status", get(ide_status))
            .route("/api/ide/start", post(ide_start))
            .route("/api/ide/stop", post(ide_stop))
            .route("/api/ide/connections", get(ide_connections))
            .route("/api/ide/connections/{id}", delete(ide_disconnect))
            .route("/api/analytics/track", post(analytics_track))
            .route("/api/analytics/daily/{date}", get(analytics_daily))
            .route("/api/analytics/range", get(analytics_range))
            .route("/api/analytics/summary", get(analytics_summary))
            .route("/api/analytics/event-counts", get(analytics_event_counts))
            .route("/api/analytics/recent-events", get(analytics_recent_events))
            .route("/api/memories", get(memories_list).post(memory_handlers_v2::memories_create))
            .route("/api/memories/search", get(memories_search))
            .route("/api/memories/stats", get(memories_stats))
            .route("/api/memories/tags", get(memory_handlers_v2::memories_tags))
            .route("/api/memories/tags/rename", post(memory_handlers_v2::memories_tag_rename))
            .route("/api/memories/tags/merge", post(memory_handlers_v2::memories_tags_merge))
            .route("/api/memories/tags/delete", post(memory_handlers_v2::memories_tag_delete))
            .route("/api/memories/{id}", delete(memories_delete).put(memory_handlers_v2::memories_update))
            .route("/api/memories/backfill", post(memories_backfill))
        .route("/api/memories/vector-search", post(memory_handlers_v2::memories_vector_search))
        .route("/api/memories/vector-stats", get(memory_handlers_v2::memories_vector_stats))
            .route("/api/memories/{id}/associations", get(memory_handlers_v2::memories_associations))
            .route("/api/memories/cluster", post(memory_handlers_v2::memories_cluster))
            .route("/api/memories/compress", post(memory_handlers_v2::memories_compress))
        .route("/api/knowledge", get(memory_handlers_v2::knowledge_list).post(memory_handlers_v2::knowledge_create))
        .route("/api/knowledge/search", post(memory_handlers_v2::knowledge_search))
            .route("/api/swarm/sessions", get(swarm_sessions_list).post(swarm_sessions_create))
            .route("/api/swarm/sessions/{id}", get(swarm_sessions_get).delete(swarm_sessions_delete))
            .route("/api/swarm/sessions/{id}/messages", get(swarm_messages_get).post(swarm_messages_add))
            .route("/api/swarm/sessions/{id}/status", post(swarm_status_update))
            .route("/api/swarm/sessions/{id}/title", post(swarm_session_rename))
            .route("/api/tdai/health", get(tdai_health))
            .route("/api/tdai/config", get(tdai_get_config).post(tdai_set_config))
            .route("/api/tdai/auth/verify", post(tdai_auth_verify))
            .route("/api/tdai/search", post(tdai_search))
            .route("/api/tdai/memory", post(tdai_add_memory))
            .route("/api/tdai/memory/{id}/promote", post(tdai_promote))
            .route("/api/tdai/stats", get(tdai_stats))

            .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
            .layer(cors)
            .with_state(state);

        // Use SO_REUSEADDR to recover from zombie sockets after crash
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", port).parse()?;
        let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)?;
        socket.set_reuse_address(true)?;
        #[cfg(windows)]
        socket.bind(&addr.into())?;
        socket.listen(1024)?;
        let std_listener: std::net::TcpListener = socket.into();
        std_listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(std_listener)?;
        tracing::info!(target: "bridge", "[Bridge] Server running on http://127.0.0.1:{}", port);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn system_status() -> Json<SystemStatus> {
    let platform = std::env::consts::OS.to_string();
    let git_bash_path = find_git_bash();

    Json(SystemStatus {
        platform,
        git_bash: GitBashStatus {
            required: cfg!(target_os = "windows"),
            found: git_bash_path.is_some(),
            path: git_bash_path,
        },
    })
}

/// 返回当前浏览器实时画面（base64 PNG），供前端侧边栏显示模型所见。
async fn browser_view_handler() -> Json<serde_json::Value> {
    match crate::browser_use::capture_png().await {
        Ok(b64) => Json(serde_json::json!({
            "success": true,
            "data": b64,
            "url": crate::browser_use::browser_session().get_url().await.unwrap_or_default(),
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// 返回浏览器会话状态（是否就绪、当前 URL）。
async fn browser_status_handler() -> Json<serde_json::Value> {
    let session = crate::browser_use::browser_session();
    match session.ensure_ready().await {
        Ok(_) => Json(serde_json::json!({
            "ready": true,
            "url": session.get_url().await.unwrap_or_default(),
        })),
        Err(e) => Json(serde_json::json!({ "ready": false, "error": e.to_string() })),
    }
}

/// 前端地址栏导航（同时给模型与用户共用同一个会话画面）。
async fn browser_navigate_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let session = crate::browser_use::browser_session();
    let raw = req.get("url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if raw.is_empty() {
        return Json(serde_json::json!({ "success": false, "error": "url required" }));
    }
    // 特殊动作：后退 / 前进 / 起始页。
    match raw.as_str() {
        "__back__" => {
            let r = session
                .send_cmd("Runtime.evaluate", serde_json::json!({ "expression": "history.back()", "returnByValue": true }))
                .await;
            return match r {
                Ok(_) => Json(serde_json::json!({ "success": true })),
                Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
            };
        }
        "__forward__" => {
            let r = session
                .send_cmd("Runtime.evaluate", serde_json::json!({ "expression": "history.forward()", "returnByValue": true }))
                .await;
            return match r {
                Ok(_) => Json(serde_json::json!({ "success": true })),
                Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
            };
        }
        "home://start" => {
            let r = session.navigate_home().await;
            return match r {
                Ok(_) => Json(serde_json::json!({ "success": true, "url": "home://start" })),
                Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
            };
        }
        _ => {}
    }
    // 无协议时补 https://；纯关键词走必应搜索。
    let looks_like_url = raw.starts_with("http://")
        || raw.starts_with("https://")
        || raw.starts_with("data:")
        || raw.starts_with("file:")
        || (raw.contains('.') && !raw.contains(' '));
    let target = if looks_like_url {
        if raw.starts_with("http") || raw.starts_with("data:") || raw.starts_with("file:") {
            raw.clone()
        } else {
            format!("https://{}", raw)
        }
    } else {
        format!("https://www.bing.com/search?q={}", urlencoding::encode(&raw))
    };
    match session.navigate(&target).await {
        Ok(_) => {
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
            Json(serde_json::json!({ "success": true, "url": target }))
        }
        Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
    }
}

/// 人工面板交互：把用户在画面上的点击/滚轮/键盘转发给真实页面。
async fn browser_interact_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let session = crate::browser_use::browser_session();
    let action = req.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let x = req.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
    let y = req.get("y").and_then(|v| v.as_i64()).unwrap_or(0);
    let dx = req.get("dx").and_then(|v| v.as_i64()).unwrap_or(0);
    let dy = req.get("dy").and_then(|v| v.as_i64()).unwrap_or(0);
    let key = req.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let text = req.get("text").and_then(|v| v.as_str()).unwrap_or("");
    if action.is_empty() {
        return Json(serde_json::json!({ "success": false, "error": "action required" }));
    }
    match session.interact(action, x, y, dx, dy, key, text).await {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
    }
}

/// 结构化快照（供面板/调试）：返回当前页面可交互元素列表。
async fn browser_snapshot_handler() -> Json<serde_json::Value> {
    let session = crate::browser_use::browser_session();
    match session.ensure_ready().await {
        Ok(_) => match session.snapshot().await {
            Ok(elements) => Json(serde_json::json!({ "success": true, "elements": elements })),
            Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
        },
        Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct OpenFolderRequest {
    path: String,
}

#[derive(Serialize)]
struct OpenFolderResponse {
    ok: bool,
    error: Option<String>,
}

async fn open_folder_handler(Json(body): Json<OpenFolderRequest>) -> Json<OpenFolderResponse> {
    let path = body.path.trim().to_string();
    if path.is_empty() {
        return Json(OpenFolderResponse { ok: false, error: Some("path is empty".to_string()) });
    }
    let pb = std::path::PathBuf::from(&path);
    if !pb.exists() {
        return Json(OpenFolderResponse { ok: false, error: Some(format!("path does not exist: {}", path)) });
    }
    let result: Result<(), String> = if cfg!(target_os = "windows") {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    match result {
        Ok(()) => Json(OpenFolderResponse { ok: true, error: None }),
        Err(e) => Json(OpenFolderResponse { ok: false, error: Some(e) }),
    }
}

#[derive(Serialize)]
struct WorkspaceConfig {
    default_dir: String,
}

async fn workspace_config_get() -> Json<WorkspaceConfig> {
    let default_dir = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    Json(WorkspaceConfig { default_dir })
}

#[derive(Deserialize)]
struct WorkspaceConfigUpdate {
    dir: String,
}

async fn workspace_config_set(
    Json(body): Json<WorkspaceConfigUpdate>,
) -> StatusCode {
    let _ = body.dir;
    StatusCode::OK
}

fn find_git_bash() -> Option<String> {
    let candidates: Vec<String> = if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Git\bin\bash.exe".to_string(),
            r"C:\Program Files (x86)\Git\bin\bash.exe".to_string(),
        ]
    } else {
        vec!["/usr/bin/bash".to_string(), "/bin/bash".to_string()]
    };

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.clone());
        }
    }
    None
}

async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let native_engine = state.native_engine.clone();
    let config_manager = state.config_manager.clone();
    let conv_id = req.conversation_id.clone();
    let model = req.model.clone();
    // 从 DB 派生完整历史（含 tool_calls），前端仅传增量 message，避免请求体随历史膨胀
    let mut messages = load_conversation_history(state.db_manager.clone(), &conv_id).await;
    if let Some(msg) = &req.message {
        messages.push(serde_json::json!({ "role": "user", "content": msg }));
    }
    let messages_len = messages.len();

    log_to_file!("[Chat] Received request: conv_id={}, model={}, messages={}", conv_id, model, messages_len);
    std::io::Write::flush(&mut std::io::stdout()).ok();

    // Research mode: route to research pipeline
    if req.research_mode == Some(true) {
        let query = req.message.clone().unwrap_or_default();
        let providers_sync = {
            let cm = config_manager.lock().await;
            if let Some(cm) = cm.as_ref() {
                cm.get_config().providers.iter().map(|p| {
                    crate::native_engine::provider_manager::Provider {
                        id: p.id.clone(), name: p.name.clone(), base_url: p.base_url.clone(),
                        api_key: p.api_key.clone().unwrap_or_default(),
                        api_format: { let d = p.base_url.contains("deepseek"); if p.provider_type=="anthropic" && !d { crate::native_engine::provider_manager::ApiFormat::Anthropic } else { crate::native_engine::provider_manager::ApiFormat::OpenAI } },
                        models: p.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig { id: m.id.clone(), name: m.name.clone(), enabled: m.enabled, max_tokens: m.max_tokens, context_window: None, supports_vision: m.supports_vision, supports_web_search: false, context_size: None }).collect(),
                        enabled: p.enabled, web_search_strategy: p.web_search_strategy.clone(),
                    }
                }).collect::<Vec<_>>()
            } else { Vec::new() }
        };
        let resolved = {
            let mut eg = native_engine.lock().await;
            if let Some(e) = eg.as_mut() { e.sync_providers(providers_sync).await; e.resolve_provider(&model).await } else { None }
        };
        let resolved = match resolved { Some(r) => r, None => {
            let es = async_stream::stream! { yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type":"error","error":format!("No provider for {}",model)}).to_string())); };
            let mut r = Sse::new(es).keep_alive(KeepAlive::default()).into_response(); r.headers_mut().insert(CONTENT_TYPE, "text/event-stream; charset=utf-8".parse().unwrap()); return r;
        }};
        let api_key = resolved.provider.api_key.clone(); let base_url = resolved.provider.base_url.clone();
        let api_fmt = match resolved.provider.api_format { crate::native_engine::provider_manager::ApiFormat::Anthropic => "anthropic", _ => "openai" }.to_string();
        let rid = uuid::Uuid::new_v4().to_string(); let ar = state.active_research.clone();
        let (btx, _) = broadcast::channel::<ResearchEvent>(256); let (mtx, mrx) = tokio::sync::mpsc::unbounded_channel::<ResearchEvent>();
        let btx2 = btx.clone(); let req2 = ResearchRequest { query, api_key, base_url, model: model.clone(), api_format: api_fmt };
        let handle = tokio::spawn(async move {
            let b = btx2.clone(); let mut mrx = mrx;
            let fh = tokio::spawn(async move { while let Some(ev) = mrx.recv().await { let _ = b.send(ev); } });
            let o = ResearchOrchestrator::new(reqwest::Client::new());
            if let Err(e) = o.run_pipeline(req2, mtx).await { tracing::error!(target: "research", "Error: {}", e); }
            let _ = fh.await;
        });
        { ar.lock().await.insert(rid.clone(), ResearchTask { handle, event_tx: btx.clone() }); }
        let mut rx = btx.subscribe(); let cid = conv_id.clone(); let db = state.db_manager.clone();
        let stream = async_stream::stream! {
            let mut report = String::new();
            while let Ok(ev) = rx.recv().await {
                if let Ok(d) = serde_json::to_value(&ev) {
                    let t = d.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if t == "research_report_delta" { if let Some(txt) = d.get("text").and_then(|v| v.as_str()) { report.push_str(txt); } }
                    let done = t == "research_done" || t == "research_error";
                    yield Ok::<Event, Infallible>(Event::default().data(d.to_string()));
                    if done { break; }
                }
            }
            if !report.is_empty() {
                let db = db;
                let cid = cid;
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = db.with_conn(|conn| {
                        let mid = uuid::Uuid::new_v4().to_string();
                        let now = chrono::Utc::now().to_rfc3339();
                        let so = crate::db::message_repo::next_sort_order(conn, &cid);
                        let _ = crate::db::message_repo::insert_message(conn, &mid, &cid, "assistant", &report, None, &now, false, so);
                        let _ = crate::db::conversation_repo::increment_message_count(conn, &cid);
                        Ok::<(), rusqlite::Error>(())
                    });
                }).await;
            }
        };
        let mut resp = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
        resp.headers_mut().insert(CONTENT_TYPE, "text/event-stream; charset=utf-8".parse().unwrap());
        return resp;
    }

    // Sync providers from ConfigManager to NativeEngine before each request
    let providers_to_sync = {
        let cm_guard: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
        if let Some(cm) = cm_guard.as_ref() {
            cm.get_config().providers.iter().map(|p| {
                crate::native_engine::provider_manager::Provider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    base_url: p.base_url.clone(),
                    api_key: p.api_key.clone().unwrap_or_default(),
                    api_format: {
                        // DeepSeek uses OpenAI-compatible API even if user selected wrong format
                        let is_deepseek = p.base_url.contains("deepseek");
                        if p.provider_type == "anthropic" && !is_deepseek {
                            crate::native_engine::provider_manager::ApiFormat::Anthropic
                        } else {
                            crate::native_engine::provider_manager::ApiFormat::OpenAI
                        }
                    },
                    models: p.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        enabled: m.enabled,
                        max_tokens: m.max_tokens, context_window: None,
                        supports_vision: m.supports_vision,
                        supports_web_search: false,
                        context_size: None,
                    }).collect(),
                    enabled: p.enabled,
                    web_search_strategy: p.web_search_strategy.clone(),
                }
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };

    let rx_opt = {
        let mut engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
        if let Some(engine) = engine_guard.as_mut() {
            // Sync latest providers
            engine.sync_providers(providers_to_sync).await;

            // Set permission mode from frontend
            if let Some(pm) = &req.permission_mode {
                let mode = crate::permissions::PermissionMode::from_str(pm);
                engine.set_permission_mode(mode).await;
                log_to_file!("[Chat] Permission mode set to: {}", pm);
            }

            let chat_req = crate::native_engine::engine_core::ChatRequest {
                conversation_id: conv_id.clone(),
                messages: messages.clone(),
                model: if model.is_empty() { "claude-sonnet-4-20250514".to_string() } else { model.clone() },
                system_prompt: req.custom_system_prompt.clone(),
                max_tokens: None,
                workspace_path: None,
                temperature: None,
                top_p: None,
                web_search_enabled: req.web_search_enabled,
                reasoning_effort: req.reasoning_effort.clone(),
                extended_thinking: req.extended_thinking.unwrap_or(false) || model.ends_with("-thinking"),
                enable_streaming: req.enable_streaming.unwrap_or(true),
            };
                    log_to_file!("[Chat] Calling send_message...");
            match engine.send_message(chat_req).await {
                Ok(rx) => Some(rx),
                Err(e) => {
                    tracing::error!(target: "chat", "NativeEngine send_message error: {}", e);
                    None
                }
            }
        } else {
            tracing::error!(target: "chat", "NativeEngine not initialized");
            None
        }
    };

    log_to_file!("[Chat] Creating SSE stream...");
    // 服务端事件历史：任何 SSE 消费者（重连端点）都能从此重放，恢复工具卡片与用量。
    {
        let mut mgr = state.stream_manager.lock().await;
        if !mgr.is_active(&conv_id) {
            mgr.create_stream(&conv_id);
        }
    }
    let sm_for_events = state.stream_manager.clone();
    let conv_id_for_events = conv_id.clone();
    let stream = async_stream::stream! {
        let mut rx = match rx_opt {
            Some(rx) => rx,
            None => {
                yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type": "error", "error": "Failed to start message: NativeEngine not available"}).to_string()));
                return;
            }
        };

        let mut full_text = String::new();

        while let Some(event) = rx.recv().await {
            let event_data = match event {
                crate::native_engine::tool_loop::EngineEvent::MessageStart { model } => {
                    Some(serde_json::json!({
                        "type": "message_start",
                        "model": model,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Text(text) => {
                    full_text.push_str(&text);
                    Some(serde_json::json!({
                        "type": "content_block_delta",
                        "delta": {"type": "text_delta", "text": text},
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Thinking(thinking) => {
                    Some(serde_json::json!({
                        "type": "thinking",
                        "thinking": thinking,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolUseStart { tool_use_id, tool_name, tool_input, text_before } => {
                    log_to_file!("[Chat] Tool use started: {} ({})", tool_name, tool_use_id);
                    Some(serde_json::json!({
                        "type": "tool_use_start",
                        "tool_use_id": tool_use_id,
                        "tool_name": tool_name,
                        "tool_input": tool_input,
                        "textBefore": text_before,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolArgDelta { tool_use_id, delta } => {
                    Some(serde_json::json!({
                        "type": "tool_arg_delta",
                        "tool_use_id": tool_use_id,
                        "delta": delta,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolUseDone { tool_use_id, tool_name, tool_input, output, is_error } => {
                    log_to_file!("[Chat] Tool use completed: {} ({}) is_error={}", tool_name, tool_use_id, is_error);
                    Some(serde_json::json!({
                        "type": "tool_use_done",
                        "tool_use_id": tool_use_id,
                        "tool_name": tool_name,
                        "tool_input": tool_input,
                        "output": output,
                        "content": output,
                        "is_error": is_error,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::MessageDelta { stop_reason } => {
                    Some(serde_json::json!({
                        "type": "message_delta",
                        "delta": {"stop_reason": stop_reason},
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::MessageStop { full_text: _, stop_reason } => {
                    Some(serde_json::json!({
                        "type": "message_stop",
                        "stop_reason": stop_reason,
                        "full_text": full_text.clone(),
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Error(err) => {
                    tracing::error!(target: "chat", "Engine error: {}", err);
                    Some(serde_json::json!({
                        "type": "error",
                        "error": err,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Usage(usage) => {
                    Some(serde_json::json!({
                        "type": "usage",
                        "usage": usage,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::AskUser { question, options } => {
                    let options_json: Vec<serde_json::Value> = options.iter()
                        .map(|o| serde_json::json!({"label": o, "description": ""}))
                        .collect();
                    Some(serde_json::json!({
                        "type": "ask_user",
                        "request_id": "ask_user_request",
                        "tool_use_id": "ask_user_tool",
                        "questions": [{
                            "question": question,
                            "options": options_json
                        }],
                    }))
                }
            };

            if let Some(data) = event_data {
                let is_stop = data.get("type").and_then(|t| t.as_str()) == Some("message_stop")
                    || data.get("type").and_then(|t| t.as_str()) == Some("error");
                // 写入服务端事件历史（供 stream-status / reconnect 重放）。
                {
                    let mut mgr = sm_for_events.lock().await;
                    mgr.broadcast(
                        &conv_id_for_events,
                        crate::streaming::StreamEvent {
                            event_type: data.get("type").and_then(|t| t.as_str()).unwrap_or("event").to_string(),
                            data: data.clone(),
                            timestamp: chrono::Utc::now().timestamp_millis(),
                        },
                    );
                }
                yield Ok::<Event, Infallible>(Event::default().data(data.to_string()));
                if is_stop {
                    break;
                }
            }
        }

        {
            let mut mgr = sm_for_events.lock().await;
            mgr.end_stream(&conv_id_for_events);
        }
        log_to_file!("[Chat] Stream ended for conv_id={}", conv_id);
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "text/event-stream; charset=utf-8".parse().unwrap(),
    );
    response
}

async fn chat_stream_handler(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let stream_manager = state.stream_manager.clone();
    let mut manager: tokio::sync::MutexGuard<'_, StreamManager> = stream_manager.lock().await;

    let receiver = manager.add_listener(&query.conversation_id)
        .ok_or_else(|| StatusCode::NOT_FOUND)?;

    let stream = async_stream::stream! {
        let mut rx = receiver;
        while let Ok(event) = rx.recv().await {
            let event_name = event.event_type;
            let data = serde_json::to_string(&event.data).unwrap_or_default();
            yield Ok::<Event, Infallible>(Event::default()
                .event(&event_name)
                .data(data));
        }
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "text/event-stream; charset=utf-8".parse().unwrap(),
    );
    Ok(response)
}

async fn tools_handler(
    Json(req): Json<ToolRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let cwd = req.cwd.clone().unwrap_or_else(|| ".".to_string());
    let name = req.name.clone();
    let input = req.input.clone();

    let result = tokio::task::spawn_blocking(move || {
        execute_tool(&name, input, &cwd)
    }).await;

    match result {
        Ok(Ok(result)) => Ok(Json(result)),
        _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// 查询会话是否有活跃的流（前端切回会话时判断是否需要重连）。
async fn conversation_stream_status_handler(
    Path(conv_id): Path<String>,
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let mut mgr = state.stream_manager.lock().await;
    mgr.cleanup_done_streams();
    let active = mgr.is_active(&conv_id);
    let event_count = mgr.get_events(&conv_id).map(|e| e.len()).unwrap_or(0);
    Json(serde_json::json!({ "active": active, "eventCount": event_count }))
}

/// 重连会话流：先重放服务端事件历史（含 tool_use_start/usage），再续传实时事件。
/// 流结束时发 [DONE]。前端 reconnectStream 可据此恢复工具卡片、文本与 token 统计。
async fn conversation_reconnect_handler(
    Path(conv_id): Path<String>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, StatusCode> {
    let (rx, history) = {
        let mut mgr = state.stream_manager.lock().await;
        mgr.add_listener_with_replay(&conv_id)
            .ok_or(StatusCode::NOT_FOUND)?
    };
    let stream = async_stream::stream! {
        // 1) 重放历史事件
        for ev in history {
            let payload = serde_json::to_string(&ev.data).unwrap_or_default();
            yield Ok::<Event, Infallible>(Event::default().data(payload));
        }
        // 2) 续传实时事件
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if event.event_type == "stream_done" {
                        yield Ok(Event::default().data("[DONE]"));
                        break;
                    }
                    let payload = serde_json::to_string(&event.data).unwrap_or_default();
                    yield Ok(Event::default().data(payload));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => {
                    // 所有发送端已关闭（引擎流结束）：通知前端收尾。
                    yield Ok(Event::default().data("[DONE]"));
                    break;
                }
            }
        }
    };
    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "text/event-stream; charset=utf-8".parse().unwrap(),
    );
    Ok(response)
}

async fn tool_execute_handler(
    State(_state): State<AppState>,
    Json(req): Json<ToolRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let cwd = req.cwd.clone().unwrap_or_else(|| ".".to_string());
    let name = req.name.clone();
    let input = req.input.clone();

    let result = tokio::task::spawn_blocking(move || {
        execute_tool(&name, input, &cwd)
    }).await;

    match result {
        Ok(Ok(result)) => Ok(Json(result)),
        _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn tools_list_handler() -> Json<Vec<ToolDefinition>> {
    Json(get_tool_definitions())
}

async fn conversations_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::conversation_repo::list_conversations(conn))
    }).await;
    match result {
        Ok(Ok(Ok(convs))) => Json(serde_json::json!({ "conversations": convs })),
        _ => Json(serde_json::json!({ "conversations": [] })),
    }
}

async fn conversations_create(State(state): State<AppState>) -> Json<serde_json::Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let db = state.db_manager.clone();
    let id_clone = id.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let now = chrono::Utc::now().to_rfc3339();
            crate::db::conversation_repo::insert_conversation(conn, &id_clone, None, None, None, None, None, false, false, false, &now, &now, 0)
        })
    }).await;
    Json(serde_json::json!({ "id": id }))
}

/// 从 DB 派生对话历史（含 tool_calls 聚合），模型上下文组装与 conversation_get 共用。
async fn load_conversation_history(db: std::sync::Arc<crate::db::DbManager>, conv_id: &str) -> Vec<serde_json::Value> {
    let id_clone = conv_id.to_string();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| -> anyhow::Result<(Vec<crate::db::message_repo::MessageRow>, Vec<crate::db::message_repo::ToolCallRow>)> {
            let messages = crate::db::message_repo::get_messages_by_conversation(conn, &id_clone)?;
            let tool_calls = crate::db::message_repo::list_tool_calls_for_conversation(conn, &id_clone).unwrap_or_default();
            Ok((messages, tool_calls))
        })
    }).await;
    match result {
        Ok(Ok(Ok((messages, tool_calls)))) => {
            // 按消息聚合工具调用，前端重载会话后仍能渲染工具卡片
            let mut by_msg: std::collections::HashMap<String, Vec<&crate::db::message_repo::ToolCallRow>> = std::collections::HashMap::new();
            for tc in &tool_calls {
                by_msg.entry(tc.message_id.clone()).or_default().push(tc);
            }
            messages.into_iter().map(|m| {
                let mut v = serde_json::to_value(&m).unwrap_or(serde_json::json!({}));
                let mid = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if let Some(list) = by_msg.get(&mid) {
                    v["tool_calls"] = serde_json::json!(list.iter().map(|tc| serde_json::json!({
                        "id": tc.id,
                        "name": tc.name,
                        "input": serde_json::from_str::<serde_json::Value>(&tc.input).unwrap_or(serde_json::json!({})),
                        "output": tc.output,
                        "is_error": tc.is_error,
                    })).collect::<Vec<_>>());
                }
                v
            }).collect()
        }
        _ => Vec::new(),
    }
}

async fn conversation_get(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let messages = load_conversation_history(state.db_manager.clone(), &id).await;
    Json(serde_json::json!({ "id": id, "messages": messages }))
}

async fn conversation_update(Path(id): Path<String>, State(state): State<AppState>, Json(messages): Json<Vec<serde_json::Value>>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn: &rusqlite::Connection| {
            let tx = conn.unchecked_transaction()?;
            crate::db::message_repo::delete_messages_from(&tx, &id, 0)?;
            for (idx, msg) in messages.iter().enumerate() {
                let msg_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or(&uuid::Uuid::new_v4().to_string()).to_string();
                let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                let content = match msg.get("content") {
                    Some(v) if v.is_string() => v.as_str().unwrap_or("").to_string(),
                    Some(v) => serde_json::to_string(v).unwrap_or_default(),
                    None => String::new(),
                };
                let now = chrono::Utc::now().to_rfc3339();
                crate::db::message_repo::insert_message(&tx, &msg_id, &id, role, &content, None, &now, false, idx as i64)?;
            }
            crate::db::conversation_repo::increment_message_count(&tx, &id)?;
            tx.commit()?;
            Ok::<(), anyhow::Error>(())
        })
    }).await;
    Json(serde_json::json!({ "ok": true }))
}




#[derive(Deserialize)]
struct CompactRequest {
    instruction: Option<String>,
}

async fn compact_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<CompactRequest>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();

    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| -> anyhow::Result<serde_json::Value> {
            let messages = crate::db::message_repo::get_messages_by_conversation(conn, &id)?;

            if messages.len() < 4 {
                return Ok(serde_json::json!({
                    "success": false,
                    "error": "Not enough messages to compact (minimum 4)"
                }));
            }

            // Smart compaction strategy:
            // 1. Keep last 5 messages intact (recent context is most valuable)
            // 2. Keep all system messages (they contain instructions)
            // 3. Summarize older user/assistant exchanges
            let keep_count = 5.min(messages.len());
            let split_point = messages.len().saturating_sub(keep_count);
            let old_messages = &messages[..split_point];
            let new_messages = &messages[split_point..];

            // Build structured summary with key information extraction
            let mut topics: Vec<String> = Vec::new();
            let mut decisions: Vec<String> = Vec::new();
            let mut code_changes: Vec<String> = Vec::new();
            let mut file_refs: Vec<String> = Vec::new();

            for msg in old_messages.iter() {
                if msg.role == "system" {
                    continue; // Skip system messages in summary
                }

                let content = &msg.content;

                // Extract file references
                for word in content.split_whitespace() {
                    if (word.ends_with(".rs") || word.ends_with(".ts") || word.ends_with(".tsx")
                        || word.ends_with(".py") || word.ends_with(".js") || word.ends_with(".json"))
                        && (word.contains('/') || word.contains('\\'))
                        && !file_refs.contains(&word.to_string())
                    {
                        file_refs.push(word.to_string());
                    }
                }

                // Extract decision patterns
                let lower = content.to_lowercase();
                if lower.contains("decided") || lower.contains("chose") || lower.contains("going with")
                    || lower.contains("let's use") || lower.contains("we'll") {
                    let preview: String = content.chars().take(150).collect();
                    decisions.push(format!("[{}]: {}", msg.role, preview));
                }

                // Extract code change descriptions
                if lower.contains("added") || lower.contains("fixed") || lower.contains("refactored")
                    || lower.contains("changed") || lower.contains("updated") || lower.contains("removed") {
                    let preview: String = content.chars().take(150).collect();
                    code_changes.push(format!("[{}]: {}", msg.role, preview));
                }

                // Extract topic from user messages
                if msg.role == "user" {
                    let preview: String = content.chars().take(100).collect();
                    topics.push(preview);
                }
            }

            // Build the summary
            let mut summary_parts = Vec::new();

            if !topics.is_empty() {
                summary_parts.push(format!("**Topics discussed:**\n{}", topics.join("\n")));
            }
            if !decisions.is_empty() {
                let recent_decisions: Vec<&str> = decisions.iter().take(5).map(|s| s.as_str()).collect();
                summary_parts.push(format!("**Key decisions:**\n{}", recent_decisions.join("\n")));
            }
            if !code_changes.is_empty() {
                let recent_changes: Vec<&str> = code_changes.iter().take(5).map(|s| s.as_str()).collect();
                summary_parts.push(format!("**Code changes:**\n{}", recent_changes.join("\n")));
            }
            if !file_refs.is_empty() {
                let recent_files: Vec<&str> = file_refs.iter().take(10).map(|s| s.as_str()).collect();
                summary_parts.push(format!("**Files referenced:**\n{}", recent_files.join(", ")));
            }

            let summary = if summary_parts.is_empty() {
                format!("**Previous conversation summary:** {} messages compacted.", old_messages.len())
            } else {
                format!("**Previous conversation summary ({} messages):**\n\n{}",
                    old_messages.len(), summary_parts.join("\n\n"))
            };

            // Custom instruction if provided
            let summary = if let Some(ref instruction) = req.instruction {
                if !instruction.is_empty() {
                    format!("{}\n\n**User instruction:** {}", summary, instruction)
                } else {
                    summary
                }
            } else {
                summary
            };

            let old_tokens: usize = old_messages.iter().map(|m| m.content.len()).sum();
            let new_tokens = summary.len();
            let tokens_saved = old_tokens.saturating_sub(new_tokens);

            // Delete old messages and insert summary
            let split_order = old_messages.last().map(|m| m.sort_order + 1).unwrap_or(0);
            crate::db::message_repo::delete_messages_before(conn, &id, split_order)?;

            let summary_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            crate::db::message_repo::insert_message(
                conn, &summary_id, &id, "system", &summary,
                None, &now, true, 0
            )?;

            tracing::info!(target: "compact", "Compacted conversation {}: {} messages -> {} + summary (saved ~{} chars)",
                id, messages.len(), keep_count, tokens_saved);

            Ok(serde_json::json!({
                "success": true,
                "summary": summary,
                "tokensSaved": tokens_saved,
                "messagesCompacted": old_messages.len(),
                "messagesRemaining": new_messages.len() + 1,
                "filesReferenced": file_refs.len(),
                "topicsExtracted": topics.len()
            }))
        })
    }).await;

    match result {
        Ok(Ok(Ok(data))) => Json(data),
        Ok(Ok(Err(e))) => Json(serde_json::json!({"success": false, "error": e.to_string()})),
        _ => Json(serde_json::json!({"success": false, "error": "Internal error"})),
    }
}
async fn context_size_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    tracing::debug!(target: "context", conversation_id = %id, "Context size query");
    let db = state.db_manager.clone();

    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let conv = crate::db::conversation_repo::get_conversation(conn, &id)?;
            let messages = crate::db::message_repo::get_messages_by_conversation(conn, &id)?;

            let total_chars: usize = messages.iter()
                .map(|m| m.content.len())
                .sum();
            let estimated_tokens = (total_chars as f64 * 1.5) as u32;

            let model_id = conv.as_ref()
                .and_then(|c| c.model.as_deref())
                .unwrap_or("default");
            let context_limit = crate::native_engine::provider_manager::get_default_context_size(model_id);

            let usage_percent = if context_limit > 0 {
                (estimated_tokens as f64 / context_limit as f64 * 100.0).round() as u32
            } else {
                0
            };

            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({
                "tokens": estimated_tokens,
                "limit": context_limit,
                "model": model_id,
                "message_count": messages.len(),
                "usage_percent": usage_percent
            }))
        })
    }).await;

    match result {
        Ok(Ok(Ok(data))) => {
            tracing::debug!(target: "context", "Context size result: {}", data);
            Json(data)
        },
        Err(e) => {
            tracing::error!(target: "context", "spawn_blocking error: {}", e);
            Json(serde_json::json!({"tokens": 0, "limit": 200000, "error": format!("spawn error: {}", e)}))
        },
        Ok(Err(e)) => {
            tracing::error!(target: "context", "with_conn error: {}", e);
            Json(serde_json::json!({"tokens": 0, "limit": 200000, "error": format!("conn error: {}", e)}))
        },
        Ok(Ok(Err(e))) => {
            tracing::error!(target: "context", "query error: {}", e);
            Json(serde_json::json!({"tokens": 0, "limit": 200000, "error": format!("query error: {}", e)}))
        },
    }
}
#[derive(Deserialize)]
struct ConversationPatch {
    title: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    workspace_path: Option<String>,
    project_id: Option<String>,
    research_mode: Option<bool>,
    pinned: Option<bool>,
    archived: Option<bool>,
}

async fn conversation_patch(Path(id): Path<String>, State(state): State<AppState>, Json(patch): Json<ConversationPatch>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let now = chrono::Utc::now().to_rfc3339();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::db::conversation_repo::update_conversation(
                conn, &id,
                patch.title.as_deref(),
                patch.model.as_deref(),
                patch.provider.as_deref(),
                patch.workspace_path.as_deref(),
                patch.project_id.as_deref(),
                patch.research_mode,
                patch.pinned,
                patch.archived,
                Some(&now),
                None,
            )
        })
    }).await;
    match result {
        Ok(Ok(Ok(()))) => Json(serde_json::json!({ "ok": true })),
        _ => Json(serde_json::json!({ "ok": false, "error": "Failed to update conversation" })),
    }
}

async fn conversation_delete(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::db::message_repo::delete_messages_from(conn, &id, 0).ok();
            crate::db::conversation_repo::delete_conversation(conn, &id)
        })
    }).await;
    Json(serde_json::json!({ "ok": true }))
}

async fn conversation_messages(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::message_repo::get_messages_by_conversation(conn, &id))
    }).await;
    match result {
        Ok(Ok(Ok(messages))) => Json(serde_json::json!({ "messages": messages })),
        _ => Json(serde_json::json!({ "messages": [] })),
    }
}

async fn conversation_message_delete(
    Path((id, mid)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let msg = crate::db::message_repo::get_message(conn, &mid)?;
            if let Some(m) = msg {
                crate::db::message_repo::delete_messages_from(conn, &id, m.sort_order)?;
            }
            crate::db::message_repo::get_messages_by_conversation(conn, &id)
        })
    }).await;
    match result {
        Ok(Ok(Ok(messages))) => Ok(Json(serde_json::json!({ "success": true, "messages": messages }))),
        Ok(Ok(Err(e))) => { tracing::error!(target: "messagedelete", "Failed: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Ok(Err(e)) => { tracing::error!(target: "chat", "MessageDelete DB lock error: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn conversation_messages_tail_delete(
    Path((id, count)): Path<(String, i64)>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::db::message_repo::delete_messages_tail(conn, &id, count)?;
            crate::db::message_repo::get_messages_by_conversation(conn, &id)
        })
    }).await;
    match result {
        Ok(Ok(Ok(messages))) => Ok(Json(serde_json::json!({ "success": true, "messages": messages }))),
        Ok(Ok(Err(e))) => { tracing::error!(target: "messagestaildelete", "Failed: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Ok(Err(e)) => { tracing::error!(target: "chat", "MessagesTailDelete DB lock error: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
struct BranchRequest {
    from_message_id: Option<String>,
}

async fn conversation_branch_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<BranchRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let new_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let source = crate::db::conversation_repo::get_conversation(conn, &id)?;
            let title = source.as_ref().and_then(|c| c.title.as_deref()).unwrap_or("Branched conversation");
            let model = source.as_ref().and_then(|c| c.model.as_deref());
            crate::db::conversation_repo::insert_conversation(
                conn, &new_id, Some(&format!("{} (branch)", title)), model, None, None, None, false, false, false, &now, &now, 0,
            )?;
            let mut messages = crate::db::message_repo::get_messages_by_conversation(conn, &id)?;
            if let Some(mid) = req.from_message_id.as_deref() {
                if let Some(m) = crate::db::message_repo::get_message(conn, mid)? {
                    messages.retain(|msg| msg.sort_order < m.sort_order);
                }
            }
            for msg in &messages {
                let msg_id = uuid::Uuid::new_v4().to_string();
                crate::db::message_repo::insert_message(
                    conn, &msg_id, &new_id, &msg.role, &msg.content, msg.thinking.as_deref(), &msg.created_at, msg.is_compact_boundary, msg.sort_order,
                )?;
            }
            Ok::<String, anyhow::Error>(new_id)
        })
    }).await;
    match result {
        Ok(Ok(Ok(new_id))) => Ok(Json(serde_json::json!({ "success": true, "new_conversation_id": new_id }))),
        Ok(Ok(Err(e))) => { tracing::error!(target: "branch", "Failed: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Ok(Err(e)) => { tracing::error!(target: "chat", "Branch DB lock error: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
struct AnswerRequest {
    request_id: String,
    tool_use_id: Option<String>,
    answers: Option<serde_json::Value>,
}

async fn conversation_answer_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<AnswerRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let engine_pool = state.engine_pool.clone();
    let mut pool: tokio::sync::MutexGuard<'_, EnginePool> = engine_pool.lock().await;

    let original_input = pool.get_ask_user_pending(&id).unwrap_or(serde_json::json!({}));

    let answers = req.answers.unwrap_or(serde_json::json!({}));

    let mut updated_input = original_input;
    if let Some(obj) = updated_input.as_object_mut() {
        obj.insert("answers".to_string(), answers.clone());
    } else {
        updated_input = serde_json::json!({ "answers": answers.clone() });
    }

    let tool_use_id = req.tool_use_id.unwrap_or_default();

    match pool.send_control_response(&id, &req.request_id, &tool_use_id, updated_input).await {
        Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(_) => {
            drop(pool);
            let native_engine = state.native_engine.clone();
            let engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
            if let Some(engine) = engine_guard.as_ref() {
                // Extract the actual answer value from the answers object.
                // Frontend sends: {"question text": "Yes"} — we need just "Yes".
                let answer_value = if let Some(obj) = answers.as_object() {
                    obj.values().next().and_then(|v| v.as_str()).unwrap_or("Yes").to_string()
                } else if let Some(s) = answers.as_str() {
                    s.to_string()
                } else {
                    "Yes".to_string()
                };
                match engine.resume_with_answer(&id, answer_value).await {
                    Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
                    Err(e) => {
                        tracing::error!(target: "chat", "AskUser engine answer failed: {}", e);
                        Err(StatusCode::NOT_FOUND)
                    }
                }
            } else {
                tracing::error!(target: "askuser", "No engine available for conversation {}", id);
                Err(StatusCode::NOT_FOUND)
            }
        }
    }
}

#[derive(Deserialize)]
struct WarmRequest {
    permission_mode: Option<String>,
}

async fn conversation_warm_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<WarmRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(ref pm_str) = req.permission_mode {
        let perm_mode = match pm_str.as_str() {
            "ask_permissions" => PermissionMode::AskPermissions,
            "accept_edits" => PermissionMode::AcceptEdits,
            "plan_mode" => PermissionMode::PlanMode,
            "bypass_permissions" => PermissionMode::BypassPermissions,
            _ => PermissionMode::AskPermissions,
        };
        if let Some(engine) = state.native_engine.lock().await.as_ref() {
            engine.set_permission_mode(perm_mode).await;
            tracing::info!(target: "bridge", "Warm: permission_mode set to {:?} for conversation {}", perm_mode, id);
        }
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct PermissionRequest {
    request_id: String,
    tool_use_id: Option<String>,
    behavior: Option<String>,
}

async fn conversation_permission_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<PermissionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let engine_pool = state.engine_pool.clone();
    let mut pool: tokio::sync::MutexGuard<'_, EnginePool> = engine_pool.lock().await;

    let pending = pool.get_tool_permission_pending(&id);
    let tool_use_id = req.tool_use_id
        .or_else(|| pending.as_ref().and_then(|p| p.get("tool_use_id").and_then(|t| t.as_str()).map(String::from)))
        .unwrap_or_default();

    let behavior = req.behavior.unwrap_or_else(|| "allow".to_string());

    let updated_input = pending.and_then(|p| p.get("input").cloned());

    match pool.send_permission_response(&id, &req.request_id, &tool_use_id, &behavior, updated_input).await {
        Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(_) => {
            drop(pool);
            let native_engine = state.native_engine.clone();
            let engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
            if let Some(engine) = engine_guard.as_ref() {
                let answer = if behavior == "allow" { "allow".to_string() } else { "deny".to_string() };
                match engine.resume_with_answer(&id, answer).await {
                    Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
                    Err(e) => {
                        tracing::error!(target: "permission", "Native engine answer failed: {}", e);
                        Err(StatusCode::NOT_FOUND)
                    }
                }
            } else {
                tracing::error!(target: "permission", "No pool engine and no native engine for conversation {}", id);
                Err(StatusCode::NOT_FOUND)
            }
        }
    }
}

async fn projects_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let projects = crate::db::project_repo::list_projects(conn).unwrap_or_default();
            serde_json::json!(projects)
        })
    }).await;
    match result {
        Ok(Ok(data)) => Json(serde_json::json!({ "projects": data })),
        _ => Json(serde_json::json!({ "projects": [] })),
    }
}

async fn projects_create(State(state): State<AppState>, Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
    let description = body.get("description").and_then(|v| v.as_str()).map(String::from);
    let workspace_path = body.get("workspace_path").and_then(|v| v.as_str()).map(String::from);
    let now = chrono::Utc::now().to_rfc3339();
    let db = state.db_manager.clone();
    let id_clone = id.clone();
    let name_clone = name.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::db::project_repo::insert_project(conn, &id_clone, &name_clone, description.as_deref(), None, workspace_path.as_deref(), false, &now, &now)
        })
    }).await;
    Json(serde_json::json!({ "id": id, "name": name }))
}

async fn projects_update(Path(id): Path<String>, State(state): State<AppState>, Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            if let Ok(Some(mut project)) = crate::db::project_repo::get_project(conn, &id) {
                if let Some(name) = body.get("name").and_then(|v| v.as_str()) { project.name = name.to_string(); }
                if let Some(desc) = body.get("description") { project.description = desc.as_str().map(String::from); }
                if let Some(instr) = body.get("instructions") { project.instructions = instr.as_str().map(String::from); }
                if let Some(wp) = body.get("workspace_path") { project.workspace_path = wp.as_str().map(String::from); }
                let _ = crate::db::project_repo::update_project(conn, &id, Some(&project.name), project.description.as_deref(), project.instructions.as_deref(), project.workspace_path.as_deref(), Some(project.is_archived));
            }
            Ok::<(), anyhow::Error>(())
        })
    }).await;
    Json(serde_json::json!({ "ok": result.is_ok() }))
}

async fn projects_delete(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::project_repo::delete_project(conn, &id))
    }).await;
    Json(serde_json::json!({ "ok": true }))
}

async fn projects_get(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::db::project_repo::get_project(conn, &id)
                .map_err(|e| anyhow::anyhow!(e))
        })
    }).await;
    match result {
        Ok(Ok(Ok(Some(project)))) => Json(serde_json::json!({
            "id": project.id,
            "name": project.name,
            "description": project.description,
            "instructions": project.instructions,
            "workspace_path": project.workspace_path,
            "is_archived": project.is_archived,
            "created_at": project.created_at,
            "updated_at": project.updated_at,
        })),
        _ => Json(serde_json::json!({"error": "not found"})),
    }
}

async fn project_conversations_list(Path(project_id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let pid = project_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::db::conversation_repo::list_conversations_by_project(conn, &pid)
        })
    }).await;
    match result {
        Ok(Ok(Ok(convs))) => {
            let items: Vec<_> = convs.iter().map(|c| serde_json::json!({
                "id": c.id,
                "title": c.title,
                "model": c.model,
                "project_id": c.project_id,
                "created_at": c.created_at,
                "updated_at": c.updated_at,
            })).collect();
            Json(serde_json::json!({"conversations": items}))
        }
        _ => Json(serde_json::json!({"conversations": []})),
    }
}
async fn project_conversation_create(Path(project_id): Path<String>, State(state): State<AppState>, Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let conv_id = uuid::Uuid::new_v4().to_string();
    let title = body.get("title").and_then(|v| v.as_str()).map(String::from);
    let model = body.get("model").and_then(|v| v.as_str()).map(String::from);
    let workspace_path = body.get("workspace_path").and_then(|v| v.as_str()).map(String::from);
    let now = chrono::Utc::now().to_rfc3339();
    let db = state.db_manager.clone();
    let conv_id_clone = conv_id.clone();
    let project_id_clone = project_id.clone();
    let workspace_path_clone = workspace_path.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::db::conversation_repo::insert_conversation(conn, &conv_id_clone, title.as_deref(), model.as_deref(), None, workspace_path_clone.as_deref(), Some(&project_id_clone), false, false, false, &now, &now, 0)
        })
    }).await;
    Json(serde_json::json!({ "id": conv_id, "project_id": project_id, "workspace_path": workspace_path }))
}

async fn project_file_delete(Path((project_id, file_id)): Path<(String, String)>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::project_repo::delete_project_file(conn, &file_id))
    }).await;
    Json(serde_json::json!({"ok": true}))
}

async fn project_file_upload(Path(project_id): Path<String>, State(state): State<AppState>, body: axum::extract::Multipart) -> Json<serde_json::Value> {
    Json(serde_json::json!({"error": "file upload not yet implemented"}))
}
static UPLOAD_DIR: std::sync::LazyLock<std::sync::Mutex<Option<PathBuf>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

fn get_upload_dir() -> PathBuf {
    let guard = UPLOAD_DIR.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(dir) = guard.as_ref() {
        return dir.clone();
    }
    drop(guard);
    let default_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("claude-desktop")
        .join("uploads");
    let mut guard = UPLOAD_DIR.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(default_dir.clone());
    default_dir
}

async fn upload_handler(mut multipart: Multipart) -> Result<Json<serde_json::Value>, StatusCode> {
    let upload_dir = get_upload_dir();
    std::fs::create_dir_all(&upload_dir).map_err(|e| {
        tracing::error!(target: "upload", "Failed to create upload dir: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!(target: "upload", "Multipart error: {}", e);
        StatusCode::BAD_REQUEST
    })? {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let file_name = field.file_name()
                .unwrap_or("unnamed")
                .to_string();
            let content_type = field.content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;

            let file_size = data.len();
            let file_id = uuid::Uuid::new_v4().to_string();
            let ext = std::path::Path::new(&file_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let file_type = if content_type.starts_with("image/") {
                "image"
            } else if content_type == "application/pdf" || ext == "pdf" {
                "document"
            } else if content_type.starts_with("text/") || matches!(ext, "txt" | "md" | "csv" | "json" | "xml" | "yaml" | "yml") {
                "text"
            } else {
                "document"
            };

            let dest_path = upload_dir.join(&file_id);
            tokio::fs::write(&dest_path, &data).await.map_err(|e| {
                tracing::error!(target: "upload", "Failed to save file: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            tracing::info!(target: "bridge", "[Upload] File saved: {} ({} bytes, type: {})", file_name, file_size, file_type);

            return Ok(Json(serde_json::json!({
                "fileId": file_id,
                "fileName": file_name,
                "fileType": file_type,
                "mimeType": content_type,
                "size": file_size,
            })));
        }
    }

    Err(StatusCode::BAD_REQUEST)
}

use axum::body::Body;
use axum::response::Response;
use axum::http::header;

async fn upload_get_handler(Path(id): Path<String>) -> Result<Response<Body>, StatusCode> {
    let upload_dir = get_upload_dir();
    let file_path = upload_dir.join(&id);

    if !file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let data = tokio::fs::read(&file_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    
    let mime_type = match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, data.len())
        .header(header::CACHE_CONTROL, "public, max-age=31536000")
        .body(Body::from(data))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(response)
}

async fn upload_delete_handler(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let upload_dir = get_upload_dir();
    let file_path = upload_dir.join(&id);

    if file_path.exists() {
        tokio::fs::remove_file(&file_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        tracing::info!(target: "bridge", "[Upload] File deleted: {}", id);
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn providers_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.config_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        let config = m.get_config();
        let providers: Vec<serde_json::Value> = config.providers.iter().map(|p| {
            serde_json::json!({
                "id": p.id,
                "name": p.name,
                "apiKey": p.api_key,
                "baseUrl": p.base_url,
                "format": p.provider_type,
                "models": p.models.iter().map(|m| serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "enabled": m.enabled,
                })).collect::<Vec<_>>(),
                "enabled": p.enabled,
                "supportsWebSearch": p.supports_web_search,
                "webSearchStrategy": p.web_search_strategy,
                "webSearchTestedAt": p.web_search_tested_at,
                "webSearchTestReason": p.web_search_test_reason,
            })
        }).collect();
        return Json(serde_json::json!({ "providers": providers }));
    }
    Json(serde_json::json!({ "providers": [] }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProviderRequest {
    name: String,
    base_url: Option<String>,
    api_key: Option<String>,
    format: Option<String>,
    models: Option<Vec<serde_json::Value>>,
    enabled: Option<bool>,
    supports_web_search: Option<bool>,
}

async fn providers_create(State(state): State<AppState>, Json(req): Json<CreateProviderRequest>) -> Json<serde_json::Value> {
    let config_manager = state.config_manager.clone();
    let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        let id = uuid::Uuid::new_v4().to_string();
        let provider_type = req.format.unwrap_or_else(|| "openai".to_string());
        let models: Vec<crate::config::ModelConfig> = req.models.unwrap_or_default().iter().map(|m| {
            crate::config::ModelConfig {
                id: m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                name: m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                enabled: m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                max_tokens: None,
                supports_vision: false,
                supports_tools: true,
                supports_streaming: true,
                context_window: None,
                cost_per_1k_input: None,
                cost_per_1k_output: None,
            }
        }).collect();
        let new_provider = crate::config::ProviderConfig {
            id: id.clone(),
            name: req.name.clone(),
            provider_type,
            api_key: if req.api_key.as_ref().map_or(false, |k| k.is_empty()) { None } else { req.api_key.clone() },
            base_url: req.base_url.clone().unwrap_or_default(),
            models,
            enabled: req.enabled.unwrap_or(true),
            is_default: false,
            settings: std::collections::HashMap::new(),
            supports_web_search: req.supports_web_search.unwrap_or(false),
            web_search_strategy: None,
            web_search_tested_at: None,
            web_search_test_reason: None,
            api_format: None,
        };
        match m.add_provider(new_provider) {
            Ok(()) => {
                let created_id = id.clone();
                drop(manager);
                let state_clone = state.clone();
                sync_provider_manager_owned(state_clone).await;
                let config_manager2 = state.config_manager.clone();
                let manager2: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager2.lock().await;
                if let Some(m2) = manager2.as_ref() {
                    if let Some(created) = m2.get_provider(&created_id) {
                        return Json(serde_json::json!({
                            "id": created.id,
                            "name": created.name,
                            "apiKey": created.api_key,
                            "baseUrl": created.base_url,
                            "format": created.provider_type,
                            "models": created.models.iter().map(|m| serde_json::json!({"id": m.id, "name": m.name, "enabled": m.enabled})).collect::<Vec<_>>(),
                            "enabled": created.enabled,
                            "supportsWebSearch": created.supports_web_search,
                            "webSearchStrategy": created.web_search_strategy,
                        }));
                    }
                }
                Json(serde_json::json!({ "error": "Provider created but not found" }))
            }
            Err(e) => Json(serde_json::json!({ "error": format!("{}", e) }))
        }
    } else {
        Json(serde_json::json!({ "error": "Config manager not initialized" }))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct UpdateProviderRequest {
    name: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    format: Option<String>,
    models: Option<Vec<serde_json::Value>>,
    enabled: Option<bool>,
    supports_web_search: Option<bool>,
    web_search_strategy: Option<Option<String>>,
    web_search_tested_at: Option<Option<u64>>,
    web_search_test_reason: Option<Option<String>>,
}

async fn providers_patch(Path(id): Path<String>, State(state): State<AppState>, Json(updates): Json<HashMap<String, serde_json::Value>>) -> Json<serde_json::Value> {
    let config_manager = state.config_manager.clone();
    let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        let config = m.get_config();
        let idx = config.providers.iter().position(|p| p.id == id);
        if let Some(idx) = idx {
            m.update_config(|c| {
                if let Some(name) = updates.get("name").and_then(|v| v.as_str()) {
                    c.providers[idx].name = name.to_string();
                }
                if let Some(base_url) = updates.get("baseUrl").and_then(|v| v.as_str()) {
                    c.providers[idx].base_url = base_url.to_string();
                }
                if let Some(api_key) = updates.get("apiKey").and_then(|v| v.as_str()) {
                    c.providers[idx].api_key = Some(api_key.to_string());
                }
                if let Some(format) = updates.get("format").and_then(|v| v.as_str()) {
                    c.providers[idx].provider_type = format.to_string();
                }
                if let Some(enabled) = updates.get("enabled").and_then(|v| v.as_bool()) {
                    c.providers[idx].enabled = enabled;
                }
                if let Some(sws) = updates.get("supportsWebSearch") {
                    c.providers[idx].supports_web_search = sws.as_bool().unwrap_or(false);
                }
                if let Some(strategy) = updates.get("webSearchStrategy") {
                    c.providers[idx].web_search_strategy = if strategy.is_null() { None } else { strategy.as_str().map(|s| s.to_string()) };
                }
                if let Some(tested_at) = updates.get("webSearchTestedAt") {
                    c.providers[idx].web_search_tested_at = if tested_at.is_null() { None } else { tested_at.as_u64() };
                }
                if let Some(reason) = updates.get("webSearchTestReason") {
                    c.providers[idx].web_search_test_reason = if reason.is_null() { None } else { reason.as_str().map(|s| s.to_string()) };
                }
                if let Some(models_val) = updates.get("models").and_then(|v| v.as_array()) {
                    c.providers[idx].models = models_val.iter().map(|m| {
                        crate::config::ModelConfig {
                            id: m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            name: m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            enabled: m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                            max_tokens: None,
                            supports_vision: false,
                            supports_tools: true,
                            supports_streaming: true,
                            context_window: None,
                            cost_per_1k_input: None,
                            cost_per_1k_output: None,
                        }
                    }).collect();
                }
            }).ok();
            
            drop(manager);
            sync_provider_manager_owned(state.clone()).await;
            
            let config_manager2 = state.config_manager.clone();
            let manager2: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager2.lock().await;
            if let Some(m2) = manager2.as_ref() {
                if let Some(p) = m2.get_provider(&id) {
                    return Json(serde_json::json!({
                        "id": p.id,
                        "name": p.name,
                        "apiKey": p.api_key,
                        "baseUrl": p.base_url,
                        "format": p.provider_type,
                        "models": p.models.iter().map(|m| serde_json::json!({"id": m.id, "name": m.name, "enabled": m.enabled})).collect::<Vec<_>>(),
                        "enabled": p.enabled,
                        "supportsWebSearch": p.supports_web_search,
                        "webSearchStrategy": p.web_search_strategy,
                        "webSearchTestedAt": p.web_search_tested_at,
                        "webSearchTestReason": p.web_search_test_reason,
                    }));
                }
            }
            Json(serde_json::json!({ "error": "Provider not found after update" }))
        } else {
            Json(serde_json::json!({ "error": format!("Provider '{}' not found", id) }))
        }
    } else {
        Json(serde_json::json!({ "error": "Config manager not initialized" }))
    }
}

async fn providers_delete(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.config_manager.clone();
    let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        match m.remove_provider(&id) {
            Ok(()) => {
                drop(manager);
                sync_provider_manager_owned(state.clone()).await;
                Json(serde_json::json!({ "ok": true }))
            }
            Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
        }
    } else {
        Json(serde_json::json!({ "error": "Config manager not initialized" }))
    }
}

async fn providers_models_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.config_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        let config = m.get_config();
        let models: Vec<serde_json::Value> = config.providers.iter()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                p.models.iter()
                    .filter(|m| m.enabled)
                    .map(|m| serde_json::json!({
                        "id": m.id,
                        "name": m.name,
                        "providerId": p.id,
                        "providerName": p.name,
                    }))
            })
            .collect();
        return Json(serde_json::json!({ "models": models }));
    }
    Json(serde_json::json!({ "models": [] }))
}

async fn providers_test_websearch(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.config_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        if let Some(provider) = m.get_provider(&id) {
            let api_key = provider.api_key.clone().unwrap_or_default();
            let base_url = provider.base_url.clone();
            let provider_type = provider.provider_type.clone();
            drop(manager);

            let result = test_web_search_capability(&id, &api_key, &base_url, &provider_type).await;

            let config_manager = state.config_manager.clone();
            let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
            if let Some(m) = manager.as_mut() {
                if let Some(provider) = m.get_provider_mut(&id) {
                    provider.supports_web_search = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                    provider.web_search_strategy = result.get("strategy").and_then(|v| v.as_str()).map(String::from);
                    provider.web_search_tested_at = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
                    provider.web_search_test_reason = result.get("reason").and_then(|v| v.as_str()).map(String::from);
                    let _ = m.save();
                }
            }
            return Json(result);
        }
    }
    Json(serde_json::json!({ "ok": false, "reason": "Provider not found" }))
}

#[allow(dead_code)]
async fn sync_provider_manager(state: &AppState) {
    let config_manager = state.config_manager.clone();
    let native_engine = state.native_engine.clone();
    
    let providers_to_sync = {
        let cm_guard = config_manager.lock().await;
        if let Some(cm) = cm_guard.as_ref() {
            cm.get_config().providers.iter().map(|p| {
                crate::native_engine::provider_manager::Provider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    base_url: p.base_url.clone(),
                    api_key: p.api_key.clone().unwrap_or_default(),
                    api_format: {
                        let is_deepseek = p.base_url.contains("deepseek");
                        if p.provider_type == "anthropic" && !is_deepseek {
                            crate::native_engine::provider_manager::ApiFormat::Anthropic
                        } else {
                            crate::native_engine::provider_manager::ApiFormat::OpenAI
                        }
                    },
                    models: p.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        enabled: m.enabled,
                        max_tokens: m.max_tokens, context_window: None,
                        supports_vision: m.supports_vision,
                        supports_web_search: p.supports_web_search,
                        context_size: None,
                    }).collect(),
                    enabled: p.enabled,
                    web_search_strategy: p.web_search_strategy.clone(),
                }
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };
    
    let mut engine_guard = native_engine.lock().await;
    if let Some(engine) = engine_guard.as_mut() {
        engine.sync_providers(providers_to_sync).await;
        tracing::info!(target: "bridge", "[Bridge] ProviderManager synced with ConfigManager providers");
    }
}

async fn sync_provider_manager_owned(state: AppState) {
    let config_manager = state.config_manager.clone();
    let native_engine = state.native_engine.clone();
    
    let providers_to_sync = {
        let cm_guard = config_manager.lock().await;
        if let Some(cm) = cm_guard.as_ref() {
            cm.get_config().providers.iter().map(|p| {
                crate::native_engine::provider_manager::Provider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    base_url: p.base_url.clone(),
                    api_key: p.api_key.clone().unwrap_or_default(),
                    api_format: {
                        let is_deepseek = p.base_url.contains("deepseek");
                        if p.provider_type == "anthropic" && !is_deepseek {
                            crate::native_engine::provider_manager::ApiFormat::Anthropic
                        } else {
                            crate::native_engine::provider_manager::ApiFormat::OpenAI
                        }
                    },
                    models: p.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        enabled: m.enabled,
                        max_tokens: m.max_tokens, context_window: None,
                        supports_vision: m.supports_vision,
                        supports_web_search: p.supports_web_search,
                        context_size: None,
                    }).collect(),
                    enabled: p.enabled,
                    web_search_strategy: p.web_search_strategy.clone(),
                }
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };
    
    let mut engine_guard = native_engine.lock().await;
    if let Some(engine) = engine_guard.as_mut() {
        engine.sync_providers(providers_to_sync).await;
        tracing::info!(target: "bridge", "[Bridge] ProviderManager synced with ConfigManager providers");
    }
}

async fn test_web_search_capability(_id: &str, _api_key: &str, _base_url: &str, _provider_type: &str) -> serde_json::Value {
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build() {
        Ok(c) => c, Err(e) => return serde_json::json!({ "ok": false, "reason": format!("Client error: {}", e) }),
    };
    match client.get("https://api.duckduckgo.com/?q=test&format=json&no_html=1").send().await {
        Ok(resp) if resp.status().is_success() => serde_json::json!({ "ok": true, "strategy": "duckduckgo" }),
        Ok(resp) => serde_json::json!({ "ok": false, "reason": format!("HTTP {}", resp.status()) }),
        Err(e) => serde_json::json!({ "ok": false, "reason": format!("Unreachable: {}", e) }),
    }
}

async fn config_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.config_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        return Json(serde_json::to_value(m.get_config()).unwrap_or_default());
    }
    Json(serde_json::json!({}))
}

async fn config_update(State(state): State<AppState>, Json(config): Json<AppConfig>) -> Json<serde_json::Value> {
    let config_manager = state.config_manager.clone();
    let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        let _ = m.update_config(|c| *c = config);
    }
    Json(serde_json::json!({ "ok": true }))
}

async fn skills_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_manager = state.skill_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.load_skills().await {
        Ok(skills) => Json(serde_json::json!({ "skills": skills })),
        Err(e) => Json(serde_json::json!({ "skills": [], "error": format!("{}", e) })),
    }
}

async fn skills_create(State(state): State<AppState>, Json(skill): Json<Skill>) -> Json<serde_json::Value> {
    let skill_manager = state.skill_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.create_skill(&skill.name, &skill.description, &skill.content.unwrap_or_default()) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn skill_get(Path(name): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_manager = state.skill_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.get_skill_by_id(&name).await {
        Ok(Some(skill)) => Json(serde_json::to_value(skill).unwrap_or_default()),
        Ok(None) => Json(serde_json::json!({ "error": "Skill not found" })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn skill_update(Path(name): Path<String>, State(state): State<AppState>, Json(updates): Json<HashMap<String, serde_json::Value>>) -> Json<serde_json::Value> {
    let skill_manager = state.skill_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.update_skill(&name, updates) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn skill_delete(Path(name): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_manager = state.skill_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.delete_skill(&name) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct SkillEnableRequest {
    pub enabled: bool,
}

async fn skill_enable(Path(name): Path<String>, State(state): State<AppState>, Json(_req): Json<SkillEnableRequest>) -> Json<serde_json::Value> {
    let skill_manager = state.skill_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.toggle_skill(&name).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct SkillExecuteRequest {
    pub input: String,
    pub conversation_id: Option<String>,
    pub workspace_path: Option<String>,
    pub variables: Option<serde_json::Map<String, serde_json::Value>>,
}

async fn skill_execute(
    Path(name): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<SkillExecuteRequest>,
) -> Json<serde_json::Value> {
    let skill_manager = state.skill_manager.clone();
    let mcp_server_manager = state.mcp_server_manager.clone();
    
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    
    let input = req.input.clone();
    
    let mut context = SkillExecutionContext::default();
    context.current_input = input.clone();
    context.conversation_id = req.conversation_id.unwrap_or_default();
    context.workspace_path = req.workspace_path;
    
    if let Some(vars) = req.variables {
        for (key, value) in vars {
            if let Some(s) = value.as_str() {
                context.variables.insert(key, s.to_string());
            }
        }
    }
    
    context = context.with_mcp_manager(mcp_server_manager.clone());
    
    let mcp_tools = mcp_server_manager.get_all_tools().await;
    context.available_mcp_tools = mcp_tools;
    
    match manager.execute_skill(&name, &input, Some(context)).await {
        Ok(result) => Json(serde_json::json!({ "success": true, "result": result })),
        Err(e) => Json(serde_json::json!({ "success": false, "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct SkillMatchRequest {
    pub input: String,
}

async fn skills_match(State(state): State<AppState>, Json(req): Json<SkillMatchRequest>) -> Json<serde_json::Value> {
    let skill_manager = state.skill_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.execute_skill("match", &req.input, None).await {
        Ok(result) => Json(serde_json::json!({ "matched": true, "result": result })),
        Err(_) => Json(serde_json::json!({ "matched": false })),
    }
}

#[derive(Deserialize)]
pub struct TaskExecuteRequest {
    pub task_id: String,
    pub prompt: String,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub context: Option<Vec<serde_json::Value>>,
}

async fn task_execute(
    State(state): State<AppState>,
    Json(req): Json<TaskExecuteRequest>,
) -> Result<Json<TaskResult>, StatusCode> {
    let task_executor = state.task_executor.clone();
    let executor: tokio::sync::MutexGuard<'_, Option<TaskExecutor>> = task_executor.lock().await;
    if let Some(e) = executor.as_ref() {
        let task_request = TaskRequest {
            task_id: req.task_id,
            prompt: req.prompt,
            model: req.model,
            max_tokens: req.max_tokens,
            context: req.context,
            tools: None,
        };

        match e.execute_task(task_request).await {
            Ok(result) => Ok(Json(result)),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn task_status(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let task_executor = state.task_executor.clone();
    let executor: tokio::sync::MutexGuard<'_, Option<TaskExecutor>> = task_executor.lock().await;
    if let Some(e) = executor.as_ref() {
        if let Some(status) = e.get_task_status(&id).await {
            return Json(serde_json::json!({ "status": format!("{:?}", status) }));
        }
    }
    Json(serde_json::json!({ "status": "not_found" }))
}

async fn task_cancel(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let task_executor = state.task_executor.clone();
    let executor: tokio::sync::MutexGuard<'_, Option<TaskExecutor>> = task_executor.lock().await;
    if let Some(e) = executor.as_ref() {
        let cancelled = e.cancel_task(&id).await;
        return Json(serde_json::json!({ "cancelled": cancelled }));
    }
    Json(serde_json::json!({ "cancelled": false }))
}

async fn mcp_servers_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let mcp_server_manager = state.mcp_server_manager.clone();
    let servers: Vec<crate::mcp::McpServerStatus> = mcp_server_manager.list_servers().await;
    let servers_json: Vec<serde_json::Value> = servers
        .iter()
        .map(|s| serde_json::json!({
            "id": s.id,
            "name": s.name,
            "command": s.command,
            "args": s.args,
            "env": s.env,
            "enabled": s.enabled,
            "running": s.running,
            "pid": s.pid,
            "tools_count": s.tools_count,
            "resources_count": s.resources_count,
            "error": s.error,
            "transport_type": s.transport_type
        }))
        .collect();

    Json(serde_json::json!({ "servers": servers_json }))
}

async fn mcp_servers_update(State(state): State<AppState>, Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    // 兼容单对象与数组两种提交格式（McpManagementPanel 提交单对象，旧调用方提交数组）；
    // 补齐 id/args/env/enabled 缺省值后再解析，避免前端字段不全时静默丢弃
    let arr = match body {
        serde_json::Value::Array(a) => a,
        v @ serde_json::Value::Object(_) => vec![v],
        _ => vec![],
    };
    let items: Vec<McpServerConfig> = arr.into_iter().filter_map(|mut item| {
        let obj = item.as_object_mut()?;
        let name = obj.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
        if name.is_empty() { return None; }
        let id_empty = obj.get("id").and_then(|i| i.as_str()).unwrap_or("").is_empty();
        if id_empty {
            obj.insert("id".into(), serde_json::Value::String(name.to_lowercase().replace(' ', "-")));
        }
        obj.entry("args".to_string()).or_insert(serde_json::Value::Array(vec![]));
        obj.entry("env".to_string()).or_insert(serde_json::Value::Object(Default::default()));
        obj.entry("enabled".to_string()).or_insert(serde_json::Value::Bool(true));
        serde_json::from_value(item).ok()
    }).collect();

    let mcp_server_manager = state.mcp_server_manager.clone();
    let mut added = 0usize;
    for server in items {
        match mcp_server_manager.add_server(server).await {
            Ok(()) => added += 1,
            Err(e) => tracing::error!(target: "bridge", "Failed to add MCP server: {}", e),
        }
    }

    Json(serde_json::json!({ "ok": true, "added": added }))
}

/// 统一的 {ok} / (500, {error}) 响应封装：供增删改/启停处理器复用
type McpOpResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

fn mcp_op_err(action: &str, name: &str, e: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!(target: "bridge", "MCP {} '{}' failed: {}", action, name, e);
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
}

async fn mcp_all_tools(State(state): State<AppState>) -> Json<serde_json::Value> {
    let tools: Vec<crate::mcp::McpTool> = state.mcp_server_manager.get_all_tools().await;
    let tools_json: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| serde_json::json!({
            "name": t.name,
            "description": t.description,
            "input_schema": t.input_schema,
            "server_name": t.server_name
        }))
        .collect();
    Json(serde_json::json!({ "tools": tools_json }))
}

async fn mcp_server_update_one(
    Path(name): Path<String>,
    State(state): State<AppState>,
    Json(mut config): Json<McpServerConfig>,
) -> McpOpResult {
    config.id = name.clone();
    state.mcp_server_manager.update_server(&name, config).await
        .map(|_| Json(serde_json::json!({ "ok": true })))
        .map_err(|e| mcp_op_err("update", &name, e))
}

async fn mcp_server_delete(Path(name): Path<String>, State(state): State<AppState>) -> McpOpResult {
    state.mcp_server_manager.remove_server(&name).await
        .map(|_| Json(serde_json::json!({ "ok": true })))
        .map_err(|e| mcp_op_err("remove", &name, e))
}

#[derive(Deserialize)]
struct McpToggleRequest {
    enabled: bool,
}

async fn mcp_server_toggle(
    Path(name): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<McpToggleRequest>,
) -> McpOpResult {
    state.mcp_server_manager.set_server_enabled(&name, req.enabled).await
        .map(|_| Json(serde_json::json!({ "ok": true })))
        .map_err(|e| mcp_op_err("toggle", &name, e))
}

/// 启动/停止/重启处理器
async fn mcp_server_start(Path(name): Path<String>, State(state): State<AppState>) -> McpOpResult {
    state.mcp_server_manager.start_server(&name).await
        .map(|_| Json(serde_json::json!({ "ok": true })))
        .map_err(|e| mcp_op_err("start", &name, e))
}

async fn mcp_server_stop(Path(name): Path<String>, State(state): State<AppState>) -> McpOpResult {
    state.mcp_server_manager.stop_server(&name).await
        .map(|_| Json(serde_json::json!({ "ok": true })))
        .map_err(|e| mcp_op_err("stop", &name, e))
}

async fn mcp_server_restart(Path(name): Path<String>, State(state): State<AppState>) -> McpOpResult {
    state.mcp_server_manager.restart_server(&name).await
        .map(|_| Json(serde_json::json!({ "ok": true })))
        .map_err(|e| mcp_op_err("restart", &name, e))
}

async fn mcp_tools_list(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.mcp_server_manager.clone();
    let tools: Vec<crate::mcp::McpTool> = mcp_server_manager.get_all_tools().await;
    
    let tools_json: Vec<serde_json::Value> = tools
        .iter()
        .filter(|t| t.server_name == name)
        .map(|t| serde_json::json!({
            "name": t.name,
            "description": t.description,
            "input_schema": t.input_schema
        }))
        .collect();

    Ok(Json(serde_json::json!({ "tools": tools_json })))
}

async fn mcp_resources_list(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.mcp_server_manager.clone();
    let resources: Vec<crate::mcp::McpResource> = mcp_server_manager.get_server_resources(&name).await;

    let resources_json: Vec<serde_json::Value> = resources
        .iter()
        .map(|r| serde_json::json!({
            "uri": r.uri,
            "name": r.name,
            "mime_type": r.mime_type
        }))
        .collect();

    Ok(Json(serde_json::json!({ "resources": resources_json })))
}

async fn mcp_resource_read(Path((name, uri)): Path<(String, String)>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.mcp_server_manager.clone();
    
    match mcp_server_manager.read_resource(&name, &uri, None).await {
        Ok(content) => Ok(Json(serde_json::json!({
            "uri": content.uri,
            "content": content.content,
            "content_type": content.content_type,
            "metadata": content.metadata
        }))),
        Err(e) => {
            tracing::error!(target: "bridge", "Failed to read resource: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn mcp_resource_monitor(Path((name, uri)): Path<(String, String)>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.mcp_server_manager.clone();
    
    match mcp_server_manager.monitor_resource(&name, &uri, true).await {
        Ok(enabled) => Ok(Json(serde_json::json!({
            "uri": uri,
            "enabled": enabled
        }))),
        Err(e) => {
            tracing::error!(target: "bridge", "Failed to monitor resource: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn mcp_connect_handler(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.mcp_server_manager.clone();
    
    match mcp_server_manager.start_server(&name).await {
        Ok(_) => {
            if let Some(status) = mcp_server_manager.get_server(&name).await {
                Ok(Json(serde_json::json!({
                    "ok": true,
                    "name": status.name,
                    "status": if status.running { "running" } else { "ready" },
                    "tools_count": status.tools_count,
                    "resources_count": status.resources_count
                })))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        },
        Err(e) => {
            tracing::error!(target: "bridge", "Failed to connect MCP server: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn mcp_disconnect_handler(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.mcp_server_manager.clone();
    
    match mcp_server_manager.stop_server(&name).await {
        Ok(_) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(e) => {
            tracing::error!(target: "bridge", "Failed to disconnect MCP server: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn engine_status_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pool = state.engine_pool.clone();
    let pool_guard: tokio::sync::MutexGuard<'_, EnginePool> = pool.lock().await;
    let engines: Vec<serde_json::Value> = pool_guard.list_engines()
        .iter()
        .map(|e| serde_json::json!({
            "conv_id": e.conv_id,
            "pid": e.pid,
            "model": e.model,
            "session_id": e.session_id,
            "state": format!("{:?}", e.state),
            "workspace": e.workspace.to_string_lossy()
        }))
        .collect();

    Json(serde_json::json!({
        "engines": engines,
        "workspace": pool_guard.get_workspace().to_string_lossy()
    }))
}

#[derive(Deserialize)]
pub struct SpawnRequest {
    pub conv_id: String,
    pub model: String,
    pub cwd: Option<String>,
}

async fn engine_spawn_handler(State(state): State<AppState>, Json(req): Json<SpawnRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let pool = state.engine_pool.clone();
    let mut pool_guard: tokio::sync::MutexGuard<'_, EnginePool> = pool.lock().await;
    match pool_guard.spawn_engine(&req.conv_id, &req.model, req.cwd).await {
        Ok(handle) => Ok(Json(serde_json::json!({
            "ok": true,
            "conv_id": handle.conv_id,
            "session_id": handle.session_id,
            "pid": handle.pid
        }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn engine_kill_handler(Path(conv_id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let pool = state.engine_pool.clone();
    let mut pool_guard: tokio::sync::MutexGuard<'_, EnginePool> = pool.lock().await;
    pool_guard.remove_engine(&conv_id).await;
    Json(serde_json::json!({ "ok": true }))
}

async fn stream_events_handler(Path(conv_id): Path<String>, State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let stream_manager = state.stream_manager.clone();
    let mut manager: tokio::sync::MutexGuard<'_, StreamManager> = stream_manager.lock().await;

    let receiver = manager.add_listener(&conv_id)
        .ok_or_else(|| StatusCode::NOT_FOUND)?;

    let stream = async_stream::stream! {
        let mut rx = receiver;
        while let Ok(event) = rx.recv().await {
            let event_name = event.event_type;
            let data = serde_json::to_string(&event.data).unwrap_or_default();
            yield Ok::<Event, Infallible>(Event::default()
                .event(&event_name)
                .data(data));
        }
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "text/event-stream; charset=utf-8".parse().unwrap(),
    );
    Ok(response)
}

async fn research_start_handler(State(state): State<AppState>, Json(req): Json<ChatRequest>) -> Json<serde_json::Value> {
    let research_id = uuid::Uuid::new_v4().to_string();
    let native_engine = state.native_engine.clone();
    let config_manager = state.config_manager.clone();
    let active_research = state.active_research.clone();

    let model = if req.model.is_empty() { "claude-sonnet-4-20250514".to_string() } else { req.model.clone() };
    let query = req.message.clone().unwrap_or_default();

    let providers_to_sync = {
        let cm_guard: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
        if let Some(cm) = cm_guard.as_ref() {
            cm.get_config().providers.iter().map(|p| {
                crate::native_engine::provider_manager::Provider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    base_url: p.base_url.clone(),
                    api_key: p.api_key.clone().unwrap_or_default(),
                    api_format: {
                        let is_deepseek = p.base_url.contains("deepseek");
                        if p.provider_type == "anthropic" && !is_deepseek {
                            crate::native_engine::provider_manager::ApiFormat::Anthropic
                        } else {
                            crate::native_engine::provider_manager::ApiFormat::OpenAI
                        }
                    },
                    models: p.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        enabled: m.enabled,
                        max_tokens: m.max_tokens, context_window: None,
                        supports_vision: m.supports_vision,
                        supports_web_search: false,
                        context_size: None,
                    }).collect(),
                    enabled: p.enabled,
                    web_search_strategy: p.web_search_strategy.clone(),
                }
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };

    let resolved = {
        let mut engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
        if let Some(engine) = engine_guard.as_mut() {
            engine.sync_providers(providers_to_sync).await;
            engine.resolve_provider(&model).await
        } else {
            None
        }
    };

    let resolved = match resolved {
        Some(r) => r,
        None => return Json(serde_json::json!({ "ok": false, "error": format!("No provider found for model: {}", model) })),
    };

    let api_key = resolved.provider.api_key.clone();
    let base_url = resolved.provider.base_url.clone();

    let (bcast_tx, _) = broadcast::channel::<ResearchEvent>(256);
    let (mpsc_tx, mut mpsc_rx) = tokio::sync::mpsc::unbounded_channel::<ResearchEvent>();

    let bcast_tx_clone = bcast_tx.clone();
    let research_request = ResearchRequest { query: query.clone(), api_key, base_url, model, api_format: match resolved.provider.api_format { crate::native_engine::provider_manager::ApiFormat::Anthropic => "anthropic".to_string(), _ => "openai".to_string() } };

    let handle = tokio::spawn(async move {
        let bcast = bcast_tx_clone.clone();
        let forward_handle = tokio::spawn(async move {
            while let Some(event) = mpsc_rx.recv().await {
                let _ = bcast.send(event);
            }
        });

        let orchestrator = ResearchOrchestrator::new(reqwest::Client::new());
        if let Err(e) = orchestrator.run_pipeline(research_request, mpsc_tx).await {
            tracing::info!(target: "research", "Pipeline error: {}", e);
        }

        let _ = forward_handle.await;
    });

    {
        let mut research: tokio::sync::MutexGuard<'_, HashMap<String, ResearchTask>> = active_research.lock().await;
        research.insert(research_id.clone(), ResearchTask {
            handle,
            event_tx: bcast_tx,
        });
    }

    Json(serde_json::json!({ "ok": true, "research_id": research_id }))
}

async fn research_stop_handler(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let active_research = state.active_research.clone();
    let mut research: tokio::sync::MutexGuard<'_, HashMap<String, ResearchTask>> = active_research.lock().await;
    if let Some(task) = research.remove(&id) {
        task.handle.abort();
        Json(serde_json::json!({ "ok": true }))
    } else {
        Json(serde_json::json!({ "ok": false, "error": "Research task not found" }))
    }
}

async fn research_status_handler(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let active_research = state.active_research.clone();
    let research: tokio::sync::MutexGuard<'_, HashMap<String, ResearchTask>> = active_research.lock().await;
    if let Some(task) = research.get(&id) {
        if task.handle.is_finished() {
            Json(serde_json::json!({ "status": "Completed" }))
        } else {
            Json(serde_json::json!({ "status": "Running" }))
        }
    } else {
        Json(serde_json::json!({ "status": "NotFound" }))
    }
}

async fn research_events_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    let active_research = state.active_research.clone();
    let research: tokio::sync::MutexGuard<'_, HashMap<String, ResearchTask>> = active_research.lock().await;
    let task = research.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    let mut rx = task.event_tx.subscribe();
    drop(research);

    let stream = async_stream::stream! {
        while let Ok(event) = rx.recv().await {
            let data = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<Event, Infallible>(Event::default()
                .event("research")
                .data(data));
        }
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "text/event-stream; charset=utf-8".parse().unwrap(),
    );
    Ok(response)
}

#[derive(Deserialize)]
struct MultiAgentResearchRequest {
    query: String,
    model: Option<String>,
}

async fn multiagent_research_handler(
    State(state): State<AppState>,
    Json(req): Json<MultiAgentResearchRequest>,
) -> Json<serde_json::Value> {
    let native_engine = state.native_engine.clone();
    let config_manager = state.config_manager.clone();

    let model = req.model.unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

    let providers_to_sync = {
        let cm_guard: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
        if let Some(cm) = cm_guard.as_ref() {
            cm.get_config().providers.iter().map(|p| {
                crate::native_engine::provider_manager::Provider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    base_url: p.base_url.clone(),
                    api_key: p.api_key.clone().unwrap_or_default(),
                    api_format: {
                        let is_deepseek = p.base_url.contains("deepseek");
                        if p.provider_type == "anthropic" && !is_deepseek {
                            crate::native_engine::provider_manager::ApiFormat::Anthropic
                        } else {
                            crate::native_engine::provider_manager::ApiFormat::OpenAI
                        }
                    },
                    models: p.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        enabled: m.enabled,
                        max_tokens: m.max_tokens, context_window: None,
                        supports_vision: m.supports_vision,
                        supports_web_search: false,
                        context_size: None,
                    }).collect(),
                    enabled: p.enabled,
                    web_search_strategy: p.web_search_strategy.clone(),
                }
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };

    let resolved = {
        let mut engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
        if let Some(engine) = engine_guard.as_mut() {
            engine.sync_providers(providers_to_sync).await;
            engine.resolve_provider(&model).await
        } else {
            None
        }
    };

    let resolved = match resolved {
        Some(r) => r,
        None => return Json(serde_json::json!({ "ok": false, "error": format!("No provider found for model: {}", model) })),
    };

    let orchestrator = PipelineOrchestrator::new(OrchestratorConfig::default());
    match orchestrator.execute_research(req.query, &resolved).await {
        Ok(result) => Json(serde_json::json!({ "ok": true, "result": result })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct GitRequest {
    pub cwd: Option<String>,
    pub message: Option<String>,
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub file: Option<String>,
    pub force: Option<bool>,
}

async fn computer_use_screen_info() -> Json<serde_json::Value> {
    let manager = crate::computer_use::ComputerUseManager::new(crate::computer_use::ComputerUseConfig::default());
    let info = manager.get_screen_info();
    Json(serde_json::json!({
        "width": info.width,
        "height": info.height,
        "scaleFactor": info.scale_factor,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComputerUseRequest {
    action_type: String,
    coordinate: Option<[i32; 2]>,
    button: Option<String>,
    key: Option<String>,
    text: Option<String>,
    scroll_y: Option<i32>,
    scroll_x: Option<i32>,
    duration_ms: Option<u64>,
}

async fn computer_use_execute(Json(req): Json<ComputerUseRequest>) -> Json<serde_json::Value> {
    let manager = crate::computer_use::ComputerUseManager::new(crate::computer_use::ComputerUseConfig::default());
    let action = crate::computer_use::ComputerAction {
        action_type: match req.action_type.as_str() {
            "mouse_move" => crate::computer_use::ComputerActionType::MouseMove,
            "mouse_click" => crate::computer_use::ComputerActionType::MouseClick,
            "mouse_down" => crate::computer_use::ComputerActionType::MouseDown,
            "mouse_up" => crate::computer_use::ComputerActionType::MouseUp,
            "mouse_scroll" => crate::computer_use::ComputerActionType::MouseScroll,
            "key_press" => crate::computer_use::ComputerActionType::KeyPress,
            "key_down" => crate::computer_use::ComputerActionType::KeyDown,
            "key_up" => crate::computer_use::ComputerActionType::KeyUp,
            "type_text" => crate::computer_use::ComputerActionType::TypeText,
            "screenshot" => crate::computer_use::ComputerActionType::Screenshot,
            "wait" => crate::computer_use::ComputerActionType::Wait,
            _ => crate::computer_use::ComputerActionType::Wait,
        },
        coordinate: req.coordinate.map(|c| crate::computer_use::ScreenCoordinate { x: c[0], y: c[1] }),
        button: req.button.map(|b| match b.as_str() {
            "right" => crate::computer_use::MouseButton::Right,
            "middle" => crate::computer_use::MouseButton::Middle,
            _ => crate::computer_use::MouseButton::Left,
        }),
        key: req.key,
        text: req.text,
        scroll_y: req.scroll_y,
        scroll_x: req.scroll_x,
        duration_ms: req.duration_ms,
    };
    match manager.execute_action(action).await {
        Ok(result) => Json(serde_json::json!({
            "ok": result.success,
            "screenshot": result.screenshot,
            "error": result.error,
            "durationMs": result.duration_ms,
        })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": format!("{}", e) })),
    }
}

async fn computer_use_screenshot() -> Json<serde_json::Value> {
    let manager = crate::computer_use::ComputerUseManager::new(crate::computer_use::ComputerUseConfig::default());
    let action = crate::computer_use::ComputerAction {
        action_type: crate::computer_use::ComputerActionType::Screenshot,
        coordinate: None,
        button: None,
        key: None,
        text: None,
        scroll_y: None,
        scroll_x: None,
        duration_ms: None,
    };
    match manager.execute_action(action).await {
        Ok(result) => Json(serde_json::json!({ "ok": result.success, "screenshot": result.screenshot, "error": result.error })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": format!("{}", e) })),
    }
}

async fn git_status_handler(State(_state): State<AppState>, Query(query): Query<GitRequest>) -> Json<serde_json::Value> {
    let git = GitIntegration::with_cwd(query.cwd);
    match git.get_status() {
        Ok(status) => Json(serde_json::json!({ "status": status })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn git_log_handler(State(_state): State<AppState>, Query(query): Query<GitRequest>) -> Json<serde_json::Value> {
    let git = GitIntegration::with_cwd(query.cwd);
    match git.get_commits(Some(10), None) {
        Ok(commits) => Json(serde_json::json!({ "commits": commits })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn git_diff_handler(State(_state): State<AppState>, Query(query): Query<GitRequest>) -> Json<serde_json::Value> {
    let git = GitIntegration::with_cwd(query.cwd);
    match git.get_file_diff(query.file.as_deref()) {
        Ok(diff) => Json(serde_json::json!({ "diff": diff })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn git_commit_handler(State(_state): State<AppState>, Json(req): Json<GitRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let git = GitIntegration::with_cwd(req.cwd);
    let message = req.message.ok_or_else(|| StatusCode::BAD_REQUEST)?;

    match git.commit(&message) {
        Ok(_) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn git_push_handler(State(_state): State<AppState>, Json(req): Json<GitRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let git = GitIntegration::with_cwd(req.cwd);
    match git.push(req.remote.as_deref(), req.branch.as_deref(), req.force.unwrap_or(false)) {
        Ok(output) => Ok(Json(serde_json::json!({ "ok": true, "output": output }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn git_pull_handler(State(_state): State<AppState>, Json(req): Json<GitRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let git = GitIntegration::with_cwd(req.cwd);
    match git.pull(req.remote.as_deref(), req.branch.as_deref()) {
        Ok(output) => Ok(Json(serde_json::json!({ "ok": true, "output": output }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
pub struct TerminalCreateRequest {
    pub cwd: Option<String>,
    pub shell: Option<String>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
}

async fn terminal_create(State(state): State<AppState>, Json(req): Json<TerminalCreateRequest>) -> Json<serde_json::Value> {
    let terminal_manager = state.terminal_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    match manager.create_session(req.cwd, req.shell, req.cols, req.rows).await {
        Ok(session) => Json(serde_json::json!({ "terminal_id": session.id, "session": session })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct TerminalWriteRequest {
    pub data: String,
}

async fn terminal_write(State(state): State<AppState>, Path(id): Path<String>, Json(req): Json<TerminalWriteRequest>) -> Json<serde_json::Value> {
    let terminal_manager = state.terminal_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    match manager.write_input(&id, &req.data).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct TerminalResizeRequest {
    pub cols: u16,
    pub rows: u16,
}

async fn terminal_resize(State(state): State<AppState>, Path(id): Path<String>, Json(req): Json<TerminalResizeRequest>) -> Json<serde_json::Value> {
    let terminal_manager = state.terminal_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    match manager.resize(&id, req.cols, req.rows).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn terminal_close(State(state): State<AppState>, Path(id): Path<String>) -> Json<serde_json::Value> {
    let terminal_manager = state.terminal_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    match manager.close_session(&id).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn terminal_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let terminal_manager = state.terminal_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    let sessions = manager.list_sessions().await;
    Json(serde_json::to_value(sessions).unwrap_or_default())
}

/// SSE endpoint: streams PTY output to the frontend.
/// Event data format: {"type":"data","data":"..."} or {"type":"exit","code":N}
async fn terminal_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    let terminal_manager = state.terminal_manager.clone();
    let manager = terminal_manager.lock().await;
    let Some((mut output_rx, mut exit_rx, _session)) = manager.get_stream(&id).await else {
        let es = async_stream::stream! {
            yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type":"error","error":"session not found"}).to_string()));
        };
        let mut r = Sse::new(es).keep_alive(KeepAlive::default()).into_response();
        r.headers_mut().insert(CONTENT_TYPE, "text/event-stream; charset=utf-8".parse().unwrap());
        return r;
    };
    drop(manager);

    // If the session already exited, report it immediately
    let initial_exit = *exit_rx.borrow();
    let stream = async_stream::stream! {
        if let Some(code) = initial_exit {
            yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type":"exit","code":code}).to_string()));
            yield Ok::<Event, Infallible>(Event::default().data("[CLOSED]".to_string()));
            return;
        }
        loop {
            tokio::select! {
                res = output_rx.recv() => {
                    match res {
                        Ok(data) => {
                            yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type":"data","data":data}).to_string()));
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            // Channel closed: session gone, treat as exit
                            yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type":"exit","code":null}).to_string()));
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            continue;
                        }
                    }
                }
                res = exit_rx.changed() => {
                    if res.is_ok() {
                        let code = *exit_rx.borrow();
                        yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type":"exit","code":code}).to_string()));
                        break;
                    } else {
                        break;
                    }
                }
            }
        }
        yield Ok::<Event, Infallible>(Event::default().data("[CLOSED]".to_string()));
    };

    let mut resp = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    resp.headers_mut().insert(CONTENT_TYPE, "text/event-stream; charset=utf-8".parse().unwrap());
    resp
}

#[derive(Deserialize)]
pub struct ProcessSpawnRequest {
    pub command: String,
    pub cwd: Option<String>,
    pub env_vars: Option<std::collections::HashMap<String, String>>,
}

async fn process_spawn(State(state): State<AppState>, Json(req): Json<ProcessSpawnRequest>) -> Json<serde_json::Value> {
    let process_manager = state.process_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, ProcessManager> = process_manager.lock().await;
    match manager.spawn(&req.command, req.cwd.as_deref(), req.env_vars).await {
        Ok(info) => Json(serde_json::to_value(info).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn process_kill(Path(pid): Path<u32>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let process_manager = state.process_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, ProcessManager> = process_manager.lock().await;
    match manager.kill(pid).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn process_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let process_manager = state.process_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, ProcessManager> = process_manager.lock().await;
    let processes = manager.list_processes().await;
    Json(serde_json::json!({ "processes": processes }))
}

async fn clipboard_read(State(state): State<AppState>) -> Json<serde_json::Value> {
    let clipboard_manager = state.clipboard_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, ClipboardManager> = clipboard_manager.lock().await;
    match manager.read() {
        Ok(content) => Json(serde_json::to_value(content).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct ClipboardWriteRequest {
    pub text: Option<String>,
}

async fn clipboard_write(State(state): State<AppState>, Json(req): Json<ClipboardWriteRequest>) -> Json<serde_json::Value> {
    let clipboard_manager = state.clipboard_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, ClipboardManager> = clipboard_manager.lock().await;
    let content = crate::clipboard::ClipboardContent {
        text: req.text,
        html: None,
        image: None,
    };
    match manager.write(&content) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct NotificationRequest {
    pub title: String,
    pub body: String,
    pub urgency: Option<String>,
}

async fn notification_show(State(state): State<AppState>, Json(req): Json<NotificationRequest>) -> Json<serde_json::Value> {
    let notification_manager = state.notification_manager.clone();
    let manager: tokio::sync::MutexGuard<'_, NotificationManager> = notification_manager.lock().await;
    let options = crate::notification::NotificationOptions {
        title: req.title,
        body: req.body,
        icon: None,
        silent: None,
        urgency: req.urgency,
        timeout: None,
    };
    match manager.show(&options) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct LogsReadRequest {
    pub level: Option<String>,
    pub source: Option<String>,
    pub search: Option<String>,
    pub limit: Option<usize>,
}

async fn logs_read(State(state): State<AppState>, Query(req): Query<LogsReadRequest>) -> Json<serde_json::Value> {
    let logger = state.logger.clone();
    let logger_guard: tokio::sync::MutexGuard<'_, Logger> = logger.lock().await;
    let filter = crate::logger::LogFilter {
        level: req.level,
        source: req.source,
        from_time: None,
        to_time: None,
        search: req.search,
    };
    match logger_guard.read_logs(Some(filter), req.limit.unwrap_or(100)) {
        Ok(entries) => Json(serde_json::json!({ "logs": entries })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct LogsClearRequest {
    pub days: Option<u32>,
}

async fn logs_clear(State(state): State<AppState>, Json(req): Json<LogsClearRequest>) -> Json<serde_json::Value> {
    let logger = state.logger.clone();
    let logger_guard: tokio::sync::MutexGuard<'_, Logger> = logger.lock().await;
    match logger_guard.clear_old_logs(req.days.unwrap_or(30)) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn watcher_start(State(state): State<AppState>) -> Json<serde_json::Value> {
    let file_watcher = state.file_watcher.clone();
    let watcher: tokio::sync::MutexGuard<'_, FileWatcher> = file_watcher.lock().await;
    match watcher.start().await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct WatcherWatchRequest {
    pub path: String,
}

async fn watcher_watch(State(state): State<AppState>, Json(req): Json<WatcherWatchRequest>) -> Json<serde_json::Value> {
    let file_watcher = state.file_watcher.clone();
    let watcher: tokio::sync::MutexGuard<'_, FileWatcher> = file_watcher.lock().await;
    match watcher.watch(&req.path).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn watcher_unwatch(State(state): State<AppState>, Json(req): Json<WatcherWatchRequest>) -> Json<serde_json::Value> {
    let file_watcher = state.file_watcher.clone();
    let watcher: tokio::sync::MutexGuard<'_, FileWatcher> = file_watcher.lock().await;
    match watcher.unwatch(&req.path).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn update_check() -> Json<serde_json::Value> {
    let updater = AutoUpdater::new(
        "https://clawparrot.com/updates",
        env!("CARGO_PKG_VERSION"),
        std::path::PathBuf::from(std::env::temp_dir()).join("claude-desktop-updates"),
    );
    match updater.check_for_updates().await {
        Ok(Some(info)) => Json(serde_json::to_value(info).unwrap_or_default()),
        Ok(None) => Json(serde_json::json!({ "up_to_date": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct UpdateDownloadRequest {
    pub url: String,
}

async fn update_download(Json(req): Json<UpdateDownloadRequest>) -> Json<serde_json::Value> {
    let updater = AutoUpdater::new(
        "https://clawparrot.com/updates",
        env!("CARGO_PKG_VERSION"),
        std::path::PathBuf::from(std::env::temp_dir()).join("claude-desktop-updates"),
    );
    match updater.download_update(&req.url).await {
        Ok(path) => Json(serde_json::json!({ "path": path.to_string_lossy() })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

use crate::worktree::{WorktreeManager, CreateWorktreeRequest, MergeWorktreeRequest};
use crate::ide::{IdeBridge, IdeConfig};

static WORKTREE_MANAGER: std::sync::LazyLock<tokio::sync::Mutex<Option<WorktreeManager>>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(None));

static IDE_BRIDGE: std::sync::LazyLock<tokio::sync::Mutex<Option<IdeBridge>>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(None));

async fn worktree_create(Json(req): Json<CreateWorktreeRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = WORKTREE_MANAGER.lock().await;
    if guard.is_none() {
        *guard = Some(WorktreeManager::with_cwd(None));
    }
    if let Some(mgr) = guard.as_ref() {
        match mgr.create_worktree(req).await {
            Ok(info) => Ok(Json(serde_json::json!({ "success": true, "worktree": info }))),
            Err(e) => {
                tracing::info!(target: "worktree", "Create failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn worktree_list() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.list_worktrees().await {
            Ok(list) => Ok(Json(serde_json::json!({ "success": true, "worktrees": list }))),
            Err(e) => {
                tracing::info!(target: "worktree", "List failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Ok(Json(serde_json::json!({ "success": true, "worktrees": [] })))
    }
}

async fn worktree_get(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.get_worktree(&id).await {
            Some(info) => Ok(Json(serde_json::json!({ "success": true, "worktree": info }))),
            None => Err(StatusCode::NOT_FOUND),
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn worktree_remove(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.remove_worktree(&id).await {
            Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
            Err(e) => {
                tracing::info!(target: "worktree", "Remove failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn worktree_merge(Json(req): Json<MergeWorktreeRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.merge_worktree(req).await {
            Ok(output) => Ok(Json(serde_json::json!({ "success": true, "output": output }))),
            Err(e) => {
                tracing::info!(target: "worktree", "Merge failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn worktree_sync() -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = WORKTREE_MANAGER.lock().await;
    if guard.is_none() {
        *guard = Some(WorktreeManager::with_cwd(None));
    }
    if let Some(mgr) = guard.as_ref() {
        match mgr.sync_from_git().await {
            Ok(list) => Ok(Json(serde_json::json!({ "success": true, "worktrees": list }))),
            Err(e) => {
                tracing::info!(target: "worktree", "Sync failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn agent_list() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        let agents = mgr.list_agents().await;
        Ok(Json(serde_json::json!({ "success": true, "agents": agents })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "agents": [] })))
    }
}

async fn agent_get(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.get_agent(&id).await {
            Some(info) => Ok(Json(serde_json::json!({ "success": true, "agent": info }))),
            None => Err(StatusCode::NOT_FOUND),
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn agent_cancel(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.cancel_agent(&id).await {
            Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
            Err(e) => {
                tracing::info!(target: "agent", "Cancel failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn ide_status() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = IDE_BRIDGE.lock().await;
    if let Some(bridge) = guard.as_ref() {
        let status = bridge.get_status().await;
        Ok(Json(serde_json::json!({ "success": true, "status": status })))
    } else {
        Ok(Json(serde_json::json!({
            "success": true,
            "status": { "server_running": false, "port": 0, "active_connections": 0, "total_connections": 0 }
        })))
    }
}

async fn ide_start() -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = IDE_BRIDGE.lock().await;
    if guard.is_none() {
        *guard = Some(IdeBridge::new(IdeConfig::default()));
    }
    if let Some(bridge) = guard.as_ref() {
        match bridge.start_server().await {
            Ok(port) => Ok(Json(serde_json::json!({ "success": true, "port": port }))),
            Err(e) => {
                tracing::error!(target: "ide", "Start failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn ide_stop() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = IDE_BRIDGE.lock().await;
    if let Some(bridge) = guard.as_ref() {
        bridge.stop_server().await;
        Ok(Json(serde_json::json!({ "success": true })))
    } else {
        Ok(Json(serde_json::json!({ "success": true })))
    }
}

async fn ide_connections() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = IDE_BRIDGE.lock().await;
    if let Some(bridge) = guard.as_ref() {
        let conns = bridge.list_connections().await;
        Ok(Json(serde_json::json!({ "success": true, "connections": conns })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "connections": [] })))
    }
}

async fn ide_disconnect(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = IDE_BRIDGE.lock().await;
    if let Some(bridge) = guard.as_ref() {
        match bridge.disconnect(&id).await {
            Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
            Err(e) => {
                tracing::error!(target: "ide", "Disconnect failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

use crate::analytics::{AnalyticsStore, TrackEventRequest};

static ANALYTICS_STORE: std::sync::LazyLock<tokio::sync::Mutex<Option<AnalyticsStore>>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(None));

async fn analytics_track(Json(req): Json<TrackEventRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        match store.track_event(&req).await {
            Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
            Err(e) => {
                tracing::info!(target: "analytics", "Track failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn analytics_daily(Path(date): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        match store.get_daily_stats(&date).await {
            Some(stats) => Ok(Json(serde_json::json!({ "success": true, "stats": stats }))),
            None => Err(StatusCode::NOT_FOUND),
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn analytics_range(Query(params): Query<HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        let from = params.get("from").map(|s| s.as_str()).unwrap_or("2025-01-01");
        let to = params.get("to").map(|s| s.as_str()).unwrap_or("2099-12-31");
        let stats = store.get_stats_range(from, to).await;
        Ok(Json(serde_json::json!({ "success": true, "stats": stats })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "stats": [] })))
    }
}

async fn analytics_summary(Query(params): Query<HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        let days: u32 = params.get("days").and_then(|d| d.parse().ok()).unwrap_or(30);
        let summary = store.get_usage_summary(days).await;
        Ok(Json(serde_json::json!({ "success": true, "summary": summary })))
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn analytics_event_counts(Query(params): Query<HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        let days: u32 = params.get("days").and_then(|d| d.parse().ok()).unwrap_or(30);
        let counts = store.get_event_type_counts(days);
        Ok(Json(serde_json::json!({ "success": true, "counts": counts })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "counts": [] })))
    }
}

async fn analytics_recent_events(Query(params): Query<HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        let limit: usize = params.get("limit").and_then(|d| d.parse().ok()).unwrap_or(50);
        let events = store.get_recent_events(limit);
        Ok(Json(serde_json::json!({ "success": true, "events": events })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "events": [] })))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowExecuteRequest {
    pub goal: String,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub workspace: Option<String>,
    #[serde(default)]
    pub resume_roles: Option<Vec<serde_json::Value>>,
}

// === Memory Handlers ===
async fn memories_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Primary source: TencentDB tiered store (legacy table retired)
            let memories = crate::memory::tiered::list_all_tiered(conn, "", 200)
                .unwrap_or_default();
            let memories = crate::memory::tiered::tiered_to_legacy(memories)
                .iter()
                .map(|m| serde_json::json!({
                    "id": m.id,
                    "workspace_path": m.workspace_path,
                    "memory_type": m.memory_type,
                    "content": m.summary,
                    "importance": m.importance,
                    "created_at": m.created_at,
                    "tags": m.tags,

                }))
                .collect::<Vec<_>>();
            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({"memories": memories}))
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"memories": [], "error": "Failed to load memories"})),
    }
}

async fn memories_search(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let query = params.get("q").cloned().unwrap_or_default();
    let workspace = params.get("workspace").cloned().unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Primary source: TencentDB tiered store (legacy table retired)
            let memories: Vec<crate::db::memory_repo::MemoryRow> = if query.is_empty() {
                crate::memory::tiered::tiered_to_legacy(
                    crate::memory::tiered::list_all_tiered(conn, "", 100).unwrap_or_default(),
                )
            } else {
                crate::memory::tiered::search_with_fallback(conn, "", "default", &query, None, 100)
                    .unwrap_or_default()
            };
            let items: Vec<_> = memories.iter()
                .filter(|m| workspace.is_empty() || m.workspace_path.contains(&workspace))
                .map(|m| serde_json::json!({
                    "id": m.id,
                    "workspace_path": m.workspace_path,
                    "memory_type": m.memory_type,
                    "content": m.summary,
                    "importance": m.importance,
                    "created_at": m.created_at,
                    "tags": m.tags,
                }))
                .collect();
            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({"memories": items}))
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"memories": []})),
    }
}

async fn memories_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let all = crate::memory::tiered::tiered_to_legacy(
                crate::memory::tiered::list_all_tiered(conn, "", 1000).unwrap_or_default(),
            );
            let total = all.len() as i64;
            let mut by_type: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
            let mut by_importance: std::collections::HashMap<i32, i64> = std::collections::HashMap::new();
            for m in &all {
                *by_type.entry(m.memory_type.clone()).or_insert(0) += 1;
                *by_importance.entry(m.importance).or_insert(0) += 1;
            }
            let by_type_vec: Vec<_> = by_type.into_iter().collect();
            let by_imp_vec: Vec<_> = by_importance.into_iter().collect();
            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({
                "total": total,
                "by_type": by_type_vec,
                "by_importance": by_imp_vec,
            }))
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"total": 0, "by_type": [], "by_importance": []})),
    }
}

async fn memories_delete(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Delete from primary tiered store (and legacy leftovers for a clean UI)
            crate::memory::tiered::delete_tiered_memory(conn, &id).ok();
            let _ = conn.execute("DELETE FROM memories WHERE id = ?1", rusqlite::params![id]);
            Ok::<(), anyhow::Error>(())
        })
    }).await;
    match result {
        Ok(Ok(Ok(()))) => Json(serde_json::json!({"ok": true})),
        _ => Json(serde_json::json!({"ok": false})),
    }
}

async fn memories_backfill(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let conversations = crate::db::conversation_repo::list_conversations(conn).unwrap_or_default();
            let existing = crate::db::memory_repo::list_all_memories(conn, 10000).unwrap_or_default();
            let existing_conv_ids: std::collections::HashSet<String> = existing.iter().map(|m| m.conversation_id.clone()).collect();
            let mut created = 0i64;
            let batch: Vec<_> = conversations.iter().filter(|c| !existing_conv_ids.contains(&c.id)).take(20).collect();
            for conv in &batch {
                let msgs = crate::db::message_repo::get_messages_by_conversation(conn, &conv.id).unwrap_or_default();
                if msgs.len() < 2 { continue; }
                let (sum, mem_tags, mem_importance) = crate::db::memory_repo::build_smart_summary(&msgs);
                let summary = if sum.is_empty() {
                    msgs.iter().rev()
                        .find(|m| m.role == "user")
                        .map(|m| format!("Context: {}", m.content.chars().take(200).collect::<String>()))
                        .unwrap_or_else(|| "conversation".to_string())
                } else { sum };
                let mem_type = if summary.contains("Decisions:") || summary.contains("决定") { "decision" }
                    else if summary.contains("Preferences:") || summary.contains("偏好") { "preference" }
                    else if summary.contains("Key facts:") { "fact" }
                    else { "context" };
                let ws = conv.workspace_path.clone().unwrap_or_default();
                // Write into primary tiered store (legacy table retired)
                let tier_row = crate::memory::tiered::TieredMemoryRow {
                    id: uuid::Uuid::new_v4().to_string(),
                    workspace_path: ws,
                    team_id: "default".to_string(),
                    conversation_id: conv.id.clone(),
                    tier: crate::memory::tiered::Tier::from_i32(
                        if mem_type == "decision" || mem_type == "preference" { 2 } else { 1 },
                    ),
                    visibility: crate::memory::tiered::Visibility::from_str("private"),
                    content: summary.clone(),
                    tags: mem_tags.clone(),
                    importance: mem_importance,
                    created_at: chrono::Utc::now().to_rfc3339(),
                };
                let _ = crate::memory::tiered::insert_tiered_memory(conn, &tier_row);
                created += 1;
            }
            tracing::info!(target: "memory", "Backfill: created {} memories from {} conversations", created, conversations.len());
            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({"created": created}))
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"created": 0, "error": "Failed"})),
    }
}

// === Swarm Session Persistence Handlers ===

async fn swarm_sessions_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::swarm_repo::list_sessions(conn))
    }).await;
    match result {
        Ok(Ok(Ok(sessions))) => Json(serde_json::json!({ "sessions": sessions })),
        _ => Json(serde_json::json!({ "sessions": [] })),
    }
}

async fn swarm_sessions_create(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("new task").to_string();
    let workspace = body.get("workspace").and_then(|v| v.as_str()).map(|s| s.to_string());
    let db = state.db_manager.clone();
    let sid = id.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::swarm_repo::create_session(conn, &sid, &title, workspace.as_deref()))
    }).await;
    match result {
        Ok(Ok(Ok(_))) => Json(serde_json::json!({ "id": id })),
        other => {
            tracing::error!(target: "bridge", "swarm session create failed: {:?}", other);
            Json(serde_json::json!({ "error": "Failed to create session" }))
        }
    }
}

async fn swarm_sessions_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::swarm_repo::get_session(conn, &id))
    }).await;
    match result {
        Ok(Ok(Ok(Some(session)))) => Json(serde_json::json!(session)),
        _ => Json(serde_json::json!({ "error": "Session not found" })),
    }
}

async fn swarm_sessions_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::swarm_repo::delete_session(conn, &id))
    }).await;
    match result {
        Ok(Ok(Ok(_))) => Json(serde_json::json!({ "ok": true })),
        _ => Json(serde_json::json!({ "error": "Failed to delete session" })),
    }
}

async fn swarm_messages_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::swarm_repo::get_messages(conn, &id))
    }).await;
    match result {
        Ok(Ok(Ok(msgs))) => Json(serde_json::json!({ "messages": msgs })),
        _ => Json(serde_json::json!({ "messages": [] })),
    }
}

async fn swarm_messages_add(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let id_clone = id.clone();
    let role = body.get("role").and_then(|v| v.as_str()).unwrap_or("system").to_string();
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let agent_name = body.get("agent_name").and_then(|v| v.as_str()).map(|s| s.to_string());
    let agent_icon = body.get("agent_icon").and_then(|v| v.as_str()).map(|s| s.to_string());
    let agent_color = body.get("agent_color").and_then(|v| v.as_str()).map(|s| s.to_string());
    let msg_type = body.get("type").and_then(|v| v.as_str()).map(|s| s.to_string());
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::swarm_repo::insert_message(
            conn, &id_clone, &session_id, &role, &content,
            agent_name.as_deref(), agent_icon.as_deref(), agent_color.as_deref(), msg_type.as_deref(),
        ))
    }).await;
    match result {
        Ok(Ok(Ok(_))) => Json(serde_json::json!({ "id": id })),
        _ => Json(serde_json::json!({ "error": "Failed to add message" })),
    }
}

async fn swarm_status_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("running").to_string();
    let agent_status = body.get("agent_status").map(|v| {
        if v.is_string() { v.as_str().unwrap_or("").to_string() } else { serde_json::to_string(v).unwrap_or_default() }
    });
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::swarm_repo::update_session_status(conn, &id, &status, agent_status.as_deref()))
    }).await;
    match result {
        Ok(Ok(Ok(_))) => Json(serde_json::json!({ "ok": true })),
        _ => Json(serde_json::json!({ "error": "Failed to update status" })),
    }
}

async fn swarm_session_rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::swarm_repo::update_session_title(conn, &id, &title))
    }).await;
    match result {
        Ok(Ok(Ok(_))) => Json(serde_json::json!({ "ok": true })),
        _ => Json(serde_json::json!({ "error": "Failed to rename" })),
    }
}
/// MetaGPT workflow endpoint - uses the ported MetaGPT orchestration system
async fn metagpt_workflow_stream(
    State(state): State<AppState>,
    Json(req): Json<WorkflowExecuteRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let native_engine = state.native_engine.clone();
    let model = req.model.clone().unwrap_or_else(|| "deepseek-v4-flash-free".to_string());
    let workspace = req.workspace.clone();
    let goal = req.goal.clone();

    let engine_guard = native_engine.lock().await;
    let resolved_provider = if let Some(engine) = engine_guard.as_ref() {
        engine.resolve_provider(&model).await
    } else {
        None
    };
    drop(engine_guard);

    let resolved_provider = match resolved_provider {
        Some(rp) => rp,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let (tx, mut rx) = tokio::sync::broadcast::channel::<crate::orchestration::WorkflowEvent>(256);
    let workspace_str = workspace.clone();

    // 续跑参数：已完成角色的 (name, cause_by, output) 列表
    let resume_outputs: Vec<(String, String, String)> = req.resume_roles
        .unwrap_or_default()
        .iter()
        .filter_map(|v| Some((
            v.get("name")?.as_str()?.to_string(),
            v.get("cause_by")?.as_str()?.to_string(),
            v.get("output")?.as_str()?.to_string(),
        )))
        .collect();

    let db_for_workflow = state.db_manager.clone();
    let embedding_for_workflow = state.embedding_engine.clone();
    let tx_panic = tx.clone();
    tokio::spawn(async move {
        // 内层 spawn 隔离 panic：任务崩溃时广播 workflow_failed，
        // 否则 broadcast sender 直接 drop，SSE 静默断流，前端表现为"中途卡住"
        let inner = tokio::spawn(async move {
            let _ = crate::orchestration::metagpt_workflow(&goal, &resolved_provider, workspace_str.as_deref(), tx, Some(db_for_workflow), Some(embedding_for_workflow), resume_outputs).await;
        });
        if let Err(je) = inner.await {
            if je.is_panic() {
                tracing::error!(target: "metagpt", "Workflow task panicked: {}", je);
                let _ = tx_panic.send(crate::orchestration::WorkflowEvent {
                    event_type: "workflow_failed".to_string(),
                    task_id: None,
                    message: format!("Workflow panicked: {}", je),
                    data: None,
                    timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
                });
            }
        }
    });

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    let is_done = event.event_type == "workflow_completed" || event.event_type == "workflow_failed";
                    yield Ok::<axum::response::sse::Event, std::convert::Infallible>(
                        axum::response::sse::Event::default().event("workflow").data(data)
                    );
                    if is_done { break; }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(target: "metagpt", "SSE stream lagged, skipped {} events", n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!(target: "metagpt", "SSE stream closed (all senders dropped)");
                    break;
                }
            }
        }
    };

    Ok(axum::response::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new()))
}

// ─── TencentDB Agent Memory bridge handlers ──────────────────────────────
//
// Inlined TencentDB Agent Memory client (see memory::tencentdb_client).
// These endpoints expose configuration, health, search, insert and promote
// operations over the local bridge so the React frontend can drive the
// memory system without spawning an external service.

async fn tdai_health(
    State(state): State<AppState>,
) -> Json<crate::memory::tencentdb_client::HealthInfo> {
    Json(state.tdai_client.health().await)
}

async fn tdai_get_config(
    State(state): State<AppState>,
) -> Json<crate::memory::tencentdb_client::TencentDBConfig> {
    Json(state.tdai_client.config().await)
}

#[derive(Deserialize)]
struct TdaiConfigPayload {
    base_url: Option<String>,
    user_key: Option<String>,
    team_id: Option<String>,
    agent_id: Option<String>,
    user_id: Option<String>,
    space_id: Option<String>,
    enabled: Option<bool>,
}

async fn tdai_set_config(
    State(state): State<AppState>,
    Json(p): Json<TdaiConfigPayload>,
) -> Json<serde_json::Value> {
    let mut cur = state.tdai_client.config().await;
    if let Some(v) = p.base_url { cur.base_url = v; }
    if let Some(v) = p.user_key { cur.user_key = v; }
    if let Some(v) = p.team_id { cur.team_id = v; }
    if let Some(v) = p.agent_id { cur.agent_id = v; }
    if let Some(v) = p.user_id { cur.user_id = v; }
    if let Some(v) = p.space_id { cur.space_id = v; }
    if let Some(v) = p.enabled { cur.enabled = v; }
    state.tdai_client.update_config(cur.clone()).await;
    // Persist to DB
    let db = state.db_manager.clone();
    let cfg = cur.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let _ = db.with_conn(|conn| crate::memory::tencentdb_client::save_config(conn, &cfg));
    }).await;
    Json(serde_json::json!({ "ok": true, "config": cur }))
}

async fn tdai_auth_verify(
    State(state): State<AppState>,
) -> Json<crate::memory::tencentdb_client::AuthVerifyResponse> {
    Json(state.tdai_client.verify_auth().await)
}

#[derive(Deserialize)]
struct TdaiSearchPayload {
    workspace_path: Option<String>,
    query: String,
    top_k: Option<i64>,
}

async fn tdai_search(
    State(state): State<AppState>,
    Json(p): Json<TdaiSearchPayload>,
) -> Json<crate::memory::tencentdb_client::TdaiSearchResponse> {
    let workspace = p.workspace_path.unwrap_or_default();
    let top_k = p.top_k.unwrap_or(5);
    let db = state.db_manager.clone();
    let client = state.tdai_client.clone();
    let q = p.query.clone();
    let res = tokio::task::spawn_blocking(move || -> anyhow::Result<crate::memory::tencentdb_client::TdaiSearchResponse> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async move {
            client.search(&workspace, &q, top_k, &*db).await
        })
    }).await;
    match res {
        Ok(Ok(v)) => Json(v),
        _ => Json(crate::memory::tencentdb_client::TdaiSearchResponse::default()),
    }
}

#[derive(Deserialize)]
struct TdaiAddPayload {
    workspace_path: String,
    content: String,
    importance: Option<i32>,
    tags: Option<String>,
    tier: Option<i32>,
}

async fn tdai_add_memory(
    State(state): State<AppState>,
    Json(p): Json<TdaiAddPayload>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let client = state.tdai_client.clone();
    let ws = p.workspace_path.clone();
    let content = p.content.clone();
    let imp = p.importance.unwrap_or(3);
    let tags = p.tags.clone().unwrap_or_default();
    let tier = p.tier.unwrap_or(1);
    let res = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        db.with_conn(|conn| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async move { client.add_memory_with_tier(&ws, &content, imp, &tags, tier, conn).await })
        }).and_then(|inner| inner)
    }).await;
    match res {
        Ok(Ok(id)) => Json(serde_json::json!({ "ok": true, "id": id })),
        Ok(Err(e)) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
}

async fn tdai_promote(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let client = state.tdai_client.clone();
    let res = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<crate::memory::tiered::TieredMemoryRow>> {
        db.with_conn(|conn| client.promote(conn, &id)).and_then(|inner| inner)
    }).await;
    match res {
        Ok(Ok(Some(row))) => Json(serde_json::json!({ "ok": true, "tier": row.tier.as_i32() })),
        Ok(Ok(None)) => Json(serde_json::json!({ "ok": false, "error": "not found" })),
        Ok(Err(e)) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct TdaiStatsQuery {
    workspace_path: Option<String>,
}

async fn tdai_stats(
    State(state): State<AppState>,
    Query(q): Query<TdaiStatsQuery>,
) -> Json<serde_json::Value> {
    let workspace = q.workspace_path.unwrap_or_default();
    let db = state.db_manager.clone();
    let client = state.tdai_client.clone();
    let res = tokio::task::spawn_blocking(move || -> anyhow::Result<std::collections::HashMap<String, i64>> {
        db.with_conn(|conn| client.stats(conn, &workspace)).and_then(|inner| inner)
    }).await;
    match res {
        Ok(Ok(m)) => Json(serde_json::json!({ "ok": true, "stats": m })),
        _ => Json(serde_json::json!({ "ok": false, "stats": {} })),
    }
}
