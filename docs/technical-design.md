# 技术设计文档 — Claude Desktop (Tauri Edition)

## 1. 架构总览 (Architecture Overview)

### 1.1 系统架构图

```
┌─────────────────────────────────────────────────────────────────────┐
│                       前端层 (React + TypeScript)                     │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │
│  │ App.tsx  │ │ Sidebar  │ │ MainContent│ │ Memory   │ │ Settings │  │
│  │ (路由)   │ │ (导航)   │ │ (对话)    │ │ Panel    │ │ Page     │  │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │
│  │ Agent    │ │ Terminal │ │ Analytics│ │ Projects │ │ Artifacts│  │
│  │ Panel    │ │ Panel    │ │ Panel    │ │ Page     │ │ Page     │  │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  Stores (Zustand)           │  API Client (api.ts)            │  │
│  │  useChatStore, useUIStore,  │  → HTTP Bridge (127.0.0.1:30080)│  │
│  │  useAuthStore, etc.         │  → Tauri Commands (IPC)         │  │
│  └─────────────────────────────┴─────────────────────────────────┘  │
├─────────────────── Tauri IPC (invoke/events) ─────────────────────┤
│                       后端层 (Rust)                                  │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                    Bridge Server (Axum HTTP)                   │ │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐              │ │
│  │  │ Chat API   │  │ Memory API │  │ Project API│  ...更多路由  │ │
│  │  │ /api/chat  │  │ /api/mem   │  │ /api/proj  │              │ │
│  │  └────────────┘  └────────────┘  └────────────┘              │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                   Native Engine (核心引擎)                      │ │
│  │  ┌──────────────────┐  ┌──────────────────┐                   │ │
│  │  │ AnthropicClient  │  │  OpenAI Client   │                   │ │
│  │  │ (Claude API)     │  │  (兼容API)       │                   │ │
│  │  └──────────────────┘  └──────────────────┘                   │ │
│  │  ┌──────────────────┐  ┌──────────────────┐                   │ │
│  │  │ Tool Loop        │  │ Session Manager  │                   │ │
│  │  │ (工具执行循环)   │  │ (会话管理)       │                   │ │
│  │  └──────────────────┘  └──────────────────┘                   │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                 Memory System (记忆系统)                        │ │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────────────────┐   │ │
│  │  │ SQLite +   │  │ FTS5 全文  │  │ MemoryStorage Trait    │   │ │
│  │  │ memories表 │  │ 索引搜索   │  │ (可插拔存储后端)       │   │ │
│  │  └────────────┘  └────────────┘  └────────────────────────┘   │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │               Orchestration (多智能体编排)                     │ │
│  │  ┌──────────────────┐  ┌──────────────────┐                   │ │
│  │  │ MultiAgent       │  │ MetaGPT Workflow │                   │ │
│  │  │ Orchestrator     │  │ (角色协作)       │                   │ │
│  │  └──────────────────┘  └──────────────────┘                   │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │               Infrastructure 层                                 │ │
│  │  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐      │ │
│  │  │  DB  │ │ Config│ │ MCP  │ │ Git  │ │ FS   │ │Permissions│ │ │
│  │  │Manager│ │Manager│ │Server│ │ 集成  │ │操作  │ │ 权限管理  │ │ │
│  │  └──────┘ └──────┘ └──────┘ └──────┘ └──────┘ └──────┘      │ │
│  └────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.2 架构分层

| 层级 | 技术 | 职责 |
|------|------|------|
| **UI 层** | React 19 + TypeScript, Tailwind CSS, Zustand | 用户界面、状态管理、路由 |
| **API 通信层** | HTTP (Axum) + Tauri IPC | 前后端通信、SSE 流式通信 |
| **核心引擎层** | Rust + Native Engine | LLM API 调用、工具执行循环、会话管理 |
| **服务层** | Rust Modules | 记忆系统、多智能体编排、项目管理、权限控制 |
| **基础设施层** | SQLite, File System, Git, MCP | 数据持久化、配置管理、文件操作、扩展协议 |
| **平台层** | Tauri 2.0 | 跨平台桌面窗口、原生能力、系统托盘 |

### 1.3 进程模型

```
┌─────────────────────┐     Tauri IPC      ┌─────────────────────┐
│  Tauri WebView      │◄────────────────────│  Rust Backend       │
│  (React SPA)        │    invoke/events    │  (Main Process)     │
│  port: 5175 (dev)   │                     │  Bridge: 30080      │
└─────────────────────┘                     └─────────────────────┘
                                                     │
                                                     │ HTTP
                                                     ▼
                                              ┌─────────────────┐
                                              │ LLM API Provider│
                                              │ (Anthropic/OpenAI)
                                              └─────────────────┘
