# PRD: Claude Desktop 架构服务网关平台

## 版本历史

| 版本 | 日期 | 作者 | 变更说明 |
|------|------|------|----------|
| 1.0 | 2026-05-30 | Product Manager | 初始版本 |

---

## 1. 背景 (Background)

### 1.1 项目概述

**Claude Desktop (Tauri Edition)** 是一个基于 **Tauri 2.0 + Rust** 构建的跨平台 AI 桌面助手应用。当前版本 v2.0.0 已具备以下核心能力：

- **Rust 后端**: 基于 Tauri 2.0 框架，包含 Bridge HTTP 服务、多智能体编排、MCP 协议支持、SQLite 持久化记忆等
- **React 前端**: 基于 Vite + TypeScript + Zustand 状态管理，提供丰富的 UI 交互
- **Bridge Server**: 基于 Axum 0.8 的 REST API 服务（端口 30080），作为前端与后端能力的通信桥梁
- **API Proxy**: 基于 Express 的认证代理服务（端口 30090），提供用户注册、登录、API Key 管理等

### 1.2 问题陈述

当前系统虽然功能丰富，但存在以下架构层面的挑战：

1. **能力暴露碎片化**：后端能力通过 Tauri Commands、Bridge HTTP API、MCP 三种方式暴露，缺乏统一的治理入口
2. **网关能力薄弱**：Bridge Server 当前仅作为简单 HTTP 路由，缺少 API 网关应有的流量控制、认证鉴权、请求/响应转换、协议适配等能力
3. **多协议并存**：系统同时支持 SSE 流式、WebSocket（规划中）、HTTP REST、MCP 协议栈，缺乏统一的协议转换层
4. **服务治理缺失**：无服务发现、健康检查、熔断降级、负载均衡等微服务治理能力
5. **安全性不足**：API 认证、速率限制、IP 白名单、敏感操作审计等安全机制不完善

### 1.3 业务目标

将 Bridge Server 从简单的 HTTP 路由升级为 **企业级 API 服务网关**，使其成为所有 AI 能力、工具执行、多智能体编排的统一入口，提供安全、可控、可观测的 API 管理平台。

---

## 2. 目标 (Goals)

### 2.1 核心目标

1. **统一网关入口**：将所有后端能力（AI 对话、工具执行、多智能体、MCP、记忆、文件操作等）收敛到单一网关
2. **企业级安全**：实现认证、授权、速率限制、审计日志、敏感操作拦截等安全机制
3. **协议适配与转换**：支持 REST、SSE、WebSocket、MCP 等多协议的统一接入和转换
4. **可观测性**：提供请求追踪、指标监控、日志聚合、健康检查等运维能力
5. **插件化扩展**：基于中间件或插件机制，支持自定义路由、转换、过滤等扩展

### 2.2 非目标

- 不重写现有 Bridge Server（在现有 Axum 基础上增强）
- 不改变前端架构（保持 React + TypeScript + Vite）
- 不引入额外运行时依赖（保持纯 Rust 实现）
- 不考虑容器化部署（当前专注于桌面应用场景）

---

## 3. 用户故事 (User Stories)

### US-1: 统一 API 入口

> **作为** 前端开发者  
> **我希望** 所有后端能力通过单一的 HTTP 网关地址访问  
> **以便于** 减少前端对多种通信方式的适配成本，简化错误处理和重试逻辑

**验收标准**：
- 前端只需通过 `http://localhost:30080/api/*` 访问所有能力
- Tauri Commands 和 Bridge HTTP API 统一为网关路由
- 网关提供统一的请求/响应格式

### US-2: API 认证与授权

> **作为** 平台管理员  
> **我希望** 网关对敏感操作进行认证和授权检查  
> **以便于** 防止未授权使用 AI 资源和执行危险命令

**验收标准**：
- 支持 API Key 认证（Bearer Token）
- 支持基于角色的权限控制（RBAC）
- 敏感操作（文件删除、命令执行等）需要二次确认
- 认证失败返回 401，权限不足返回 403

### US-3: 速率限制与配额管理

