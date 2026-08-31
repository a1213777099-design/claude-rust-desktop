# 技术栈 (Tech Stack)

## 1. 后端技术栈

| 分类 | 技术 | 版本 | 用途 |
|------|------|------|------|
| **语言** | Rust | edition 2021 | 后端主语言 |
| **桌面框架** | Tauri | 2.x | 原生桌面壳 |
| **HTTP 服务器** | Axum | 0.8 | REST API + SSE |
| **异步运行时** | Tokio | 1.x (full) | 异步任务调度 |
| **ORM/数据库** | rusqlite | 0.31 (bundled) | SQLite 数据库 |
| **序列化** | serde / serde_json | 1.x | JSON 序列化 |
| **HTTP 客户端** | reqwest | 0.12 | LLM API 调用 |
| **CORS** | tower-http | 0.6 | 跨域支持 |
| **流式处理** | tokio-stream / async-stream | — | SSE 流处理 |
| **UUID** | uuid | 1.x (v4) | 主键生成 |
| **时间处理** | chrono | 0.4 (serde) | 时间戳 |
| **日志** | tracing / tracing-subscriber | 0.1 / 0.3 | 结构化日志 |
| **向量嵌入** | fastembed | 4 | ONNX 本地嵌入 |
| **聚类** | linfa-clustering | 0.7 | DBSCAN 聚类 |
| **数值计算** | ndarray | 0.16 | 向量运算 |
| **配置** | toml | 0.8 | 配置文件解析 |
| **加密** | aes-gcm | 0.10 | 敏感数据加密 |
| **键盘模拟** | enigo | 0.2 | 计算机使用功能 |
| **文件监听** | notify | 6 | 工作区文件监听 |

### Tauri Plugins

| 插件 | 用途 |
|------|------|
| tauri-plugin-shell | shell 命令执行 |
| tauri-plugin-dialog | 原生对话框 |
| tauri-plugin-fs | 文件系统访问 |
| tauri-plugin-http | HTTP 请求 |
| tauri-plugin-process | 进程管理 |
| tauri-plugin-clipboard-manager | 剪贴板 |
| tauri-plugin-notification | 系统通知 |

## 2. 前端技术栈

| 分类 | 技术 | 版本 | 用途 |
|------|------|------|------|
| **语言** | TypeScript | — | 前端主语言 |
| **UI 框架** | React | 18.x | 组件化 UI |
| **构建工具** | Vite | 5.x | 开发/构建 |
| **路由** | react-router-dom | 6.x | 前端路由 (HashRouter) |
| **状态管理** | Zustand | — | 轻量状态管理 |
| **样式** | Tailwind CSS | 3.x | 原子化 CSS |
| **图标** | Lucide React | — | UI 图标库 |
| **Markdown** | 自定义渲染器 | — | 消息渲染 |
| **国际化** | 自定义 i18n | — | 多语言 (en/zh) |

## 3. 开发工具

| 工具 | 用途 |
|------|------|
| Cargo | Rust 包管理 |
| npm/pnpm | 前端包管理 |
| Tauri CLI | 桌面应用构建 |
| rust-analyzer | Rust LSP |
| ESLint | TypeScript 检查 |

## 4. 运行时依赖

| 依赖 | 用途 |
|------|------|
| SQLite (bundled) | 本地数据库 |
| fastembed ONNX 模型 | 本地向量嵌入 |
| Git | Git 集成 |
| Node.js | 部分脚本执行 |