```

### 1.4 当前模块完成度评估

| 模块 | 状态 | 说明 |
|------|------|------|
| ✅ Bridge (HTTP API) | 完成 | Axum 路由、SSE 流式、CORS |
| ✅ Native Engine | 完成 | Anthropic/OpenAI 双客户端、Tool Loop |
| ✅ Chat/Conversation | 完成 | CRUD、流式对话、消息管理 |
| ✅ DB Layer | 完成 | SQLite、WAL、迁移机制 |
| ✅ Permissions | 完成 | 权限模式、审计日志 |
| ✅ MCP Integration | 完成 | MCP 服务器管理、工具注册 |
| ✅ Config Management | 完成 | TOML 配置、Provider 管理 |
| ✅ File System Ops | 完成 | 文件读写、目录操作 |
| ✅ Memory Basic | 基本完成 | SQLite + FTS5、CRUD、标签管理 |
| ⚠️ Memory Vector | 架构就绪 | config.rs 已预留 embedding 字段，storage trait 已定义，但未实现 |
| ⚠️ Multi-Agent | 架构就绪 | MultiAgentOrchestrator 已实现基础编排，前端 UI 需增强 |
| ⚠️ MetaGPT Workflow | 架构就绪 | 角色架构完整，但缺少产品化 |
| ⚠️ Projects | 基础完成 | CRUD 完成，缺少工作区绑定 |
| ❌ Internationalization | 未开始 | locales 文件已创建，但 UI 未接入 |
| ❌ Auto Updater | 未开始 | 基础结构存在，未集成到 UI |
| ❌ E2E Testing | 未开始 | 无测试基础设施 |


## 2. 模块设计 (Module Design)

### 2.1 模块总览

```
src-tauri/src/
├── lib.rs                  # 模块声明、BridgeServer 初始化、Axum 路由注册
├── main.rs                 # Tauri 入口、插件注册、窗口配置
├── bridge/                 # HTTP 桥接层 (Axum REST API)
│   ├── mod.rs              # BridgeServer 结构体、路由注册、ChatRequest/SSE
│   ├── state.rs            # AppState (共享状态)
│   └── memory_handlers_v2.rs  # 记忆 API 处理器 (CRUD、标签管理)
├── native_engine/          # 核心 LLM 引擎
│   ├── mod.rs              # 模块声明
│   ├── engine_core.rs      # NativeEngine + QueryEngine (对话核心)
│   ├── anthropic_client.rs # Anthropic API 客户端
│   ├── openai_client.rs    # OpenAI 兼容 API 客户端
│   ├── provider_manager.rs # Provider 配置管理
│   ├── session_manager.rs  # 会话管理
│   └── tool_loop.rs        # 工具执行循环 (Tool Loop)
├── memory/                 # 记忆系统
│   ├── mod.rs              # 模块声明 (config, error, storage)
│   ├── config.rs           # MemoryConfig (已预留 embedding 配置)
│   ├── error.rs            # MemoryError 类型
│   └── storage/            # 存储层
│       └── mod.rs          # MemoryStorage Trait + MemoryRecord/Query
├── db/                     # 数据库层
│   ├── mod.rs              # DbManager (SQLite 连接管理)
│   ├── schema.rs           # 数据库 schema (DDL)
│   ├── memory_repo.rs      # memories 表 CRUD + FTS5 搜索
│   ├── conversation_repo.rs# conversations 表 CRUD
│   ├── message_repo.rs     # messages 表 CRUD
│   ├── project_repo.rs     # projects 表 CRUD
│   └── migration.rs        # 数据迁移
├── multiagent/             # 多智能体编排
│   └── mod.rs              # MultiAgentOrchestrator + AgentConfig/State/Event
├── orchestration/          # MetaGPT 工作流引擎
│   ├── mod.rs              # metagpt_workflow 入口
│   ├── agent_loop.rs       # 代理循环
│   ├── sandbox.rs          # 沙箱
│   ├── task_store.rs       # 任务存储
│   └── metagpt/            # MetaGPT 角色架构
│       ├── mod.rs, action.rs, config.rs, context_manager.rs
│       ├── environment.rs, knowledge.rs, memory.rs, message.rs
│       ├── role.rs, role_context.rs, serialization.rs
│       ├── actions/ (write_code, write_design, write_prd, etc.)
│       └── roles/ (architect, engineer, pm, qa, reviewer, etc.)
├── commands/               # Tauri IPC 命令
│   └── mod.rs              # get_platform, get_app_path, select_directory
├── streaming/              # SSE 流式处理
│   ├── mod.rs              # StreamManager + StreamEvent
│   └── sse_parser.rs       # SSE 解析器
├── engine/                 # 旧版引擎池 (Python SDK 调用)
│   └── mod.rs              # EnginePool, EngineHandle
├── project/                # 项目管理
│   └── mod.rs              # Project, ProjectManager
├── config/                 # 配置管理
│   └── mod.rs              # AppConfig, ConfigManager
├── permissions/            # 权限控制
│   ├── mod.rs, manager.rs, rules.rs, audit.rs
├── mcp/                    # MCP 协议集成
│   ├── mod.rs, composio.rs, tool_executor.rs
├── tools/                  # 工具系统
│   ├── mod.rs, retry.rs
├── skills/                 # 技能系统
│   ├── mod.rs, engine.rs
├── prompt/                 # Prompt 管理
│   ├── mod.rs, prompts.rs
├── updater/                # 自动更新
│   └── mod.rs              # AutoUpdater + UpdateInfo
├── analytics/              # 分析统计
│   ├── mod.rs
├── clipboard/              # 剪贴板
├── notification/           # 通知
├── terminal/               # 终端 (PTY)
├── process/                # 进程管理
├── watcher/                # 文件监听
├── git/                    # Git 集成
├── github/                 # GitHub 集成
├── fs/                     # 文件系统
├── logger/                 # 日志
├── upload/                 # 文件上传
├── worktree/               # 工作树
├── document/               # 文档管理
├── sandbox/                # 沙箱执行
├── computer_use/           # 计算机使用
├── research/               # 研究模式
├── task/                   # 任务执行
├── ask_user/               # 提问用户
├── ide/                    # IDE 集成
├── slash_commands/         # 斜杠命令
├── cost_tracker/           # 成本追踪
└── user_management/        # 用户管理
```

### 2.2 核心模块详解

#### 2.2.1 Bridge 桥接层

BridgeServer 是整个后端的 HTTP 入口，基于 Axum 0.8 框架：

```
BridgeServer
├── start(port) → 启动 Axum 监听
├── 路由表:
│   ├── GET  /api/system-status        → 系统状态
│   ├── POST /api/chat                 → 对话 (SSE 流式)
│   ├── GET  /api/chat/{id}            → 获取对话
│   ├── DELETE /api/chat/{id}          → 删除对话
│   ├── GET  /api/conversations        → 对话列表
│   ├── GET  /api/memories             → 记忆列表
│   ├── POST /api/memories             → 创建记忆
│   ├── GET  /api/memories/search      → FTS5 搜索
│   ├── DELETE /api/memories/{id}      → 删除记忆
│   ├── GET  /api/memories/stats       → 记忆统计
│   ├── POST /api/memories/backfill    → 回溯生成
│   ├── GET  /api/memories/tags        → 标签列表
│   ├── POST /api/memories/tags/rename → 标签重命名
│   ├── ... 以及其他 30+ 路由
```

**通信方式：**
- **HTTP REST**：常规 CRUD 操作
- **SSE (Server-Sent Events)**：流式对话响应
- **Tauri IPC**：原生能力调用 (invoke/events)

#### 2.2.2 Native Engine 核心引擎

```
NativeEngine
├── ProviderManager → 管理 API Provider (Anthropic/OpenAI)
├── AnthropicClient → Claude API 调用 (支持流式)
├── OpenAIClient    → OpenAI 兼容 API 调用 (支持流式)
├── SessionManager  → 会话状态管理
├── ToolLoopExecutor → 工具执行循环
└── QueryEngine     → 对话查询引擎
```

**核心流程：**
1. 接收 ChatRequest → lookup provider → 调用 LLM API
2. 流式接收 SSE chunks → 解析 Text/Thinking/ToolUse
3. 执行工具调用 → 继续循环直到完成
4. 存储消息和工具调用到数据库

#### 2.2.3 Memory 记忆系统

```
MemorySystem
├── config.rs → MemoryConfig (已预留 embedding 字段)
├── error.rs  → MemoryError (统一的错误类型)
├── storage/
│   └── mod.rs → MemoryStorage Trait (抽象存储后端)
│               ├── MemoryRecord (含 embedding 字段)
│               ├── MemoryQuery (搜索参数)
│               ├── HealthStatus / StorageStats
│               └── 15+ 抽象方法
└── db/memory_repo.rs → SQLite 实现
    ├── insert_memory()     → 带去重的插入
    ├── search_memories()   → FTS5 全文搜索 + LIKE 降级
    ├── list_recent_memories() → 按重要性+时间排序
    ├── get_important_memories() → 高重要性记忆 (>=4)
    ├── build_smart_summary() → 从对话消息提取摘要
    └── 标签管理 API