> **作为** 系统管理员  
> **我希望** 对 API 调用进行速率限制和配额管理  
> **以便于** 防止滥用和资源耗尽，保障服务质量

**验收标准**：
- 支持基于用户/API Key 的速率限制（RPS/QPS）
- 支持每日/每月配额管理
- 超限时返回 429 Too Many Requests
- 配额接近上限时发出告警

### US-4: 多协议统一接入

> **作为** AI 应用开发者  
> **我希望** 通过网关同时支持 HTTP REST 和 SSE 流式协议  
> **以便于** 在需要实时流式输出的场景下获得良好体验

**验收标准**：
- 支持 `Accept: text/event-stream` 自动切换为 SSE 模式
- 支持 WebSocket 升级（后续迭代）
- 协议转换层自动处理消息格式适配

### US-5: 请求审计与追踪

> **作为** 安全审计员  
> **我希望** 所有经过网关的请求都有完整的审计日志和追踪 ID  
> **以便于** 追溯问题、合规审计和安全事件调查

**验收标准**：
- 每个请求分配唯一 Trace ID（`X-Request-ID`）
- 记录请求来源、时间、方法、路径、状态码、耗时
- 敏感操作记录请求体和响应体摘要
- 审计日志可查询、可导出

### US-6: 健康检查与熔断

> **作为** 运维人员  
> **我希望** 网关提供健康检查端点并支持熔断降级  
> **以便于** 及时发现后端服务异常并优雅降级

**验收标准**：
- 提供 `/health` 端点返回各服务健康状态
- 后端连续失败超过阈值时自动熔断
- 熔断后返回友好错误提示
- 定期恢复检测

### US-7: 插件化中间件

> **作为** 平台扩展开发者  
> **我希望** 能够通过插件/中间件机制扩展网关能力  
> **以便于** 在不修改核心代码的情况下添加自定义逻辑

**验收标准**：
- 支持请求前置/后置中间件链
- 中间件可读取和修改请求/响应
- 提供 `tower::Layer` 或类似 trait 接口
- 支持条件路由和通配符匹配

---

## 4. 功能需求 (Requirements)

### 4.1 功能需求

| ID | 需求描述 | 优先级 | 关联用户故事 |
|----|---------|--------|------------|
| FR-1 | **路由聚合**：将现有 Bridge 路由（`/api/chat`, `/api/tools`, `/api/research`, `/api/mcp` 等）统一注册到网关路由器 | P0 | US-1 |
| FR-2 | **认证中间件**：实现 API Key 认证中间件，支持 Bearer Token 验证 | P0 | US-2 |
| FR-3 | **速率限制**：实现基于令牌桶的速率限制中间件，支持按用户/全局配置 | P1 | US-3 |
| FR-4 | **协议适配**：实现内容协商中间件，根据 Accept 头自动切换 JSON/SSE 响应 | P1 | US-4 |
| FR-5 | **请求追踪**：为每个请求生成唯一 Trace ID，注入请求上下文 | P0 | US-5 |
| FR-6 | **审计日志**：记录所有请求的元数据（来源、方法、路径、状态、耗时） | P1 | US-5 |
| FR-7 | **健康检查**：实现 `/health` 和 `/ready` 端点，聚合各服务状态 | P1 | US-6 |
| FR-8 | **熔断器**：实现基于滑动窗口的熔断降级机制 | P2 | US-6 |
| FR-9 | **中间件链**：实现可组合的中间件链，支持路由级别和全局注册 | P1 | US-7 |
| FR-10 | **错误统一**：定义统一错误响应格式，网关层捕获并格式化所有错误 | P0 | US-1 |

### 4.2 非功能需求

| ID | 需求描述 | 指标 |
|----|---------|------|
| NFR-1 | **性能**：网关转发延迟 < 5ms（不含后端处理时间） | < 5ms |
| NFR-2 | **并发**：支持至少 1000 并发连接 | 1000+ |
| NFR-3 | **可用性**：网关自身无单点故障，可优雅重启 | 99.9% |
| NFR-4 | **安全**：认证绕过攻击防护、CSRF 防护、请求体大小限制 | OWASP Top 10 |
| NFR-5 | **可维护性**：代码覆盖率 > 80%，注释完善 | > 80% |
| NFR-6 | **兼容性**：现有 API 路径和响应格式保持向后兼容 | 无破坏性变更 |

