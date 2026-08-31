# 数据模型 (Data Model)

## 1. SQLite 数据库 Schema

数据库文件路径: `{data_dir}/claude_desktop.db`

### 1.1 对话系统

```sql
-- 对话表
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,              -- UUID v4
    title TEXT,                        -- 对话标题
    model TEXT,                        -- 使用的模型 ID
    provider TEXT,                     -- 提供商名称
    workspace_path TEXT,               -- 关联工作区路径
    project_id TEXT,                   -- 关联项目 ID
    research_mode INTEGER DEFAULT 0,   -- 是否研究模式
    pinned INTEGER DEFAULT 0,          -- 是否置顶
    archived INTEGER DEFAULT 0,        -- 是否归档
    created_at TEXT NOT NULL,          -- RFC3339 格式
    updated_at TEXT NOT NULL,          -- RFC3339 格式
    message_count INTEGER DEFAULT 0    -- 消息计数
);

-- 消息表
CREATE TABLE messages (
    id TEXT PRIMARY KEY,               -- UUID v4
    conversation_id TEXT NOT NULL,      -- FK → conversations(id)
    role TEXT NOT NULL,                 -- 'user' | 'assistant' | 'system'
    content TEXT NOT NULL,              -- 消息内容 (Markdown)
    thinking TEXT,                      -- Claude 思考过程
    created_at TEXT NOT NULL,           -- RFC3339 格式
    is_compact_boundary INTEGER DEFAULT 0,  -- 上下文压缩边界
    sort_order INTEGER NOT NULL,        -- 排序序号
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- 工具调用表
CREATE TABLE tool_calls (
    id TEXT PRIMARY KEY,               -- UUID v4
    message_id TEXT NOT NULL,           -- FK → messages(id)
    name TEXT NOT NULL,                 -- 工具名称
    input TEXT,                         -- JSON 格式输入参数
    output TEXT,                        -- JSON 格式输出结果
    is_error INTEGER DEFAULT 0,         -- 是否执行错误
    sort_order INTEGER NOT NULL,        -- 排序序号
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

-- 附件表
CREATE TABLE attachments (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,           -- FK → messages(id)
    file_name TEXT,
    file_type TEXT,
    mime_type TEXT,
    file_size INTEGER,
    source TEXT,
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);
```

### 1.2 项目系统

```sql
-- 项目表
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    instructions TEXT,                   -- 项目指令/上下文
    workspace_path TEXT,
    is_archived INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 项目文件表
CREATE TABLE project_files (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,           -- FK → projects(id)
    file_name TEXT,
    file_path TEXT,
    file_size INTEGER,
    mime_type TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
```

### 1.3 记忆系统

```sql
-- 记忆表 (基础列)
CREATE TABLE memories (
    id TEXT PRIMARY KEY,
    workspace_path TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    summary TEXT NOT NULL,              -- 记忆摘要
    tags TEXT NOT NULL DEFAULT '',      -- 逗号分隔标签
    created_at TEXT NOT NULL
);

-- V2 迁移: 添加 memory_type 和 importance 列 + FTS5 虚拟表
-- V3 迁移: 添加向量嵌入表
```

### 1.4 Swarm (多智能体协作)

```sql
-- Swarm 会话表
CREATE TABLE swarm_sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    workspace TEXT,
    status TEXT NOT NULL DEFAULT 'running',
    agent_status TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

-- Swarm 消息表
CREATE TABLE swarm_messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES swarm_sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    agent_name TEXT,
    agent_icon TEXT,
    agent_color TEXT,
    type TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
```

## 2. Rust 数据结构 (内存模型)

### 2.1 Provider 模型 (`provider_manager.rs`)

```rust
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub api_format: ApiFormat,      // Anthropic | OpenAI
    pub models: Vec<ModelConfig>,
    pub enabled: bool,
    pub web_search_strategy: Option<String>,
}

pub enum ApiFormat {
    Anthropic,
    OpenAI,
}

pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub max_tokens: Option<u32>,
    pub context_window: Option<u32>,
    pub supports_vision: bool,
    pub supports_web_search: bool,
}

pub struct ResolvedProvider {
    pub provider: Provider,
    pub model: ModelConfig,
}
```

### 2.2 配置模型 (`config/mod.rs`)

```rust
pub struct AppConfig {
    pub version: String,
    pub user: UserConfig,
    pub api: ApiConfig,
    pub providers: Vec<ProviderConfig>,
    pub mcp: McpConfig,
    pub appearance: AppearanceConfig,
    pub behavior: BehaviorConfig,
    pub shortcuts: ShortcutConfig,
    pub logging: LoggingConfig,
}
```

### 2.3 MCP 模型 (`mcp/mod.rs`)

```rust
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub enabled: bool,
}

pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub server_name: String,
}

pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
}

pub struct McpServerStatus {
    pub id: String,
    pub name: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub tools_count: usize,
    pub error: Option<String>,
}
```

### 2.4 对话请求/响应模型

```rust
pub struct ChatRequest {
    pub conversation_id: String,
    pub messages: Vec<Value>,
    pub model: String,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u32>,
    pub workspace_path: Option<String>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub web_search_enabled: Option<bool>,
    pub reasoning_effort: Option<String>,
    pub extended_thinking: bool,
}

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

## 3. 索引策略

| 索引 | 表 | 列 | 用途 |
|------|-----|-----|------|
| `idx_messages_conversation_id` | messages | conversation_id | 按对话查询消息 |
| `idx_messages_created_at` | messages | created_at | 时间排序 |
| `idx_conversations_updated_at` | conversations | updated_at | 最近对话排序 |
| `idx_conversations_model` | conversations | model | 按模型筛选 |
| `idx_tool_calls_message_id` | tool_calls | message_id | 按消息查工具调用 |
| `idx_attachments_message_id` | attachments | message_id | 按消息查附件 |
| `idx_project_files_project_id` | project_files | project_id | 按项目查文件 |
| `idx_memories_workspace_path` | memories | workspace_path | 按工作区查记忆 |
| `idx_memories_created_at` | memories | created_at | 时间排序 |
| `idx_swarm_messages_session` | swarm_messages | session_id | 按会话查消息 |
| `idx_swarm_sessions_updated_at` | swarm_sessions | updated_at | 最近会话排序 |