```

**当前架构的 key 设计亮点：**
- `MemoryStorage` trait 定义了统一的存储抽象，可轻松替换后端
- `MemoryRecord` 已预留 `embedding: Option<Vec<f32>>` 字段
- `MemoryConfig` 已预留 `embedding_model` 和 `embedding_dimension` 配置
- FTS5 搜索有 LIKE 降级机制，保证搜索可用性


### 2.3 前端组件架构

```
src/
├── App.tsx              # 主应用 (路由、布局、全局状态)
├── main.tsx             # React 入口
├── api.ts               # 统一 API 客户端 (~2600 行)
├── adminApi.ts          # 管理后台 API
├── index.css            # Tailwind + 自定义样式
├── constants.ts         # 常量定义
├── vite-env.d.ts        # Vite 类型声明
│
├── components/          # UI 组件 (57 个组件)
│   ├── Sidebar.tsx              # 侧边栏导航
│   ├── MainContent.tsx          # 主对话区域
│   ├── MemoryPanel.tsx          # 记忆面板
│   ├── AgentPanel.tsx           # 多智能体面板
│   ├── AnalyticsPanel.tsx       # 分析面板
│   ├── TerminalPanel.tsx        # 终端面板
│   ├── SettingsPage.tsx         # 设置页面
│   ├── ProjectsPage.tsx         # 项目管理
│   ├── SearchModal.tsx          # 搜索模态框
│   ├── ModelSelector.tsx        # 模型选择器
│   ├── McpSettingsPage.tsx      # MCP 配置
│   ├── SwarmCollaboration.tsx   # 多智能体协作
│   ├── VoiceInput.tsx           # 语音输入
│   ├── DocumentPanel.tsx        # 文档面板
│   ├── ArtifactsPanel.tsx       # 产物面板
│   ├── ResourceViewerPanel.tsx  # 资源查看器
│   ├── ToolCallCard.tsx         # 工具调用卡片
│   ├── MarkdownRenderer.tsx     # Markdown 渲染
│   ├── CodeExecution.tsx        # 代码执行
│   └── admin/                   # 管理后台组件
│       ├── AdminDashboard.tsx
│       ├── AdminUsers.tsx
│       ├── AdminKeyPool.tsx
│       └── ...
│
├── stores/              # Zustand 状态管理
│   ├── index.ts                 # 导出
│   ├── useChatStore.ts          # 对话状态
│   ├── useUIStore.ts            # UI 状态
│   ├── useAuthStore.ts          # 认证状态
│   ├── useProjectStore.ts       # 项目状态
│   ├── useStreamingStore.ts     # 流状态
│   └── useToolStore.ts          # 工具状态
│
├── types/               # TypeScript 类型定义
│   ├── api.ts                   # 核心 API 类型
│   └── declarations.d.ts        # 全局声明
│
├── hooks/               # 自定义 Hooks
│   ├── useAnalytics.ts          # 分析 Hook
│   └── useI18n.ts               # 国际化 Hook
│
├── utils/               # 工具函数
│   ├── tauriAPI.ts              # Tauri IPC 封装
│   ├── apiProxy.ts              # API 代理
│   ├── clipboard.ts             # 剪贴板
│   ├── artifactRenderer.ts      # 产物渲染
│   └── proxyIntegration.ts      # 代理集成
│
├── locales/             # 国际化资源
│   ├── en.json                 # 英文 (21320 行)
│   └── zh.json                 # 中文 (20866 行)
│
├── data/                # 静态数据
└── assets/              # 静态资源
```

### 2.4 数据流架构

```
用户操作
    │
    ▼
React 组件 ──调用──▶ API Client (api.ts)
    │                       │
    │                       ▼
    │                  Axum HTTP Bridge (127.0.0.1:30080)
    │                       │
    │                       ├──▶ NativeEngine → LLM API (流式 SSE)
    │                       │       │
    │                       │       ▼
    │                       │  ToolLoopExecutor → Tools/MCP/Skills
    │                       │       │
    │                       │       ▼
    │                       │  MemoryService → SQLite → 持久化
    │                       │
    │                       ├──▶ DbManager → SQLite CRUD
    │                       │
    │                       └──▶ StreamManager → SSE 推送到前端
    │                                   │
    ◄───────────────────────────────────┘
    │
    ▼
Zustand Store ──▶ UI 更新
```


## 3. 数据模型 (Data Model)

### 3.1 SQLite 数据库 Schema

当前数据库文件：`{data_dir}/claude_desktop.db`

```sql
-- ========= 对话系统 =========

