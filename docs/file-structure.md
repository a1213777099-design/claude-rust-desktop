# 文件结构 (File Structure)

```
claude-code-rust/
├── src/                          # 前端源代码 (React + TypeScript)
│   ├── main.tsx                  # React 入口
│   ├── App.tsx                   # 主应用组件 (路由、布局)
│   ├── api.ts                    # 统一 HTTP API 客户端 (~95000 字符)
│   ├── adminApi.ts               # 管理后台 API
│   ├── constants.ts              # 全局常量
│   ├── index.css                 # Tailwind + 自定义样式
│   ├── vite-env.d.ts             # Vite 类型声明
│   │
│   ├── components/               # UI 组件 (57+ 组件)
│   │   ├── MainContent.tsx       # 主对话区域 (~248000 字符)
│   │   ├── Sidebar.tsx           # 侧边栏导航
│   │   ├── SettingsPage.tsx      # 设置页面
│   │   ├── AgentPanel.tsx        # 多智能体面板
│   │   ├── AnalyticsPanel.tsx    # 分析面板
│   │   ├── TerminalPanel.tsx     # 终端面板
│   │   ├── SwarmCollaboration.tsx # MetaGPT 协作
│   │   ├── ChatsPage.tsx         # 聊天列表页
│   │   ├── ProjectsPage.tsx      # 项目页面
│   │   ├── CustomizePage.tsx     # 个性化页面
│   │   ├── DesignPage.tsx        # 设计页面
│   │   ├── MemoryPanel.tsx       # 记忆面板
│   │   ├── DocumentPanel.tsx     # 文档面板
│   │   ├── ArtifactsPanel.tsx    # 产物面板
│   │   ├── ArtifactsPage.tsx     # 产物页面
│   │   ├── MarkdownRenderer.tsx  # Markdown 渲染器
│   │   ├── ToolCallCard.tsx      # 工具调用卡片
│   │   ├── ModelSelector.tsx     # 模型选择器
│   │   ├── McpManagementPanel.tsx # MCP 管理面板
│   │   ├── DirectoryModal.tsx    # 目录选择模态框
│   │   ├── Auth.tsx              # 认证组件
│   │   ├── Onboarding.tsx        # 新手引导
│   │   ├── UpgradePlan.tsx       # 升级计划
│   │   ├── GitBashRequiredModal.tsx # Git Bash 提示
│   │   ├── ErrorBoundary.tsx     # 错误边界
│   │   ├── DraggableDivider.tsx  # 可拖拽分隔线
│   │   ├── ClaudeLogo.tsx        # Claude 标志
│   │   ├── Icons.tsx             # 图标组件
│   │   ├── CostTracker.tsx       # 成本追踪
│   │   ├── EmbeddedBrowser.tsx   # 嵌入式浏览器
│   │   ├── VoiceInput.tsx        # 语音输入
│   │   ├── DocumentCard.tsx      # 文档卡片
│   │   ├── DocxPreview.tsx       # DOCX 预览
│   │   ├── FileUploadPreview.tsx # 文件上传预览
│   │   ├── CodeExecution.tsx     # 代码执行
│   │   ├── CodeLoginModal.tsx    # 代码登录
│   │   ├── DocumentCreationProcess.tsx # 文档创建流程
│   │   ├── AddFromGithubModal.tsx # 从 GitHub 添加
│   │   └── admin/                # 管理后台
│   │       ├── AdminLayout.tsx
│   │       ├── AdminDashboard.tsx
│   │       ├── AdminKeyPool.tsx
│   │       ├── AdminUsers.tsx
│   │       ├── AdminPlans.tsx
│   │       ├── AdminRedemption.tsx
│   │       ├── AdminModels.tsx
│   │       └── AdminAnnouncements.tsx
│   │
│   ├── stores/                   # Zustand 状态管理
│   │   ├── index.ts
│   │   ├── useChatStore.ts       # 对话状态
│   │   ├── useUIStore.ts         # UI 状态
│   │   ├── useAuthStore.ts       # 认证状态
│   │   ├── useProjectStore.ts    # 项目状态
│   │   ├── useStreamingStore.ts  # 流状态
│   │   └── useToolStore.ts       # 工具状态
│   │
│   ├── types/                    # TypeScript 类型定义
│   │   └── api.ts                # API 类型
│   │
│   ├── hooks/                    # 自定义 Hooks
│   │   ├── useAnalytics.ts
│   │   └── useI18n.ts
│   │
│   ├── utils/                    # 工具函数
│   │   ├── tauriAPI.ts           # Tauri IPC 封装
│   │   ├── apiProxy.ts           # API 代理
│   │   ├── clipboard.ts          # 剪贴板
│   │   ├── artifactRenderer.ts   # 产物渲染
│   │   └── proxyIntegration.ts   # 代理集成
│   │
│   ├── locales/                  # 国际化资源
│   │   ├── en.json               # 英文 (21320 行)
│   │   └── zh.json               # 中文 (20866 行)
│   │
│   ├── data/                     # 静态数据
│   ├── assets/                   # 静态资源
│   └── pyodideRunner.ts          # Pyodide Python 运行器
│
├── src-tauri/                    # 后端源代码 (Rust)
│   ├── Cargo.toml                # Rust 包配置
│   ├── Cargo.lock                # 依赖锁定
│   ├── tauri.conf.json           # Tauri 配置
│   ├── build.rs                  # 构建脚本
│   │
│   └── src/                      # Rust 源代码
│       ├── main.rs               # 入口 (Tauri Builder + Plugin 初始化)
│       ├── lib.rs                # Crate 根 (pub mod 所有模块)
│       │
│       ├── bridge/               # HTTP 桥接层
│       │   ├── mod.rs            # BridgeServer + Axum 路由 (~2600 行)
│       │   ├── state.rs          # AppState 共享状态
│       │   └── memory_handlers_v2.rs # 记忆 CRUD handlers
│       │
│       ├── native_engine/        # 核心推理引擎
│       │   ├── mod.rs            # 模块导出
│       │   ├── engine_core.rs    # NativeEngine + QueryEngine
│       │   ├── anthropic_client.rs # Claude Messages API 客户端
│       │   ├── openai_client.rs  # OpenAI Chat Completions 客户端
│       │   ├── provider_manager.rs # Provider 管理/路由
│       │   ├── session_manager.rs # 会话管理
│       │   └── tool_loop.rs      # 工具执行循环
│       │
│       ├── memory/               # 记忆系统
│       │   ├── mod.rs
│       │   ├── config.rs         # 记忆配置
│       │   ├── embedding.rs      # 向量嵌入引擎 (fastembed/API/TF-IDF)
│       │   ├── vector_index.rs   # 向量索引
│       │   ├── clustering.rs     # DBSCAN 聚类
│       │   ├── compression.rs    # 记忆压缩
│       │   ├── error.rs          # 错误类型
│       │   └── storage/
│       │       └── mod.rs        # MemoryStorage trait
│       │
│       ├── orchestration/        # 多智能体编排
│       │   ├── mod.rs            # metagpt_workflow 入口
│       │   ├── agent_loop.rs     # 代理循环
│       │   ├── sandbox.rs        # 沙箱
│       │   ├── task_store.rs     # 任务存储
│       │   └── metagpt/          # MetaGPT 框架
│       │       ├── mod.rs, action.rs, config.rs
│       │       ├── environment.rs, message.rs, role.rs
│       │       ├── memory.rs, knowledge.rs, sandbox.rs
│       │       ├── context_manager.rs, serialization.rs
│       │       ├── role_context.rs, human_role.rs
│       │       ├── token_tracker.rs, cost_calculator.rs
│       │       ├── review_verdict.rs, prompt_templates.rs
│       │       ├── persistence.rs, tool_loop.rs
│       │       ├── actions/       # 12 个 Action
│       │       │   ├── write_prd.rs, write_design.rs
│       │       │   ├── write_code.rs, write_review.rs
│       │       │   ├── write_test.rs, debug_error.rs
│       │       │   ├── conduct_research.rs, search_and_summarize.rs
│       │       │   ├── collect_links.rs, invoice_ocr.rs
│       │       │   ├── run_code.rs, write_tutorial.rs
│       │       │   └── write_teaching_plan.rs
│       │       └── roles/         # 8 个角色
│       │           ├── product_manager.rs, architect.rs
│       │           ├── engineer.rs, reviewer.rs
│       │           ├── qa_engineer.rs, devops.rs
│       │           ├── project_manager.rs, researcher.rs
│       │           ├── searcher.rs, assistant.rs
│       │           ├── teacher.rs, tutorial_assistant.rs
│       │           ├── customer_service.rs
│       │           └── invoice_ocr_assistant.rs
│       │
│       ├── db/                   # 数据库层
│       │   ├── mod.rs            # DbManager
│       │   ├── schema.rs         # DDL 定义
│       │   ├── conversation_repo.rs
│       │   ├── message_repo.rs
│       │   ├── memory_repo.rs
│       │   ├── project_repo.rs
│       │   ├── swarm_repo.rs
│       │   └── migration.rs      # 数据迁移 (V2/V3)
│       │
│       ├── mcp/                  # MCP 协议集成
│       │   ├── mod.rs            # McpServerManager
│       │   ├── tool_executor.rs  # McpToolRegistry
│       │   └── composio.rs       # Composio 集成
│       │
│       ├── commands/             # Tauri IPC 命令
│       │   └── mod.rs            # 30+ 命令
│       │
│       ├── streaming/            # SSE 流处理
│       │   ├── mod.rs            # StreamManager
│       │   └── sse_parser.rs     # SSE 解析器
│       │
│       ├── tools/                # 工具系统
│       │   ├── mod.rs            # 工具定义 + execute_tool()
│       │   └── retry.rs          # 重试逻辑
│       │
│       ├── permissions/          # 权限系统
│       │   ├── mod.rs            # PermissionResult/ToolPermission
│       │   ├── manager.rs        # PermissionManager (4种模式)
│       │   ├── rules.rs          # 权限规则
│       │   └── audit.rs          # 审计日志
│       │
│       ├── skills/               # 技能系统
│       │   ├── mod.rs            # Skill/SkillSource/SkillFile
│       │   └── engine.rs         # SkillExecutionEngine
│       │
│       ├── engine/               # 旧版引擎池
│       │   └── mod.rs            # EnginePool (Python SDK 调用)
│       │
│       ├── config/               # 配置管理
│       │   └── mod.rs            # AppConfig/ConfigManager
│       │
│       ├── multiagent/           # 多智能体通用编排
│       │   └── mod.rs            # MultiAgentOrchestrator
│       │
│       ├── project/              # 项目管理
│       │   └── mod.rs            # Project/ProjectManager
│       │
│       ├── document/             # 文档管理
│       │   └── mod.rs            # DocumentManager
│       │
│       ├── prompt/               # Prompt 管理
│       │   ├── mod.rs
│       │   └── prompts.rs
│       │
│       ├── analytics/            # 分析统计
│       │   └── mod.rs
│       │
│       ├── updater/              # 自动更新
│       │   └── mod.rs
│       │
│       ├── research/             # 研究模式
│       │   └── mod.rs
│       │
│       ├── task/                 # 任务执行
│       │   └── mod.rs
│       │
│       ├── notification/         # 通知
│       ├── clipboard/            # 剪贴板
│       ├── terminal/             # PTY 终端
│       ├── process/              # 进程管理
│       ├── watcher/              # 文件监听
│       ├── git/                  # Git 集成
│       ├── github/               # GitHub API
│       ├── fs/                   # 文件系统
│       ├── logger/               # 日志
│       ├── upload/               # 文件上传
│       ├── worktree/             # 工作树
│       ├── sandbox/              # 沙箱执行
│       ├── computer_use/         # 计算机使用
│       ├── ide/                  # IDE 集成
│       ├── ask_user/             # 提问用户
│       ├── slash_commands/       # 斜杠命令
│       ├── cost_tracker/         # 成本追踪
│       └── user_management/      # 用户管理
│
├── api-proxy/                    # API 代理服务器 (Node.js)
│   ├── package.json
│   └── server.js                 # Express 代理 (~48460 字符)
│
├── docs/                         # 文档
│   ├── architecture-overview.md
│   ├── module-design.md
│   ├── data-model.md
│   ├── api-design.md
│   ├── tech-stack.md
│   ├── file-structure.md
│   ├── technical-design.md       # 原有详细设计文档
│   ├── prd-architecture-gateway.md
│   ├── prd-memory-vector-upgrade.md
│   ├── optimization-plan.md
│   └── superpowers/              # 能力文档
│
├── scripts/                      # 构建/修复脚本
│   ├── *.cjs                     # 多项修复脚本
│   └── patch_orm.py
│
├── config/                       # 运行时配置
│   └── orchestration.toml
│
├── data/                         # 运行时数据
│   ├── analytics/                # 分析事件日志
│   └── ...
│
├── gen/                          # 生成的文件
│   └── schemas/                  # JSON Schema
│
├── icons/                        # 应用图标
│
├── public/                       # 前端静态资源
│
├── dist/                         # 构建输出
│
├── package.json                  # 前端包配置
├── vite.config.ts                # Vite 构建配置
├── tsconfig.json                 # TypeScript 配置
├── tailwind.config.js            # Tailwind 配置
├── postcss.config.js             # PostCSS 配置
└── index.html                    # HTML 入口
```
