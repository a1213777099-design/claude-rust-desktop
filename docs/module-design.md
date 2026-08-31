# 模块设计 (Module Design)

## 1. 核心模块详解

### 1.1 Bridge Server (`src-tauri/src/bridge/`)

**文件**: `mod.rs`, `state.rs`, `memory_handlers_v2.rs`

BridgeServer 是整个后端的 HTTP 入口，基于 **Axum 0.8** 框架，监听 `127.0.0.1:30080`。

```rust
pub struct BridgeServer {
    engine_pool: Arc<Mutex<EnginePool>>,
    native_engine: Arc<Mutex<Option<NativeEngine>>>,
    mcp_server_manager: Arc<McpServerManager>,
    stream_manager: Arc<Mutex<StreamManager>>,
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
}
```

**共享状态 (AppState)**:

```rust
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
}
```

**路由分组**:

| 路由前缀 | 功能 | handler 位置 |
|----------|------|-------------|
| `/api/chat` | 对话 CRUD + SSE 流式 | `mod.rs` |
| `/api/memories` | 记忆 CRUD + FTS5 搜索 | `memory_handlers_v2.rs` |
| `/api/projects` | 项目管理 | `mod.rs` |
| `/api/swarm` | MetaGPT 工作流 | `mod.rs` |
| `/api/research` | 深度研究模式 | `mod.rs` |
| `/api/tools` | 工具执行 | `mod.rs` |
| `/api/skills` | 技能管理 | `mod.rs` |
| `/api/mcp` | MCP 服务器配置 | `mod.rs` |
| `/api/providers` | Provider 管理 | `mod.rs` |
| `/api/system` | 系统状态 | `mod.rs` |

### 1.2 Native Engine (`src-tauri/src/native_engine/`)

**模块组成**:

| 文件 | 类/结构体 | 职责 |
|------|----------|------|
| `engine_core.rs` | `NativeEngine`, `QueryEngine`, `ConversationState` | 引擎入口、对话查询编排 |
| `anthropic_client.rs` | `AnthropicClient` | Claude Messages API 调用 |
| `openai_client.rs` | `OpenAIClient` | OpenAI Chat Completions API 调用 |
| `provider_manager.rs` | `ProviderManager`, `Provider`, `ModelConfig` | Provider 注册、路由、缓存 |
| `session_manager.rs` | `SessionManager` | 会话状态持久化 |
| `tool_loop.rs` | `ToolLoopExecutor`, `EngineEvent` | 工具执行循环 |

**核心流程 (Tool Loop)**:

```
1. 用户消息 → QueryEngine.send_message()
2. → 解析 Provider (Anthropic/OpenAI)
3. → 调用 LLM API (流式)
4. → 解析 Stream Events (Text/Thinking/ToolUse)
5. → 工具执行 (内置工具/MCP/Skills)
6. → 继续循环直到 stop_reason=end_turn
7. → 持久化消息和工具调用
```

**EngineEvent 枚举 (流式事件)**:

```rust
pub enum EngineEvent {
    Text(String),
    Thinking(String),
    ToolUseStart { tool_use_id, tool_name, tool_input, text_before },
    ToolArgDelta { tool_use_id, delta },
    ToolUseDone { tool_use_id, tool_name, tool_input, output, is_error },
    MessageStart { model },
    MessageDelta { stop_reason },
    MessageStop { full_text, stop_reason },
    Error(String),
    Usage(Value),
    AskUser { question, options },
}
```

### 1.3 Memory System (`src-tauri/src/memory/`)

**模块组成**:

| 文件 | 类/结构体 | 职责 |
|------|----------|------|
| `mod.rs` | 模块导出 | |
| `config.rs` | `MemoryConfig` | 记忆系统配置 |
| `error.rs` | `MemoryError` | 错误类型 |
| `embedding.rs` | `EmbeddingEngine` | 向量嵌入引擎 (fastembed + API + TF-IDF) |
| `vector_index.rs` | `VectorIndex` | 向量索引和相似度搜索 |
| `clustering.rs` | 聚类算法 | DBSCAN 聚类 |
| `compression.rs` | 压缩工具 | 记忆压缩 |
| `storage/mod.rs` | `MemoryStorage` trait | 存储后端抽象 |

**MemoryStorage Trait**:

```rust
pub trait MemoryStorage: Send + Sync {
    async fn store(&self, record: MemoryRecord) -> Result<()>;
    async fn search(&self, query: MemoryQuery) -> Result<Vec<MemoryRecord>>;
    async fn get(&self, id: &str) -> Result<Option<MemoryRecord>>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryRecord>>;
    // ... 15+ 抽象方法
}
```