---

## 5. 当前架构现状分析

### 5.1 现有代码库结构

```
src-tauri/
├── src/
│   ├── main.rs              # Tauri 应用入口，初始化 Bridge、MCP、DB
│   ├── lib.rs                # 模块声明
│   ├── bridge/
│   │   ├── mod.rs            # BridgeServer: Axum HTTP 服务 (端口 30080)
│   │   ├── state.rs          # AppState: 共享状态 (17+ 组件)
│   │   └── memory_handlers_v2.rs  # 记忆相关路由
│   ├── native_engine/        # AI 引擎层
│   │   ├── engine_core.rs    # NativeEngine 核心
│   │   ├── anthropic_client.rs    # Anthropic API 客户端
│   │   ├── openai_client.rs       # OpenAI API 客户端
│   │   ├── provider_manager.rs    # 提供商管理
│   │   ├── session_manager.rs     # 会话管理
│   │   └── tool_loop.rs           # 工具循环
│   ├── multiagent/           # 多智能体编排
│   ├── orchestration/        # 编排引擎 (MetaGPT)
│   ├── mcp/                  # MCP 协议支持
│   ├── permissions/          # 权限管理
│   ├── skills/               # 技能引擎
│   ├── tools/                # 工具系统
│   ├── db/                   # SQLite 数据库
│   ├── memory/               # 记忆系统
│   ├── config/               # 配置管理
│   ├── commands/             # Tauri Commands
│   └── ...                   # 其他模块
│
api-proxy/
├── server.js                 # Express 认证代理 (端口 30090)
└── package.json
```

### 5.2 当前网关能力评估

| 能力维度 | 当前状态 | 目标状态 |
|---------|---------|---------|
| 路由管理 | 硬编码路由注册 | 声明式路由配置 |
| 认证鉴权 | 无（Bridge）/ 基础 JWT（API Proxy） | 统一 API Key + RBAC |
| 速率限制 | 无 | 令牌桶限流 |
| 协议转换 | 手动处理 | 自动内容协商 |
| 请求追踪 | 无 | 全局 Trace ID |
| 审计日志 | 无 | 结构化审计 |
| 健康检查 | 无 | 聚合健康检查 |
| 熔断降级 | 无 | 滑动窗口熔断 |
| 中间件 | 无 | 插件化中间件链 |
| 错误处理 | 分散在各 handler | 统一网关层处理 |

### 5.3 技术债务

1. **AppState 过大**：当前 `state.rs` 中 `AppState` 包含 17+ 字段，不利于维护和测试
2. **路由分散**：路由注册分散在 `bridge/mod.rs`、`commands/mod.rs` 等多处
3. **错误处理不一致**：各 handler 返回不同格式的错误响应
4. **缺乏抽象**：中间件逻辑与业务 handler 耦合

---

## 6. 验收标准 (Acceptance Criteria)

### AC-1: 网关路由聚合

- [ ] 所有现有 Bridge API 路径（`/api/chat`, `/api/tools`, `/api/research`, `/api/memory`, `/api/config`, `/api/mcp` 等）通过统一网关路由器注册
- [ ] 新增路由统一使用声明式配置（如 `Router::new().route("/api/chat", ...)`）
- [ ] Tauri Commands 中的 HTTP 调用路径统一指向网关

### AC-2: 认证与授权

- [ ] 网关中间件支持 `Authorization: Bearer <api_key>` 认证
- [ ] API Key 从 SQLite 数据库验证（复用现有 `api_proxy` 数据库表）
- [ ] 支持路由级别的权限注解（`#[require_role("admin")]` 或类似）
- [ ] 未认证请求返回 `401 { "error": "Unauthorized", "code": "AUTH_REQUIRED" }`
- [ ] 权限不足返回 `403 { "error": "Forbidden", "code": "INSUFFICIENT_PERMISSIONS" }`

### AC-3: 速率限制