-- 对话表
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,              -- UUID
    title TEXT,                        -- 对话标题
    model TEXT,                        -- 使用的模型
    provider TEXT,                     -- 提供商
    workspace_path TEXT,               -- 工作区路径
    project_id TEXT,                   -- 关联项目ID
    research_mode INTEGER DEFAULT 0,   -- 研究模式
    pinned INTEGER DEFAULT 0,          -- 置顶
    archived INTEGER DEFAULT 0,        -- 归档
    created_at TEXT NOT NULL,          -- 创建时间 (RFC3339)
    updated_at TEXT NOT NULL,          -- 更新时间
    message_count INTEGER DEFAULT 0    -- 消息数量
);

-- 消息表
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,               -- UUID
    conversation_id TEXT NOT NULL,      -- 关联对话ID
    role TEXT NOT NULL,                 -- 'user' | 'assistant' | 'system'
    content TEXT NOT NULL,              -- 消息内容
    thinking TEXT,                      -- 思考过程
    created_at TEXT NOT NULL,
    is_compact_boundary INTEGER DEFAULT 0,  -- 压缩边界标记
    sort_order INTEGER NOT NULL,            -- 排序序号
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- 工具调用表
CREATE TABLE IF NOT EXISTS tool_calls (
    id TEXT PRIMARY KEY,               -- UUID
    message_id TEXT NOT NULL,           -- 关联消息ID
    name TEXT NOT NULL,                 -- 工具名称
    input TEXT,                         -- 输入参数 (JSON)
    output TEXT,                        -- 输出结果
    is_error INTEGER DEFAULT 0,         -- 是否错误
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

-- 附件表
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    file_name TEXT,
    file_type TEXT,
    mime_type TEXT,
    file_size INTEGER,
    source TEXT,
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

-- ========= 项目系统 =========

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    instructions TEXT,                   -- 项目指令
    workspace_path TEXT,
    is_archived INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS project_files (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    file_name TEXT,
    file_path TEXT,
    file_size INTEGER,
    mime_type TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- ========= 记忆系统 (V2) =========

CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    workspace_path TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '',
    memory_type TEXT,           -- 'fact' | 'preference' | 'decision' | 'context'
    importance INTEGER,         -- 1-5
    created_at TEXT NOT NULL
);

-- FTS5 全文索引
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    summary, tags,
    content='memories',
    content_rowid='rowid'
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_memories_workspace_path ON memories(workspace_path);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at);
CREATE INDEX IF NOT EXISTS idx_messages_conversation_id ON messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);
CREATE INDEX IF NOT EXISTS idx_conversations_updated_at ON conversations(updated_at);
CREATE INDEX IF NOT EXISTS idx_conversations_model ON conversations(model);
CREATE INDEX IF NOT EXISTS idx_tool_calls_message_id ON tool_calls(message_id);
CREATE INDEX IF NOT EXISTS idx_attachments_message_id ON attachments(message_id);
CREATE INDEX IF NOT EXISTS idx_project_files_project_id ON project_files(project_id);
```

### 3.2 核心数据结构 (Rust)

```rust
// === Memory System ===

// storage/mod.rs
struct MemoryRecord {
    id: String,
    workspace_path: String,
    conversation_id: String,
    summary: String,
    tags: String,
    memory_type: String,        // 'fact' | 'preference' | 'decision' | 'context'
    importance: i32,            // 1-5
    created_at: String,
    embedding: Option<Vec<f32>>, // 已预留向量字段
}

struct MemoryQuery {
    query: String,                   // 搜索文本
    workspace_path: Option<String>,  // 工作区筛选
    memory_type: Option<String>,     // 类型筛选
    min_importance: Option<i32>,     // 最低重要性
    limit: usize,
    sort_by_importance: bool,
}

// === Multi-Agent ===

enum AgentState { Idle, Planning, Executing, Synthesizing, Completed, Failed }
enum AgentType { Planner, Researcher, Writer, Reviewer, Custom(String) }

struct AgentConfig {
    agent_id: String,
    agent_type: AgentType,
    model_id: Option<String>,
    system_prompt: Option<String>,
    max_tokens: Option<u32>,
    enabled: bool,
}

struct AgentTask {
    task_id: String,
    agent_id: String,
    description: String,
    input: serde_json::Value,
    context: Option<serde_json::Value>,
}

struct AgentResult {
    task_id: String,
    agent_id: String,
    success: bool,
    output: serde_json::Value,
    error: Option<String>,
    duration_ms: u64,
}

// === Native Engine ===

struct ChatRequest {
    conversation_id: String,
    messages: Vec<Value>,
    model: String,
    system_prompt: Option<String>,
    max_tokens: Option<u32>,
    workspace_path: Option<String>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    web_search_enabled: Option<bool>,
    reasoning_effort: Option<String>,
    extended_thinking: bool,
}

struct Project {
    id: String,
    name: String,
    description: Option<String>,
    instructions: Option<String>,
    workspace_path: Option<String>,
    created_at: String,
    updated_at: String,
    is_archived: bool,
    file_count: usize,
}
```

### 3.3 前端 TypeScript 类型

```typescript
// types/api.ts
interface Message {
    id?: string;
    role: 'user' | 'assistant' | 'system';
    content: string | ContentBlock[];
    thinking?: string;
    toolUse?: ToolUseBlock;
    toolResult?: ToolResultBlock;
    isCompactBoundary?: boolean;
    createdAt?: string;
}

interface Conversation {
    id: string;
    title: string | null;
    model: string | null;
    provider: string | null;
    workspace_path: string | null;
    project_id: string | null;
    research_mode: boolean;
    pinned: boolean;
    archived: boolean;
    created_at: string;
    updated_at: string;
    message_count: number;
}

interface ProviderConfig {
    id: string;
    name: string;
    apiKey: string | null;
    baseUrl: string;
    format: 'anthropic' | 'openai';
    models: ModelConfig[];
    enabled: boolean;
}

