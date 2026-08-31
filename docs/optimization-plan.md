# Claude Desktop Tauri Optimization Plan

## Phase 0: Documentation Discovery Summary

### Sources Consulted
- `src-tauri/src/prompt/mod.rs` (343 lines) — Existing prompt system with identity block, template management, model resolution
- `src-tauri/src/bridge/mod.rs` (3501 lines) — 120+ routes, 17-element tuple state
- `src-tauri/src/native_engine/tool_loop.rs` (970 lines) — Sequential tool loop, no retry, no parallel
- `src-tauri/src/tools/mod.rs` (1318 lines) — 15+ tools, varying implementation depth
- `src-tauri/src/permissions/manager.rs` (315 lines) — 4 permission modes, hardcoded dangerous tools
- `src-tauri/src/mcp/mod.rs` (1005 lines) — MCP stdio transport only
- `src-tauri/src/logger/mod.rs` (174 lines) — Custom logger, not used in core paths
- `src-tauri/src/orchestration/mod.rs` (732 lines) — Multi-agent with one-line system prompts

### Key Findings
1. Hardcoded paths: `F:/Projects/claude-code-rust/mcp_debug.log` in bridge/mod.rs:985 and mcp/mod.rs:11
2. `chat_debug.log` macro in bridge/mod.rs:11 uses `current_dir()` (relative, acceptable)
3. `eprintln!` used in 50+ places instead of tracing
4. AppState tuple has 17 elements — extremely fragile
5. Tool loop is purely sequential (tool_loop.rs:379-600)
6. Agent system prompts are single sentences (orchestration/mod.rs:616-618)
7. Permission check is tool-name-only, no argument inspection

---

## Phase 1: Unified Logging (P1-5) — Quick Win

**Goal**: Eliminate all hardcoded paths and replace `eprintln!` with `tracing`.

### Tasks
1. **Add tracing-subscriber initialization** in `main.rs`
   - File: `src-tauri/src/main.rs`
   - Pattern: Follow `tracing-subscriber` docs for `fmt::init()`
   - Replace `std::env::set_var("RUST_BACKTRACE", "1")` with tracing setup

2. **Replace hardcoded `mcp_debug.log` paths**
   - File: `src-tauri/src/mcp/mod.rs` line 11 — `mcp_log()` function
   - File: `src-tauri/src/bridge/mod.rs` line 985 — `context_size_handler`
   - Change to: `tracing::debug!()` / `tracing::info!()` / `tracing::error!()`

3. **Replace `log_to_file!` macro** in bridge/mod.rs
   - File: `src-tauri/src/bridge/mod.rs` lines 8-17
   - Change all `log_to_file!()` calls to `tracing::debug!()`

4. **Replace `eprintln!` with tracing across all modules**
   - Files: All `.rs` files with `eprintln!`
   - Map: `eprintln!("[Bridge]...")` → `tracing::info!(target: "bridge", ...)`
   - Map: `eprintln!("[Chat]...")` → `tracing::debug!(target: "chat", ...)`
   - Map: error cases → `tracing::error!(...)`

5. **Add tracing to Cargo.toml** if not already present
   - Already has: `tracing = "0.1"`, `tracing-subscriber = "0.3"`
   - Add feature: `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`

### Verification
- `cargo build` succeeds
- No `F:/` paths remain: `grep -r "F:/" src-tauri/src/`
- No `eprintln!` in core modules: `grep -rn "eprintln!" src-tauri/src/bridge/ src-tauri/src/native_engine/ src-tauri/src/mcp/`

---

## Phase 2: AppState Refactoring (P0-2 prep)

**Goal**: Replace 17-element tuple with named struct.