- [ ] 默认全局限制：100 请求/秒/用户
- [ ] 超限返回 `429 { "error": "Too Many Requests", "retry_after": 5 }`
- [ ] 速率限制可通过配置文件调整
- [ ] 限制信息通过响应头 `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` 返回

### AC-4: 请求追踪与审计

- [ ] 每个请求自动生成 `X-Request-ID`（UUID v4）
- [ ] 审计日志记录：时间戳、方法、路径、状态码、延迟、客户端 IP、用户 ID
- [ ] 敏感操作（`POST /api/tools/execute`, `DELETE /api/*`）记录请求体摘要
- [ ] 审计日志存储在 SQLite 数据库 `audit_logs` 表

### AC-5: 健康检查与熔断

- [ ] `GET /health` 返回各服务健康状态 JSON
- [ ] `GET /ready` 返回就绪状态
- [ ] 后端连续 5 次错误（5xx）触发熔断，熔断时间 30 秒
- [ ] 熔断期间返回 `503 { "error": "Service Unavailable", "circuit": "open" }`

### AC-6: 统一错误处理

- [ ] 所有错误响应格式统一：
  ```json
  {
    "error": "描述信息",
    "code": "ERROR_CODE",
    "request_id": "uuid",
    "timestamp": 1234567890
  }
  ```
- [ ] 网关层捕获所有未处理 panic，返回 500 错误
- [ ] 数据库错误返回 500，不泄露 SQL 细节

### AC-7: 性能基准

- [ ] 网关空路由转发延迟 < 1ms
- [ ] 认证中间件附加延迟 < 2ms
- [ ] 速率限制中间件附加延迟 < 1ms
- [ ] 审计日志写入不阻塞请求响应（异步写入）

---

## 7. 风险与缓解措施 (Risks)

| ID | 风险描述 | 概率 | 影响 | 等级 | 缓解措施 |
|----|---------|------|------|------|---------|
| R1 | **向后兼容性破坏**：重构网关路由可能导致现有前端调用失败 | 中 | 高 | 高 | 保留旧路由别名；添加集成测试覆盖所有现有 API 路径；分阶段灰度切换 |
| R2 | **性能退化**：引入多层中间件导致请求延迟增加 | 中 | 中 | 中 | 使用异步中间件；基准测试对比；关键路径优化（如认证缓存） |
| R3 | **认证成为瓶颈**：每次请求都查询数据库验证 API Key | 高 | 中 | 高 | 引入内存缓存（TTL 60s）；使用 HMAC 本地验证 |
| R4 | **熔断误触发**：网络抖动导致熔断器错误打开 | 中 | 中 | 中 | 最小请求数采样（至少 10 请求/窗口）；半开状态恢复；手动重置 API |
| R5 | **审计日志膨胀**：大量请求导致审计表快速增长 | 高 | 低 | 中 | 日志轮转策略（保留 7 天）；异步批量写入；归档历史数据 |
| R6 | **团队学习成本**：团队成员对 Axum 中间件机制不熟悉 | 中 | 低 | 低 | 编写开发文档；Code Review 制度；TDD 驱动开发 |
| R7 | **多协议适配复杂**：WebSocket + SSE + REST 协议转换逻辑复杂 | 中 | 高 | 高 | 分阶段实现（先 SSE + REST）；引入协议适配层抽象；参考业界成熟方案 |

---

## 8. 实施建议

### 8.1 分阶段实施

**Phase 1（基础网关）**：
- 统一路由注册与中间件链框架
- 请求追踪（Trace ID）
- 统一错误处理
- 集成测试

**Phase 2（安全增强）**：
- API Key 认证中间件
- 速率限制
- 审计日志
- 敏感操作保护

**Phase 3（可观测性）**：
- 健康检查端点
- 熔断器
- 指标暴露（Prometheus）
- 健康看板

**Phase 4（协议扩展）**：
- WebSocket 支持
- MCP 协议代理
- 协议自动转换

### 8.2 技术选型建议

