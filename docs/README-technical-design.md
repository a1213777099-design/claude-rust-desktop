# Claude Desktop (Tauri Edition) — 技术设计文档

> 基于代码库实际架构分析生成 | 2025-06-20

---

## 文档索引

| 文档 | 路径 | 内容 |
|------|------|------|
| **架构总览** | `docs/architecture-overview.md` | 系统架构图、分层架构、通信协议、核心模块职责、关键设计决策 |
| **模块设计** | `docs/module-design.md` | 10个核心模块的详细设计、数据结构、类关系、核心流程 |
| **数据模型** | `docs/data-model.md` | SQLite Schema (5个系统)、Rust 数据结构、索引策略 |
| **API 设计** | `docs/api-design.md` | Bridge REST API (50+ 路由)、Tauri IPC Commands (20+)、SSE 事件流、数据流示例 |
| **技术栈** | `docs/tech-stack.md` | 后端 (Rust/Cargo)、前端 (React/Vite)、开发工具、运行时依赖 |
| **文件结构** | `docs/file-structure.md` | 完整的目录树 (前端 + 后端 + 配置 + 文档) |

## 架构快照

```
                        ┌─────────────────────┐
                        │   React Frontend     │
                        │  (Vite + Zustand)    │
                        └──────────┬──────────┘
                                   │ HTTP / SSE :30080
                        ┌──────────▼──────────┐
                        │   Bridge Server      │
                        │   (Axum 0.8)         │
                        └──────────┬──────────┘
                                   │
              ┌────────────────────┼────────────────────┐
              ▼                    ▼                    ▼
     ┌────────────────┐  ┌────────────────┐  ┌────────────────┐
     │  Native Engine  │  │  Memory System │  │ Orchestration  │
     │  (LLM推理)      │  │  (向量+全文)   │  │  (MetaGPT)     │
     └────────────────┘  └────────────────┘  └────────────────┘
              │                    │                    │
              ▼                    ▼                    ▼
     ┌────────────────┐  ┌────────────────┐  ┌────────────────┐
     │  MCP Server    │  │  Permissions   │  │  SQLite DB     │
     │  (工具协议)    │  │  (4种模式)     │  │  (6 Repos)     │
     └────────────────┘  └────────────────┘  └────────────────┘
```

## 核心数量统计

| 统计项 | 数量 |
|--------|------|
| Rust 模块 | 40+ |
| Rust 源文件 | 100+ |
| React 组件 | 57+ |
| Zustand Stores | 6 |
| 数据库表 | 10 |
| Bridge API 路由 | 50+ |
| Tauri IPC 命令 | 20+ |
| 内置工具定义 | 12 |
| MetaGPT 角色 | 8 |
| MetaGPT Actions | 12 |
| 国际化语言 | 2 (en/zh) |