**EmbeddingEngine (三级优先级)**:

1. **Local ONNX** (fastembed) — 默认，无网络依赖
2. **API-based** — 远程嵌入服务
3. **TF-IDF** — 回退方案

### 1.4 Orchestration (`src-tauri/src/orchestration/`)

**MetaGPT 工作流** (`metagpt_workflow`):

角色序列:
```
User Requirement
  → Product Manager (PRD)
    → Architect (Design)
      → Engineer (Code)
        → Reviewer (Review)
          → QA Engineer (Test)
            → DevOps (Deploy)
              → Project Manager (Summary)
```

核心结构:

```rust
pub struct Environment {
    pub history: MessageHistory,
    subscribers: HashMap<String, Vec<RoleType>>,
}

pub struct Role {
    pub name: String,
    pub profile: String,
    pub actions: Vec<Action>,
    pub watch: Vec<String>,
    pub memory: Memory,
}

pub struct Message {
    pub content: String,
    pub role: String,
    pub cause_by: CauseBy,
    pub sent_from: String,
}
```

**MultiAgent Orchestrator** (`src-tauri/src/multiagent/`):

```rust
pub enum AgentType { Planner, Researcher, Writer, Reviewer, Custom(String) }

pub struct AgentConfig {
    pub agent_id: String,
    pub agent_type: AgentType,
    pub model_id: Option<String>,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u32>,
    pub enabled: bool,
}
```

### 1.5 MCP Server Manager (`src-tauri/src/mcp/`)

**核心结构**:

```rust
pub struct McpServerManager {
    servers: Arc<RwLock<HashMap<String, McpServerState>>>,
    config_path: PathBuf,
}

pub struct McpServerState {
    pub connector: Option<Arc<Mutex<McpConnector>>>,
    pub config: McpServerConfig,
    pub status: McpServerStatus,
}

pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub server_name: String,
}
```

支持 **stdio** 和 **SSE** 两种传输协议，动态发现工具和资源。

### 1.6 Permissions System (`src-tauri/src/permissions/`)

**4 种权限模式**:

| 模式 | 行为 |
|------|------|
| `BypassPermissions` | 所有操作自动批准 |
| `AcceptEdits` | 编辑操作自动批准 (默认) |
| `AskPermissions` | 每次操作询问用户 |
| `PlanMode` | 只读操作允许，写入操作拒绝 |

**PermissionResult**:

```rust
pub enum PermissionResult {
    Granted,
    Denied(String),
    RequiresConfirmation(String),
}
```

### 1.7 Database Layer (`src-tauri/src/db/`)

**Repository 模式**:

| Repository | 表 | 主要方法 |
|-----------|-----|---------|
| `conversation_repo.rs` | conversations | CRUD, list, search |
| `message_repo.rs` | messages, tool_calls, attachments | CRUD, batch, pagination |
| `memory_repo.rs` | memories | insert, search, FTS5, tags |
| `project_repo.rs` | projects, project_files | CRUD, list |
| `swarm_repo.rs` | swarm_sessions, swarm_messages | CRUD |
| `migration.rs` | — | Schema 升级 (V2, V3) |

### 1.8 Frontend Architecture (`src/`)

**状态管理 (Zustand)**:

| Store | 文件 | 状态 |
|-------|------|------|
| useChatStore | `stores/chatStore.ts` | 对话列表、当前对话、消息 |
| useUIStore | `stores/uiStore.ts` | 侧边栏、主题、布局 |
| useAuthStore | `stores/authStore.ts` | 用户认证 |
| useProjectStore | `stores/projectStore.ts` | 项目管理 |
| useStreamingStore | `stores/streamingStore.ts` | SSE 流状态 |
| useToolStore | `stores/toolStore.ts` | 工具调用状态 |

**主要组件树**:

```
App.tsx
├── Sidebar (导航、对话列表、项目)
├── MainContent (主对话区域)
│   ├── ChatHeader (标题、操作菜单)
│   ├── MessageList (消息列表)
│   ├── ChatInput (输入框)
│   └── ToolCallCard (工具调用卡片)
├── SettingsPage (设置)
├── AgentPanel (多智能体)
├── AnalyticsPanel (分析)
├── TerminalPanel (终端)
├── SwarmCollaboration (MetaGPT 协作)
├── ProjectsPage (项目管理)
├── MemoryPanel (记忆面板)
└── admin/ (管理后台)
```