interface Project {
    id: string;
    name: string;
    description: string | null;
    instructions: string | null;
    workspace_path: string | null;
    is_archived: boolean;
    created_at: string;
    updated_at: string;
}
```


## 4. API 设计 (API Design)

### 4.1 桥接 API 接口 (Axum HTTP)

| 方法 | 路径 | 描述 | 状态 |
|------|------|------|------|
| **对话** | | | |
| POST | `/api/chat` | 发送消息 (SSE 流式响应) | ✅ |
| GET | `/api/chat/{id}` | 获取对话详情 | ✅ |
| DELETE | `/api/chat/{id}` | 删除对话 | ✅ |
| GET | `/api/conversations` | 对话列表 | ✅ |
| POST | `/api/conversations` | 创建对话 | ✅ |
| PUT | `/api/conversations/{id}` | 更新对话 | ✅ |
| DELETE | `/api/conversations/{id}` | 删除对话 | ✅ |
| PATCH | `/api/conversations/{id}/pin` | 置顶/取消置顶 | ✅ |
| PATCH | `/api/conversations/{id}/archive` | 归档/取消归档 | ✅ |
| POST | `/api/conversations/{id}/export` | 导出对话 | ✅ |
| **记忆** | | | |
| GET | `/api/memories` | 记忆列表 | ✅ |
| POST | `/api/memories` | 创建记忆 | ✅ |
| PUT | `/api/memories/{id}` | 更新记忆 | ✅ |
| DELETE | `/api/memories/{id}` | 删除记忆 | ✅ |
| GET | `/api/memories/search` | FTS5 全文搜索 | ✅ |
| GET | `/api/memories/stats` | 记忆统计 | ✅ |
| POST | `/api/memories/backfill` | 回溯生成记忆 | ✅ |
| GET | `/api/memories/tags` | 标签列表 | ✅ |
| POST | `/api/memories/tags/rename` | 标签重命名 | ✅ |
| POST | `/api/memories/tags/merge` | 标签合并 | ✅ |
| POST | `/api/memories/tags/delete` | 标签删除 | ✅ |
| **项目** | | | |
| GET | `/api/projects` | 项目列表 | ✅ |
| POST | `/api/projects` | 创建项目 | ✅ |
| GET | `/api/projects/{id}` | 项目详情 | ✅ |
| PUT | `/api/projects/{id}` | 更新项目 | ✅ |
| DELETE | `/api/projects/{id}` | 删除项目 | ✅ |
| POST | `/api/projects/{id}/sync` | 同步项目文件 | ✅ |
| **多智能体** | | | |
| POST | `/api/research/start` | 启动研究任务 | ✅ |
| GET | `/api/research/{id}/events` | SSE 研究进度事件 | ✅ |
| POST | `/api/swarm/run` | 启动 Swarm 协作 | ✅ |
| GET | `/api/swarm/events` | SSE Swarm 事件 | ✅ |
| **模型与 Provider** | | | |
| GET | `/api/models` | 模型列表 | ✅ |
| GET | `/api/providers` | Provider 列表 | ✅ |
| POST | `/api/providers` | 添加 Provider | ✅ |
| PUT | `/api/providers/{id}` | 更新 Provider | ✅ |
| DELETE | `/api/providers/{id}` | 删除 Provider | ✅ |
| POST | `/api/providers/{id}/test` | 测试 Provider | ✅ |
| **系统** | | | |
| GET | `/api/system-status` | 系统状态 | ✅ |
| GET | `/api/settings` | 获取设置 | ✅ |
| PUT | `/api/settings` | 更新设置 | ✅ |
| GET | `/api/config` | 配置读取 | ✅ |
| PUT | `/api/config` | 配置写入 | ✅ |
| **MCP** | | | |
| GET | `/api/mcp/servers` | MCP 服务器列表 | ✅ |
| POST | `/api/mcp/servers` | 添加 MCP 服务器 | ✅ |
| DELETE | `/api/mcp/servers/{id}` | 删除 MCP 服务器 | ✅ |
| POST | `/api/mcp/servers/{id}/restart` | 重启 MCP 服务器 | ✅ |
| GET | `/api/mcp/tools` | MCP 工具列表 | ✅ |
| **技能** | | | |
| GET | `/api/skills` | 技能列表 | ✅ |
| POST | `/api/skills/{name}/run` | 执行技能 | ✅ |
| **认证** | | | |
| POST | `/api/auth/login` | 登录 | ✅ |
| POST | `/api/auth/register` | 注册 | ✅ |
| POST | `/api/auth/send-code` | 发送验证码 | ✅ |
| GET | `/api/admin/*` | 管理后台 API | ✅ |

### 4.2 流式通信协议 (SSE)

对话流式响应使用 Server-Sent Events 协议：

```
event: message_start
data: {"type":"message_start","model":"claude-sonnet-4-20250514"}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello! "}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"How can I help?"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_stop
data: {"type":"message_stop","usage":{"input_tokens":10,"output_tokens":15}}
```

### 4.3 Tauri IPC 命令

| 命令 | 描述 |
|------|------|
| `get_platform` | 获取平台信息 |
| `get_app_path` | 获取应用数据路径 |
| `select_directory` | 选择目录对话框 |
| `tauriAPI.init()` | 初始化 Tauri API |
| `tauriAPI.writeFile()` | 写入文件 |
| `tauriAPI.readFile()` | 读取文件 |

### 4.4 向量记忆 API 扩展设计 (待实现)

```typescript
// 向量搜索 API (新增)
POST /api/memories/vector-search
Body: {
  query: string;          // 搜索文本
  workspace_path?: string;
  limit?: number;         // 默认 10
  min_score?: number;     // 最小相似度 0.0-1.0
  hybrid?: boolean;       // 是否混合 FTS5 + 向量
}
Response: {
  memories: MemoryRecord[];
  scores: number[];       // 相似度分数
  total: number;
}

// 关联记忆推荐 (新增)
GET /api/memories/{id}/related
Query: { limit?: number }
Response: {
  memories: MemoryRecord[];
  scores: number[];
}

// Embedding 状态 (新增)
GET /api/memories/vector-status
Response: {
  total_embedding: number;
  total_memories: number;
  embedding_model: string;
  embedding_dimension: number;
  last_indexed: string;
}
```


## 5. 技术栈 (Tech Stack)

### 5.1 当前技术栈

| 层级 | 技术 | 版本 | 用途 |
|------|------|------|------|
| **桌面框架** | Tauri 2.x | 2.11+ | 跨平台桌面应用容器 |
| **后端语言** | Rust | 2021 edition | 高性能、安全的系统编程 |
| **异步运行时** | Tokio | 1.x | 异步 I/O、任务调度 |
| **HTTP 框架** | Axum | 0.8 | REST API + SSE 流式通信 |
| **HTTP 客户端** | reqwest | 0.12 | LLM API 调用 |
| **数据库** | rusqlite (SQLite) | 0.31 | 本地持久化 (bundled) |
| **序列化** | serde / serde_json | 1.x | JSON 序列化 |
| **UUID** | uuid | 1.x | 唯一标识符 (v4) |
| **时间处理** | chrono | 0.4 | 时间戳、日期格式化 |
| **CORS** | tower-http | 0.6 | 跨域支持 |
| **异步 Trait** | async-trait | 0.1 | 异步 trait 方法 |
| **错误处理** | anyhow / thiserror | 1.x | 错误链和自定义错误 |
| **日志** | tracing / tracing-subscriber | 0.1 | 结构化日志 |
| **流处理** | tokio-stream / async-stream / futures | - | 异步流处理 |
| **文件监控** | notify | 6.x | 文件系统变更通知 |
| **剪贴板** | arboard / tauri-plugin-clipboard | - | 系统剪贴板 |
| **PTY** | - | - | 伪终端 (终端模拟) |
| **前端框架** | React | 19.x | UI 组件库 |
| **前端语言** | TypeScript | 5.7+ | 类型安全 |
| **构建工具** | Vite | 6.x | 前端构建 |
| **CSS 框架** | Tailwind CSS | 3.x | 原子化样式 |
| **状态管理** | Zustand | 5.x | 轻量级状态管理 |
| **路由** | react-router-dom | 6.x | 前端路由 |
| **图表** | recharts | 3.x | 数据可视化 |
| **图标** | lucide-react | 0.563+ | 图标库 |
| **Markdown** | react-markdown + remark-gfm | - | Markdown 渲染 |
| **代码高亮** | highlight.js / react-syntax-highlighter | - | 代码语法高亮 |
| **数学公式** | KaTeX | 0.16+ | 数学公式渲染 |
| **流程图** | mermaid | 11.x | 图表渲染 |
| **终端** | xterm.js | 5.x | 终端模拟 |
| **Tauri 插件** | shell, dialog, fs, http, process, clipboard, notification | 2.x | 原生能力桥接 |

### 5.2 关键 Rust 依赖分析

```toml
# Cargo.toml 核心依赖
[dependencies]
tauri = { version = "2", features = ["tray-icon", "devtools"] }
tauri-plugin-shell = "2"
tauri-plugin-dialog = "2"
tauri-plugin-fs = "2"
tauri-plugin-http = "2"
tauri-plugin-process = "2"
tauri-plugin-clipboard-manager = "2"
tauri-plugin-notification = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "stream", "rustls-tls", "blocking", "system-proxy"] }
axum = { version = "0.8", features = ["macros", "multipart"] }
tower-http = { version = "0.6", features = ["cors"] }
rusqlite = { version = "0.31", features = ["bundled"] }
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
async-trait = "0.1"
dirs = "5"
```

### 5.3 新增依赖建议 (按迭代优先级)

| 依赖 | 版本 | 用途 | 迭代 |
|------|------|------|------|
| `fastembed` | 3.x | 本地 Embedding 生成 (ONNX 运行时) | Phase 1 |
| `arrow2` / `ndarray` | - | 向量计算、数组操作 | Phase 1 |
| `hnswlib` / `pgrus` | - | 近似最近邻 (ANN) 向量索引 | Phase 1 |
| `serde` (已有) | - | - | - |
| `i18n-embed` / `fluent` | - | 国际化、本地化 | Phase 2 |
| `tokio-tungstenite` | - | WebSocket 支持 (可选) | Phase 3 |

### 5.4 技术约束

| 约束 | 说明 |
|------|------|
| **离线优先** | 所有核心功能应可在无网络下运行 (除 LLM API 调用) |
| **本地数据** | 所有数据存储在本地 SQLite，不依赖外部数据库 |
| **跨平台** | Windows/macOS/Linux 三平台支持 |
| **安装包大小** | 目标 ~15MB (当前 Tauri 优势) |
| **内存占用** | 目标比 Electron 方案降低 80% |
| **向后兼容** | 数据库 schema 变更须通过迁移机制，不破坏现有数据 |


## 6. 文件结构 (File Structure)

### 6.1 完整项目目录树

```
claude-code-rust/
├── 📁 .claude/                    # Claude 配置文件
├── 📁 .codegraph/                 # 代码图谱
├── 📁 .github/                    # GitHub Actions CI/CD
├── 📁 .trae/                      # Trae IDE 配置
├── 📁 api-proxy/                  # API 代理服务 (Electron 兼容)
├── 📁 dist/                       # 前端构建产物 (Vite build)
├── 📁 docs/                       # 文档
│   ├── 📄 prd-memory-vector-upgrade.md  # 记忆系统升级 PRD
│   ├── 📄 optimization-plan.md         # 优化计划
│   ├── 📄 technical-design.md          # 本文档
│   └── 📁 superpowers/                 # 超级功能文档
├── 📁 logs/                       # 运行日志
├── 📁 node_modules/               # npm 依赖
├── 📁 outputs/                    # 输出文件
├── 📁 public/                     # 静态资源
├── 📁 scripts/                    # 构建/开发脚本
├── 📁 src/                        # 前端源码
│   ├── 📄 App.tsx                 # 主应用组件
│   ├── 📄 main.tsx                # React 入口
│   ├── 📄 api.ts                  # API 客户端 (2600+ 行)
│   ├── 📄 adminApi.ts             # 管理后台 API
│   ├── 📄 index.css               # 全局样式
│   ├── 📄 constants.ts            # 常量
│   ├── 📄 vite-env.d.ts           # Vite 环境类型
│   ├── 📁 assets/                 # 静态资源
│   ├── 📁 components/             # UI 组件 (57 个)
│   │   ├── 📄 Sidebar.tsx
│   │   ├── 📄 MainContent.tsx
│   │   ├── 📄 MemoryPanel.tsx
│   │   ├── 📄 AgentPanel.tsx
│   │   ├── 📄 AnalyticsPanel.tsx
│   │   ├── 📄 TerminalPanel.tsx
│   │   ├── 📄 SettingsPage.tsx
│   │   ├── 📄 ProjectsPage.tsx
│   │   ├── 📄 SearchModal.tsx
│   │   ├── 📄 SwarmCollaboration.tsx
│   │   ├── 📄 ... (更多组件)
│   │   └── 📁 admin/              # 管理后台组件
│   ├── 📁 stores/                 # Zustand Store
│   │   ├── 📄 useChatStore.ts
│   │   ├── 📄 useUIStore.ts
│   │   ├── 📄 useAuthStore.ts
│   │   ├── 📄 useProjectStore.ts
│   │   ├── 📄 useStreamingStore.ts
│   │   └── 📄 useToolStore.ts
│   ├── 📁 types/                  # TypeScript 类型
│   │   └── 📄 api.ts
│   ├── 📁 hooks/                  # 自定义 Hooks
│   │   ├── 📄 useAnalytics.ts
│   │   └── 📄 useI18n.ts
│   ├── 📁 utils/                  # 工具函数
│   │   ├── 📄 tauriAPI.ts
│   │   ├── 📄 apiProxy.ts
│   │   ├── 📄 artifactRenderer.ts
│   │   ├── 📄 clipboard.ts
│   │   └── 📄 proxyIntegration.ts
│   ├── 📁 locales/                # 国际化
│   │   ├── 📄 en.json             # 英文 (21320 行)
│   │   └── 📄 zh.json             # 中文 (20866 行)
│   ├── 📁 data/                   # 静态数据
│   └── 📁 assets/                 # 图片等资源
│
├── 📁 src-tauri/                  # Rust 后端源码
│   ├── 📁 .cargo/                 # Cargo 配置
│   ├── 📁 capabilities/           # Tauri 能力配置
│   ├── 📁 config/                 # 运行时配置 (TOML)
│   ├── 📁 data/                   # 数据目录
│   ├── 📁 gen/                    # 代码生成
│   ├── 📁 icons/                  # 应用图标
│   ├── 📁 src/                    # Rust 源代码
│   │   ├── 📄 lib.rs              # 模块声明 + BridgeServer
│   │   ├── 📄 main.rs             # Tauri 入口
│   │   ├── 📁 bridge/             # HTTP 桥接层
│   │   │   ├── 📄 mod.rs          # BridgeServer, 路由, ChatRequest
│   │   │   ├── 📄 state.rs        # AppState
│   │   │   └── 📄 memory_handlers_v2.rs  # 记忆 API
│   │   ├── 📁 native_engine/      # LLM 引擎
│   │   │   ├── 📄 mod.rs
│   │   │   ├── 📄 engine_core.rs  # NativeEngine
│   │   │   ├── 📄 anthropic_client.rs
│   │   │   ├── 📄 openai_client.rs
│   │   │   ├── 📄 provider_manager.rs
│   │   │   ├── 📄 session_manager.rs
│   │   │   └── 📄 tool_loop.rs
│   │   ├── 📁 memory/             # 记忆系统
│   │   │   ├── 📄 mod.rs
│   │   │   ├── 📄 config.rs
│   │   │   ├── 📄 error.rs
│   │   │   └── 📁 storage/
│   │   │       └── 📄 mod.rs      # MemoryStorage Trait
│   │   ├── 📁 db/                 # 数据库
│   │   │   ├── 📄 mod.rs
│   │   │   ├── 📄 schema.rs
│   │   │   ├── 📄 memory_repo.rs
│   │   │   ├── 📄 conversation_repo.rs
│   │   │   ├── 📄 message_repo.rs
│   │   │   ├── 📄 project_repo.rs
│   │   │   └── 📄 migration.rs
│   │   ├── 📁 multiagent/         # 多智能体
│   │   │   └── 📄 mod.rs
│   │   ├── 📁 orchestration/      # MetaGPT 编排
│   │   │   ├── 📄 mod.rs
│   │   │   ├── 📄 agent_loop.rs
│   │   │   ├── 📄 sandbox.rs
│   │   │   ├── 📄 task_store.rs
│   │   │   └── 📁 metagpt/
│   │   │       ├── 📄 mod.rs, action.rs, config.rs
│   │   │       ├── 📄 context_manager.rs, environment.rs
│   │   │       ├── 📄 knowledge.rs, memory.rs, message.rs
│   │   │       ├── 📄 role.rs, role_context.rs
│   │   │       ├── 📄 serialization.rs
│   │   │       ├── 📄 prompt_templates.rs, review_verdict.rs
│   │   │       ├── 📄 cost_calculator.rs, token_tracker.rs
│   │   │       ├── 📄 human_role.rs, sandbox.rs, tool_loop.rs
│   │   │       ├── 📁 actions/    # 12 个动作实现
│   │   │       └── 📁 roles/      # 14 个角色实现
│   │   ├── 📁 streaming/          # SSE 流
│   │   │   ├── 📄 mod.rs
│   │   │   └── 📄 sse_parser.rs
│   │   ├── 📁 engine/             # 旧版引擎池
│   │   ├── 📁 commands/           # Tauri 命令
│   │   ├── 📁 project/            # 项目管理
│   │   ├── 📁 config/             # 配置管理
│   │   ├── 📁 permissions/        # 权限控制
│   │   │   ├── 📄 mod.rs, manager.rs, rules.rs, audit.rs
│   │   ├── 📁 mcp/                # MCP 协议
│   │   │   ├── 📄 mod.rs, composio.rs, tool_executor.rs
│   │   ├── 📁 tools/              # 工具系统
│   │   ├── 📁 skills/             # 技能系统
│   │   ├── 📁 prompt/             # Prompt 管理
│   │   ├── 📁 updater/            # 自动更新
│   │   ├── 📁 analytics/          # 分析统计
│   │   ├── 📁 ... (其他基础设施模块)
│   │   └── 📁 user_management/    # 用户管理
│   │
│   ├── 📄 Cargo.toml              # Rust 依赖配置
│   ├── 📄 Cargo.lock              # 依赖锁定
│   ├── 📄 tauri.conf.json         # Tauri 配置
│   └── 📄 build.rs                # 构建脚本
│
├── 📄 package.json                # 前端依赖
├── 📄 package-lock.json           # 锁定文件
├── 📄 vite.config.ts              # Vite 配置
├── 📄 tsconfig.json               # TypeScript 配置
├── 📄 tsconfig.node.json          # Node TypeScript 配置
├── 📄 tailwind.config.js          # Tailwind 配置
├── 📄 postcss.config.js           # PostCSS 配置
├── 📄 index.html                  # HTML 入口
├── 📄 README.md                   # 项目说明
├── 📄 LICENSE.txt                 # 许可证
└── 📄 .gitignore                  # Git 忽略
```

### 6.2 建议的新增文件结构 (Phase 1-4)

```
# Phase 1: 向量记忆系统
src-tauri/src/memory/
├── storage/
│   └── mod.rs              # (已有) 扩展 MemoryStorage
├── embedding.rs            # [新增] Embedding 生成器
├── vector_index.rs         # [新增] 向量索引 (HNSW)
└── hybrid_search.rs        # [新增] 混合搜索 (FTS5 + 向量)

# Phase 2: 国际化
src/
├── hooks/
│   └── useI18n.ts          # (已有) 增强国际化 Hook
└── i18n/                   # [新增] 国际化工具
    ├── index.ts
    └── utils.ts

# Phase 3: 多智能体可视化
src/components/
├── AgentPipeline.tsx       # [新增] 智能体流水线可视化
└── AgentNode.tsx           # [新增] 智能体节点组件

# Phase 4: 自动更新完善
src-tauri/src/updater/
├── mod.rs                  # (已有) 增强更新 UI 集成
└── update_notifier.rs      # [新增] 更新通知组件
```


## 7. 迭代路线图与架构演进

### 7.1 Phase 1 — 向量记忆升级 (4 周)

```
目标: 将关键词记忆系统升级为向量语义记忆系统

架构变更:
┌─────────────────────────────────────────────────────┐
│                    记忆系统 (升级后)                    │
├─────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐                  │
│  │ MemoryConfig │  │ MemoryRecord │                  │
│  │ (已有, 增强) │  │ (已有, 扩展)  │                  │
│  └──────┬───────┘  └──────┬───────┘                  │
│         │                  │                          │
│  ┌──────▼──────────────────▼───────┐                  │
│  │       MemoryStorage Trait       │                  │
│  │  (已有, 新增 vector_search)      │                  │
│  └──────┬──────────────────┬───────┘                  │
│         │                  │                          │
│  ┌──────▼──────┐  ┌───────▼────────┐                 │
│  │ SQLiteBackend│  │ VectorIndex   │ [新增]           │
│  │ (FTS5 + 向量)│  │ (HNSW/IVF)   │                  │
│  │             │  │ + cosine_sim  │                  │
│  └─────────────┘  └───────┬────────┘                  │
│                           │                           │
│                    ┌──────▼──────┐                    │
│                    │ Embedding   │ [新增]              │
│                    │ Generator   │                    │
│                    │ (fastembed  │                    │
│                    │  / ONNX)    │                    │
│                    └─────────────┘                    │
└─────────────────────────────────────────────────────┘

关键任务:
1. 实现 EmbeddingGenerator: 基于 fastembed 的本地向量生成
2. 实现 VectorIndex: HNSW 近似最近邻搜索
3. 扩展 MemoryStorage trait: 添加 vector_search 方法
4. 实现混合搜索: FTS5 关键词 + 向量语义融合
5. 新增记忆关联 API: /api/memories/{id}/related
6. 更新 MemoryPanel: 支持语义搜索结果展示
```

### 7.2 Phase 2 — 国际化与自动更新 (2 周)

```
目标: 完善国际化框架和自动更新机制

架构变更:
- useI18n Hook: 从 en/zh JSON 文件加载翻译，支持动态切换
- AutoUpdater: 集成到 UI 设置页面，显示更新进度
- 所有 UI 组件接入 i18n 上下文
```

### 7.3 Phase 3 — 多智能体可视化 (2 周)

```
目标: 增强多智能体前端可视化

架构变更:
- AgentPipeline 组件: 实时显示智能体执行流水线
- AgentNode 组件: 显示单个智能体状态、输入输出
- SwarmCollaboration 增强: 支持拖拽式智能体配置
```

### 7.4 Phase 4 — 项目工作区绑定与 E2E 测试 (2 周)

```
目标: 项目与工作区深度集成 + 测试基础设施

架构变更:
- 项目绑定工作区: 项目关联特定目录，记忆/对话作用域
- E2E 测试: Playwright + Tauri 测试框架
```

## 8. 关键架构决策记录 (ADR)

| 决策 | 方案 | 理由 |
|------|------|------|
| 向量索引方案 | HNSW (内存) + SQLite BLOB 持久化 | 避免引入外部向量数据库，保持离线优先 |
| Embedding 方案 | 本地 ONNX (fastembed) | 数据隐私、离线可用、低延迟 (vs. 远程 API) |
| 混合搜索策略 | RRF (Reciprocal Rank Fusion) | 简单有效的融合算法，无需训练 |
| 国际化方案 | JSON 资源文件 + useI18n Hook | 轻量级，无需额外构建步骤 |
| 测试框架 | Playwright + cargo test | 统一的前端+后端测试方案 |
| 自动更新 | Tauri updater plugin | 原生集成、签名验证、增量更新 |

## 9. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| ONNX 模型体积大 | 增加安装包大小 | 延迟加载、可选下载、量化模型 |
| 向量搜索性能 | 大量记忆时变慢 | HNSW 索引、分页、异步生成 |
| 本地 Embedding 质量 | 语义理解不足 | 提供切换远程 Embedding API 选项 |
| 国际化覆盖不全 | 部分 UI 未翻译 | 增量推进、社区贡献 |
| 更新打断用户 | 体验下降 | 静默下载 + 用户确认安装 |
```