### Tasks
1. **Create `AppState` struct** in new file `src-tauri/src/bridge/state.rs`
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
       pub orchestrator: Arc<Mutex<Option<MultiAgentOrchestrator>>>,
   }
   ```

2. **Split bridge/mod.rs** into route modules:
   - `src-tauri/src/bridge/mod.rs` — re-exports + `start()` + router setup
   - `src-tauri/src/bridge/state.rs` — AppState struct + AppState impl
   - `src-tauri/src/bridge/routes/chat.rs` — chat_handler, chat_stream_handler, conversation_* handlers
   - `src-tauri/src/bridge/routes/providers.rs` — providers_* handlers
   - `src-tauri/src/bridge/routes/projects.rs` — projects_* handlers
   - `src-tauri/src/bridge/routes/skills.rs` — skills_* handlers
   - `src-tauri/src/bridge/routes/mcp.rs` — mcp_* handlers
   - `src-tauri/src/bridge/routes/system.rs` — system_status, config_*, logs_*, etc.
   - `src-tauri/src/bridge/routes/workflow.rs` — workflow_*, task_* handlers
   - `src-tauri/src/bridge/routes/tools.rs` — tools_handler, tool_execute_handler
   - `src-tauri/src/bridge/routes/mod.rs` — router builder function

3. **Update all handlers** to use `State(state): State<AppState>` with named fields
   - Replace `state.6.clone()` → `state.db_manager.clone()`
   - Replace `state.14.clone()` → `state.native_engine.clone()`
   - etc.

### Verification
- `cargo build` succeeds
- `bridge/mod.rs` under 200 lines
- No tuple-index access (`state.N`) remains

---

## Phase 3: System Prompt Engineering (P0-1)

**Goal**: Create high-quality system prompts for different agent roles.

### Tasks
1. **Create `src-tauri/src/prompt/prompts.rs`** with role-specific prompts:
   - `CHAT_SYSTEM_PROMPT` — General assistant prompt with tool usage guidelines
   - `RESEARCH_SYSTEM_PROMPT` — Deep research agent prompt
   - `CODE_SYSTEM_PROMPT` — Code-focused assistant prompt
   - `MULTIAGENT_ROLE_PROMPTS` — Per-role prompts (Architect, Developer, Reviewer, etc.)

2. **Design prompt structure** (modeled after Claude Code patterns):
   ```
   <identity>...</identity>
   <capabilities>Available tools and when to use each</capabilities>
   <constraints>Safety rules, permission boundaries</constraints>
   <behavior>Response style, error handling, user interaction</behavior>
   ```

3. **Update tool_loop.rs** to inject proper system prompts
   - File: `src-tauri/src/native_engine/tool_loop.rs`
   - Pass role-specific system prompt based on context

4. **Update orchestration/mod.rs** `execute_task()`
   - File: `src-tauri/src/orchestration/mod.rs` lines 611-639
   - Replace one-line prompt with full role-specific prompt
   - Include tool usage guidelines per role

### Prompt Content Guidelines
- Tool usage: When to use Read vs Grep, when to use Bash vs specific tools
- Error handling: How to respond to tool failures
- Safety: Never expose credentials, destructive operation awareness
- Language: Match user language (Chinese/English)
- Response format: Structured output for multi-agent results

### Verification
- Each prompt is >200 chars (not one-liners)
- Prompts contain tool-specific guidance
- `cargo build` succeeds

---

## Phase 4: Tool Loop Error Recovery (P1-4)

**Goal**: Add automatic retry with exponential backoff for transient failures.

### Tasks
1. **Add retry configuration** to `ToolLoopExecutor`
   - File: `src-tauri/src/native_engine/tool_loop.rs`
   - Add fields: `max_retries: usize`, `retry_base_ms: u64`
   - Default: 2 retries, 500ms base

2. **Implement retry logic** in `execute_tool_call()`
   - File: `src-tauri/src/native_engine/tool_loop.rs` lines 188-245
   - Only retry on transient errors (timeout, network, rate limit)
   - Never retry on permission denied, file not found, validation errors
   - Exponential backoff: 500ms, 1000ms, 2000ms

3. **Create error classification** in `src-tauri/src/tools/mod.rs`
   ```rust
   pub enum ToolError {
       Transient(String),   // Retry-able
       Permanent(String),   // Not retry-able
       Permission(String),  // Needs user input
   }
   ```

4. **Add retry event** to `EngineEvent`
   - New variant: `ToolRetry { tool_use_id, attempt, max_attempts, reason }`
   - Frontend can show retry status

### Verification
- Network timeout triggers retry
- Permission denied does NOT trigger retry
- Max retries respected
- `cargo build` succeeds

---

## Phase 5: Unified Logging Completion (P1-5 continued)

**Goal**: Replace ALL remaining eprintln! and log_to_file! with tracing.

### Tasks
1. **Audit and replace** all remaining `eprintln!` calls
2. **Remove** `log_to_file!` macro entirely
3. **Remove** `mcp_log()` function entirely
4. **Add structured fields** to key log points:
   - `conversation_id`, `tool_name`, `model`, `provider`
5. **Configure env-filter** for development vs production:
   - Dev: `RUST_LOG=debug`
   - Prod: `RUST_LOG=info`

### Verification
- `grep -rn "eprintln!" src-tauri/src/ | wc -l` returns < 5
- `grep -rn "F:/" src-tauri/src/ | wc -l` returns 0
- `grep -rn "log_to_file" src-tauri/src/ | wc -l` returns 0

---

## Phase 6: Tool Implementation Deepening (P1-3)

**Goal**: Improve core tool implementations.

### Tasks
1. **Bash tool safety**
   - File: `src-tauri/src/tools/mod.rs` (Bash handler)
   - Add: Configurable timeout (default 60s, max 600s)
   - Add: Dangerous command detection (`rm -rf /`, `git push --force` to main)
   - Add: Output truncation (max 50KB)

2. **Grep improvement**
   - Consider: Shell out to `rg` if available (faster than regex crate)
   - Fallback to current regex implementation
   - Add: context lines (-A/-B/-C), output modes

3. **WebFetch improvement**
   - Add: HTML → Markdown conversion (use `html2text` or similar crate)
   - Add: Content size limit (1MB default)
   - Add: Redirect handling

4. **Tool output size limits**
   - All tools: Truncate output > 100KB with "... (truncated)"
   - Read tool: Respect offset/limit already, but add default limit

### Verification
- Bash with 1s timeout actually times out
- Grep with `rg` is measurably faster on large dirs
- WebFetch returns readable text from HTML pages
- `cargo build` succeeds

---

## Execution Order

```
Phase 1 (Logging) ──► Phase 2 (AppState) ──► Phase 3 (Prompts)
                                                Phase 4 (Retry)
                                                Phase 5 (Logging pt2)
                                                Phase 6 (Tools)
```

Phases 3-6 are independent after Phase 2 completes and can be parallelized.

## Dependencies
- Phase 2 depends on Phase 1 (clean logging before refactoring)
- Phase 3, 4, 5, 6 are independent of each other
- Phase 5 is Phase 1 completion (catch remaining eprintln!)
