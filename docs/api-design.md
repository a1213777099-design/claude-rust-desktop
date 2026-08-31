# API 设计 (API Design)

## 1. Bridge REST API (Axum HTTP Server on :30080)

### 1.1 对话 API

| 方法 | 路径 | 描述 | 请求体 | 响应 |
|------|------|------|--------|------|
| POST | `/api/chat` | 发送消息 (SSE 流式) | `ChatRequest` | SSE stream |
| GET | `/api/chat/{id}` | 获取对话详情 | — | `Conversation` |
| DELETE | `/api/chat/{id}` | 删除对话 | — | `{ok: bool}` |
| GET | `/api/conversations` | 对话列表 | Query params | `Vec<Conversation>` |
| GET | `/api/conversations/search` | 搜索对话 | `?q=keyword` | `Vec<Conversation>` |
| PATCH | `/api/chat/{id}/title` | 修改标题 | `{title: string}` | `{ok: bool}` |
| PATCH | `/api/chat/{id}/pin` | 置顶/取消 | `{pinned: bool}` | `{ok: bool}` |
| POST | `/api/chat/{id}/archive` | 归档对话 | — | `{ok: bool}` |
| POST | `/api/chat/{id}/export` | 导出对话 | `{format: string}` | `{url: string}` |

**ChatRequest** (POST /api/chat):

```json
{
  "conversation_id": "uuid",
  "message": "用户消息",
  "messages": [{"role": "user", "content": "消息"}],
  "model": "claude-sonnet-4-20250514",
  "user_mode": "clawparrot",
  "env_token": "sk-...",
  "env_base_url": "https://api.anthropic.com",
  "research_mode": false,
  "enable_streaming": true,
  "custom_system_prompt": null,
  "permission_mode": "accept_edits",
  "web_search_enabled": false,
  "reasoning_effort": null,
  "extended_thinking": false
}
```

**SSE 事件流**:

```
event: text
data: {"text": "Hello..."}

event: thinking
data: {"thinking": "思考过程..."}

event: tool_use_start
data: {"tool_use_id": "id", "tool_name": "Bash", "input": {...}}

event: tool_result
data: {"tool_use_id": "id", "output": "...", "is_error": false}

event: message_stop
data: {"full_text": "...", "stop_reason": "end_turn"}

event: error
data: {"error": "错误信息"}
```

### 1.2 记忆 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/memories` | 获取记忆列表 |
| POST | `/api/memories` | 创建记忆 |
| GET | `/api/memories/{id}` | 获取单个记忆 |
| PUT | `/api/memories/{id}` | 更新记忆 |
| DELETE | `/api/memories/{id}` | 删除记忆 |
| GET | `/api/memories/search` | FTS5 全文搜索 |
| GET | `/api/memories/stats` | 记忆统计 |
| POST | `/api/memories/backfill` | 回溯生成记忆 |
| POST | `/api/memories/consolidate` | 合并记忆 |
| POST | `/api/memories/tag` | 批量打标签 |
| GET | `/api/memories/tags` | 获取所有标签 |
| POST | `/api/memories/tags/rename` | 重命名标签 |
| GET | `/api/memories/important` | 获取重要记忆 |

**MemoryCreatePayload**:

```json
{
  "summary": "记忆内容",
  "memory_type": "context",
  "importance": 3,
  "tags": "tag1,tag2",
  "workspace_path": "/path",
  "conversation_id": "uuid"
}
```

### 1.3 项目 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/projects` | 项目列表 |
| POST | `/api/projects` | 创建项目 |
| GET | `/api/projects/{id}` | 项目详情 |
| PUT | `/api/projects/{id}` | 更新项目 |
| DELETE | `/api/projects/{id}` | 删除项目 |
| POST | `/api/projects/{id}/archive` | 归档/恢复 |
| GET | `/api/projects/{id}/files` | 项目文件列表 |

### 1.4 Swarm (MetaGPT) API

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/api/swarm/start` | 启动 MetaGPT 工作流 |
| GET | `/api/swarm/sessions` | 会话列表 |
| GET | `/api/swarm/sessions/{id}` | 会话详情 |
| DELETE | `/api/swarm/sessions/{id}` | 删除会话 |
| GET | `/api/swarm/sessions/{id}/events` | SSE 实时事件流 |
| POST | `/api/swarm/sessions/{id}/stop` | 停止工作流 |

### 1.5 MCP API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/mcp/servers` | MCP 服务器列表 |
| POST | `/api/mcp/servers` | 添加服务器 |
| PUT | `/api/mcp/servers/{id}` | 更新服务器 |
| DELETE | `/api/mcp/servers/{id}` | 删除服务器 |
| POST | `/api/mcp/servers/{id}/start` | 启动服务器 |
| POST | `/api/mcp/servers/{id}/stop` | 停止服务器 |
| GET | `/api/mcp/tools` | 所有可用 MCP 工具 |
| POST | `/api/mcp/tools/{name}/call` | 调用 MCP 工具 |
| GET | `/api/mcp/resources` | MCP 资源列表 |