| 组件 | 推荐方案 | 理由 |
|------|---------|------|
| 网关框架 | Axum 0.8 + Tower 中间件 | 已在项目中稳定使用，生态成熟 |
| 认证库 | `tower-http` auth + 自研 | 复用现有 ProviderManager |
| 速率限制 | `governor` crate | 基于令牌桶，支持 burst |
| 熔断器 | `circuit-breaker` 或自研 | 轻量级滑动窗口实现 |
| 追踪 | `tracing` + `tracing-opentelemetry` | 已使用 tracing，可扩展 |
| 审计存储 | SQLite + `rusqlite` | 复用现有数据库层 |

### 8.3 架构演进路线

```
Phase 1 (当前)
┌───────────────────────────────────────────┐
│  Bridge Server (Axum)                      │
│  ├── Chat Handler                          │
│  ├── Tool Handler                          │
│  ├── Research Handler                      │
│  └── ...                                   │
└───────────────────────────────────────────┘

Phase 4 (目标)
┌───────────────────────────────────────────┐
│  API Gateway (Axum + Tower)               │
│  ├── Auth Middleware (API Key + RBAC)     │
│  ├── Rate Limiter (Governor)              │
│  ├── Request Tracing (Trace ID)           │
│  ├── Circuit Breaker                      │
│  ├── Protocol Adapter (REST/SSE/WS)       │
│  ├── Audit Logger                         │
│  ├── Router → Backend Services            │
│  │   ├── Chat Service                     │
│  │   ├── Tool Service                     │
│  │   ├── Multi-Agent Service              │
│  │   ├── MCP Proxy                        │
│  │   └── ...                              │
│  └── Health Endpoint                      │
└───────────────────────────────────────────┘
```

---

## 9. 附录

### 9.1 相关文档

- [README.md](../README.md) - 项目总览
- [Cargo.toml](../src-tauri/Cargo.toml) - Rust 依赖清单
- [.trae/specs/full-rust-refactor/spec.md](../.trae/specs/full-rust-refactor/spec.md) - Rust 重构 Spec
- [.trae/specs/production-grade-overhaul/spec.md](../.trae/specs/production-grade-overhaul/spec.md) - 生产级改造 Spec
- [.trae/specs/rust-feature-completion/spec.md](../.trae/specs/rust-feature-completion/spec.md) - 功能补全 Spec

### 9.2 术语表

| 术语 | 定义 |
|------|------|
| Bridge Server | 基于 Axum 的 HTTP API 服务，作为前端与后端能力的桥梁 |
| API Gateway | 统一的服务入口，提供认证、限流、路由等网关能力 |
| MCP | Model Context Protocol，模型上下文协议 |
| SSE | Server-Sent Events，服务器推送事件 |
| RBAC | Role-Based Access Control，基于角色的访问控制 |
| Tauri | 基于 Rust 的桌面应用框架，替代 Electron |

### 9.3 现有 API 清单

| 方法 | 路径 | 描述 | 当前状态 |
|------|------|------|---------|
| POST | `/api/chat` | 发送聊天消息 | ✅ 已实现 |
| GET | `/api/chat/stream` | 流式聊天 | ✅ 已实现 |
| POST | `/api/tools` | 执行工具 | ✅ 已实现 |
| POST | `/api/research/start` | 启动研究任务 | ✅ 已实现 |
| GET | `/api/research/{id}/status` | 研究状态 | ✅ 已实现 |
| POST | `/api/research/{id}/stop` | 停止研究 | ✅ 已实现 |
| POST | `/api/mcp/execute` | MCP 工具执行 | ✅ 已实现 |
| GET | `/api/config` | 获取配置 | ✅ 已实现 |
| POST | `/api/config` | 更新配置 | ✅ 已实现 |
| GET | `/api/memory` | 查询记忆 | ✅ 已实现 |
| POST | `/api/memory` | 存储记忆 | ✅ 已实现 |
| GET | `/api/system/status` | 系统状态 | ✅ 已实现 |
| POST | `/api/auth/register` | 用户注册（API Proxy） | ✅ 已实现 |
| POST | `/api/auth/login` | 用户登录（API Proxy） | ✅ 已实现 |
| GET | `/api/auth/me` | 当前用户（API Proxy） | ✅ 已实现 |