### 1.6 Provider API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/providers` | Provider 列表 |
| POST | `/api/providers` | 添加/更新 Provider |
| DELETE | `/api/providers/{id}` | 删除 Provider |
| POST | `/api/providers/sync` | 批量同步 |
| GET | `/api/providers/models` | 所有可用模型 |
| POST | `/api/providers/test` | 测试连接 |

### 1.7 工具 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/tools` | 工具定义列表 |
| POST | `/api/tools/execute` | 执行工具 |

### 1.8 技能 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/skills` | 技能列表 |
| POST | `/api/skills` | 创建技能 |
| GET | `/api/skills/{id}` | 技能详情 |
| PUT | `/api/skills/{id}` | 更新技能 |
| DELETE | `/api/skills/{id}` | 删除技能 |
| POST | `/api/skills/execute` | 执行技能 |

### 1.9 系统 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/system-status` | 系统状态 |
| GET | `/api/cost-summary` | 成本汇总 |
| GET | `/api/cost-sessions` | 会话成本 |
| GET | `/api/announcements` | 公告列表 |
| POST | `/api/announcements/{id}/read` | 标记已读 |

## 2. Tauri IPC Commands

| Command | 描述 | 参数 | 返回 |
|---------|------|------|------|
| `get_platform` | 获取平台信息 | — | `{os, arch, is_electron}` |
| `get_app_path` | 应用数据路径 | — | `string` |
| `select_directory` | 选择目录对话框 | — | `Option<string>` |
| `show_item_in_folder` | 在文件管理器中显示 | `path` | — |
| `open_folder` | 打开文件夹 | `path` | — |
| `open_external_url` | 打开外部 URL | `url` | — |
| `resize_window` | 调整窗口大小 | `width, height` | — |
| `show_main_window` | 显示主窗口 | — | — |
| `export_workspace` | 导出工作区 | `workspace_id, content` | `path` |
| `get_system_status` | 系统状态 (含 Git Bash) | — | `SystemStatus` |
| `chat_send` | 发送消息 (非流式) | `conversation_id, message, model` | `text` |
| `chat_stream` | 流式对话 (EventSource) | `conversation_id, message, model` | `stream` |
| `execute_tool` | 执行工具 | `tool_name, input` | `result` |
| `check_update` | 检查更新 | — | `UpdateInfo` |
| `install_update` | 安装更新 | — | — |
| `list_slash_commands` | 斜杠命令列表 | — | `Vec<Command>` |
| `search_slash_commands` | 搜索斜杠命令 | `query` | `Vec<Command>` |
| `get_cost_summary` | 成本汇总 | — | `CostSummary` |
| `get_all_session_costs` | 所有会话成本 | — | `Vec<SessionCost>` |

## 3. 数据流示例

### 3.1 对话流 (SSE)

```
User → Frontend → api.ts → HTTP POST /api/chat (SSE)
  → Bridge Server → NativeEngine → ProviderManager → LLM API
  → SSE Stream (text/thinking/tool_use)
  → ToolLoopExecutor → 执行工具 → 继续循环
  → SSE Stream (text/thinking/tool_result)
  → MessageStop → 前端显示

前端 EventSource 监听:
  eventSource.onmessage = (event) => {
    switch(event.type) {
      case 'text': appendText(event.data.text); break;
      case 'thinking': appendThinking(event.data.thinking); break;
      case 'tool_use_start': showToolCall(event.data); break;
      case 'tool_result': updateToolCall(event.data); break;
      case 'message_stop': finalize(); break;
    }
  }
```

### 3.2 MetaGPT 工作流

```
User → POST /api/swarm/start {goal: "..."}
  → Bridge → metagpt_workflow()
  → Environment 初始化 7 个角色
  → 循环执行角色:
      Product Manager → write_prd
      Architect → write_design
      Engineer → write_code
      Reviewer → write_review
      QA Engineer → write_test
      DevOps → deploy
      Project Manager → 总结
  → SSE 实时推送事件 (task_started/task_completed)
  → 持久化到 swarm_sessions + swarm_messages
```
