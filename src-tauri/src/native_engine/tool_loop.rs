use crate::native_engine::anthropic_client::{AnthropicClient, AnthropicContent, AnthropicMessage, ContentBlock};
use crate::native_engine::openai_client::{OpenAIClient, OpenAIContent, OpenAIMessage};
use crate::native_engine::provider_manager::{ApiFormat, ResolvedProvider};
use crate::permissions::{PermissionManager, PermissionResult};
use crate::streaming::sse_parser::{consume_sse_payloads, merge_tool_args, recover_malformed_tool_input};
use crate::tools::get_tool_definitions;
use crate::mcp::McpToolRegistry;
use anyhow::Result;
use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, oneshot};

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Text(String),
    Thinking(String),
    ToolUseStart {
        tool_use_id: String,
        tool_name: String,
        tool_input: Value,
        text_before: String,
    },
    ToolArgDelta {
        tool_use_id: String,
        delta: String,
    },
    ToolUseDone {
        tool_use_id: String,
        tool_name: String,
        tool_input: Value,
        output: String,
        is_error: bool,
    },
    MessageStart {
        model: String,
    },
    MessageDelta {
        stop_reason: Option<String>,
    },
    MessageStop {
        full_text: String,
        stop_reason: Option<String>,
    },
    Error(String),
    Usage(Value),
    AskUser {
        question: String,
        options: Vec<String>,
    },
}

pub struct ToolLoopExecutor {
    provider: ResolvedProvider,
    messages: Vec<Value>,
    system_prompt: Option<String>,
    max_tokens: u32,
    max_tool_iterations: usize,
    event_tx: mpsc::Sender<EngineEvent>,
    anthropic_client: AnthropicClient,
    openai_client: OpenAIClient,
    workspace_cwd: String,
    mcp_registry: Option<Arc<McpToolRegistry>>,
    streaming_tool_args: HashMap<String, StreamingToolCall>,
    conv_id: Option<String>,
    answer_waiters: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
    permission_manager: Option<Arc<PermissionManager>>,
    web_search_enabled: bool,
    reasoning_effort: Option<String>,
    extended_thinking: bool,
    streaming: bool,
    retry_config: crate::tools::retry::RetryConfig,
    /// 本轮已完成的工具调用记录 (id, name, input_json, output, is_error)，
    /// 随助手消息持久化到 tool_calls 表，前端重载会话后仍能渲染工具卡片
    pub completed_tool_calls: Vec<(String, String, String, String, bool)>,
}

#[derive(Debug, Clone)]
/// A pending tool call collected during streaming.
struct PendingToolCall {
    id: String,
    name: String,
    input: Value,
}

#[derive(Debug, Clone)]
struct StreamingToolCall {
    name: String,
    accumulated_args: String,
}

impl ToolLoopExecutor {
    pub fn new(
        provider: ResolvedProvider,
        messages: Vec<Value>,
        system_prompt: Option<String>,
        max_tokens: u32,
        event_tx: mpsc::Sender<EngineEvent>,
        workspace_cwd: String,
    ) -> Self {
        Self {
            provider,
            messages,
            system_prompt,
            max_tokens,
            max_tool_iterations: usize::MAX,
            event_tx,
            anthropic_client: AnthropicClient::new(),
            openai_client: OpenAIClient::new(),
            workspace_cwd,
            mcp_registry: None,
            streaming_tool_args: HashMap::new(),
            conv_id: None,
            answer_waiters: Arc::new(Mutex::new(HashMap::new())),
            permission_manager: None,
            web_search_enabled: false,
            reasoning_effort: None,
            extended_thinking: false,
            streaming: true,
            retry_config: crate::tools::retry::RetryConfig::default(),
            completed_tool_calls: Vec::new(),
        }
    }

    pub fn with_conv_id(mut self, conv_id: String) -> Self {
        self.conv_id = Some(conv_id);
        self
    }

    pub fn with_answer_waiters(mut self, waiters: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>) -> Self {
        self.answer_waiters = waiters;
        self
    }

    pub fn get_answer_waiters(&self) -> Arc<Mutex<HashMap<String, oneshot::Sender<String>>>> {
        self.answer_waiters.clone()
    }

    pub fn with_mcp_registry(mut self, registry: Arc<McpToolRegistry>) -> Self {
        self.mcp_registry = Some(registry);
        self
    }

    pub fn with_permission_manager(mut self, manager: Arc<PermissionManager>) -> Self {
        self.permission_manager = Some(manager);
        self
    }

    pub fn with_web_search_enabled(mut self, enabled: bool) -> Self {
        self.web_search_enabled = enabled;
        self
    }

    pub fn with_reasoning_effort(mut self, effort: Option<String>) -> Self {
        self.reasoning_effort = effort;
        self
    }

    pub fn with_extended_thinking(mut self, enabled: bool) -> Self {
        self.extended_thinking = enabled;
        self
    }

    pub fn with_streaming(mut self, enabled: bool) -> Self {
        self.streaming = enabled;
        self
    }

    pub fn with_retry_config(mut self, config: crate::tools::retry::RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    async fn check_permission(&self, tool_name: &str, tool_input: &Value) -> PermissionResult {
        if let Some(ref pm) = self.permission_manager {
            let conv_id = self.conv_id.clone().unwrap_or_default();
            let workspace_path = Some(self.workspace_cwd.clone());
            
            let context = crate::permissions::PermissionContext {
                tool_name: tool_name.to_string(),
                tool_input: tool_input.clone(),
                conversation_id: conv_id,
                user_id: None,
                workspace_path,
            };
            pm.check_permission(&context)
        } else {
            PermissionResult::Granted
        }
    }

    pub async fn execute(&mut self) -> Result<(String, Option<String>)> {
        let _ = self.event_tx.send(EngineEvent::MessageStart {
            model: self.provider.model.id.clone(),
        }).await;

        let (full_text, stop_reason) = match self.provider.provider.api_format {
            ApiFormat::Anthropic => {
                self.execute_anthropic_loop().await?
            }
            ApiFormat::OpenAI => {
                self.execute_openai_loop().await?
            }
        };

        let _ = self.event_tx.send(EngineEvent::MessageStop {
            full_text: full_text.clone(),
            stop_reason: stop_reason.clone(),
        }).await;
        Ok((full_text, stop_reason))
    }

    async fn execute_tool_call(
        &mut self,
        tool_name: &str,
        tool_input: &Value,
        _tool_use_id: &str,
    ) -> (Value, String, bool) {
        if tool_name == "AskUserQuestion" {
            return self.execute_ask_user_question(tool_input).await;
        }

        let permission_result = self.check_permission(tool_name, tool_input).await;
        match permission_result {
            PermissionResult::Denied(reason) => {
                return (tool_input.clone(), format!("Permission denied: {}", reason), true);
            }
            PermissionResult::RequiresConfirmation(message) => {
                return self.execute_ask_user_confirmation(tool_name, tool_input, &message).await;
            }
            PermissionResult::Granted => {}
        }

        // Execute with retry for transient errors
        let max_attempts = self.retry_config.max_retries + 1;
        let mut last_output = String::new();
        let mut last_is_error = false;

        for attempt in 0..max_attempts {
            let (output_str, is_error) = self.execute_tool_inner(tool_name, tool_input).await;

            if !is_error {
                return (tool_input.clone(), output_str, false);
            }

            // Classify the error
            let tool_error = crate::tools::retry::ToolError::classify(&output_str);

            if !tool_error.is_retryable() || attempt + 1 >= max_attempts {
                // Permanent error or last attempt — return as-is
                return (tool_input.clone(), output_str, true);
            }

            // Transient error — retry with backoff
            let delay_ms = self.retry_config.delay_for_attempt(attempt);
            tracing::warn!(target: "tool_loop",
                "Tool '{}' failed (attempt {}/{}): {}. Retrying in {}ms...",
                tool_name, attempt + 1, max_attempts, output_str, delay_ms);

            let _ = self.event_tx.send(EngineEvent::ToolUseDone {
                tool_use_id: _tool_use_id.to_string(),
                tool_name: tool_name.to_string(),
                tool_input: tool_input.clone(),
                output: format!("Retrying (attempt {})...", attempt + 2),
                is_error: false,
            }).await;

            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            last_output = output_str;
            last_is_error = is_error;
        }

        (tool_input.clone(), last_output, last_is_error)
    }

    /// Inner tool execution without retry logic.
    async fn execute_tool_inner(
        &self,
        tool_name: &str,
        tool_input: &Value,
    ) -> (String, bool) {
        if let Some(ref registry) = self.mcp_registry {
            if registry.is_mcp_tool(tool_name).await {
                let result = registry.execute_tool(tool_name, tool_input.clone()).await;
                match result {
                    Ok(val) => (serde_json::to_string_pretty(&val).unwrap_or_default(), false),
                    Err(e) => (format!("Error: {}", e), true),
                }
            } else {
                let cwd = self.get_workspace_cwd().to_string();
                let result = crate::tools::execute_tool_async(tool_name, tool_input.clone(), &cwd).await;
                match &result {
                    Ok(val) => (serde_json::to_string_pretty(val).unwrap_or_default(), false),
                    Err(e) => (format!("Error: {}", e), true),
                }
            }
        } else {
            let cwd = self.get_workspace_cwd().to_string();
            let result = crate::tools::execute_tool_async(tool_name, tool_input.clone(), &cwd).await;
            match &result {
                Ok(val) => (serde_json::to_string_pretty(val).unwrap_or_default(), false),
                Err(e) => (format!("Error: {}", e), true),
            }
        }
    }

    async fn execute_ask_user_question(&mut self, tool_input: &Value) -> (Value, String, bool) {
        let question = tool_input["question"].as_str().unwrap_or("").to_string();
        let options: Vec<String> = tool_input["options"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|opt| opt["label"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let _ = self.event_tx.send(EngineEvent::AskUser {
            question: question.clone(),
            options: options.clone(),
        }).await;

        let conv_id = self.conv_id.clone().unwrap_or_default();
        let (tx, rx) = oneshot::channel::<String>();
        {
            let mut waiters = self.answer_waiters.lock().await;
            waiters.insert(conv_id.clone(), tx);
        }

        match rx.await {
            Ok(answer) => {
                let result = serde_json::json!({
                    "type": "ask_user_question",
                    "question": question,
                    "answer": answer,
                    "content": answer,
                    "requires_user_input": false
                });
                (tool_input.clone(), serde_json::to_string_pretty(&result).unwrap_or_default(), false)
            }
            Err(_) => {
                let result = serde_json::json!({
                    "type": "ask_user_question",
                    "question": question,
                    "content": "User did not respond",
                    "is_error": true
                });
                (tool_input.clone(), serde_json::to_string_pretty(&result).unwrap_or_default(), true)
            }
        }
    }

    async fn execute_ask_user_confirmation(&mut self, tool_name: &str, tool_input: &Value, message: &str) -> (Value, String, bool) {
        let question = format!("{}:\n\nTool: {}\nInput: {}\n\nDo you want to proceed?", 
            message, tool_name, serde_json::to_string_pretty(tool_input).unwrap_or_default());
        let options = vec!["Yes".to_string(), "No".to_string()];

        let _ = self.event_tx.send(EngineEvent::AskUser {
            question: question.clone(),
            options: options.clone(),
        }).await;

        let conv_id = self.conv_id.clone().unwrap_or_default();
        let (tx, rx) = oneshot::channel::<String>();
        {
            let mut waiters = self.answer_waiters.lock().await;
            waiters.insert(conv_id.clone(), tx);
        }

        match rx.await {
            Ok(answer) => {
                let answer_lower = answer.trim().to_lowercase();
                let is_allowed = answer_lower == "yes" || answer_lower == "allow" || answer_lower == "ok" || answer_lower == "continue" || answer_lower == "proceed";

                if is_allowed {
                    if let Some(ref pm) = self.permission_manager {
                        pm.confirm_permission(&conv_id, tool_name);
                    }

                    let result = self.execute_tool_call_unchecked(tool_name, tool_input, "").await;
                    result
                } else {
                    (tool_input.clone(), "User cancelled the operation".to_string(), true)
                }
            }
            Err(_) => {
                (tool_input.clone(), "User did not respond, operation cancelled".to_string(), true)
            }
        }
    }

    async fn execute_tool_call_unchecked(
        &mut self,
        tool_name: &str,
        tool_input: &Value,
        _tool_use_id: &str,
    ) -> (Value, String, bool) {
        let output_str;
        let is_error;

        if let Some(ref registry) = self.mcp_registry {
            if registry.is_mcp_tool(tool_name).await {
                let result = registry.execute_tool(tool_name, tool_input.clone()).await;
                output_str = match &result {
                    Ok(val) => serde_json::to_string_pretty(val).unwrap_or_default(),
                    Err(e) => format!("Error: {}", e),
                };
                is_error = result.is_err();
            } else {
                let cwd = self.get_workspace_cwd().to_string();
                let result = crate::tools::execute_tool_async(tool_name, tool_input.clone(), &cwd).await;
                output_str = match &result {
                    Ok(val) => serde_json::to_string_pretty(val).unwrap_or_default(),
                    Err(e) => format!("Error: {}", e),
                };
                is_error = result.is_err();
            }
        } else {
            let cwd = self.get_workspace_cwd().to_string();
            let result = crate::tools::execute_tool_async(tool_name, tool_input.clone(), &cwd).await;
            output_str = match &result {
                Ok(val) => serde_json::to_string_pretty(val).unwrap_or_default(),
                Err(e) => format!("Error: {}", e),
            };
            is_error = result.is_err();
        }

        (tool_input.clone(), output_str, is_error)
    }

    async fn execute_anthropic_loop(&mut self) -> Result<(String, Option<String>)> {
        let mut conversation_messages: Vec<AnthropicMessage> = self.build_anthropic_messages();
        let tools: Vec<_> = get_tool_definitions().into_iter()
            .filter(|t| self.web_search_enabled || t.name != "WebSearch")
            .collect();
        let mut full_text = String::new();
        let mut stop_reason = None;

        for iteration in 0..self.max_tool_iterations {
            self.streaming_tool_args.clear();
            // 建立流式连接带自动重试：429/过载/超时等瞬时错误按 2s/5s/10s 退避重试；
            // 全部失败时必须发出 Error 事件，否则前端只见输出"静默停止"
            let mut stream = {
                let waits = [2u64, 5, 10];
                let mut attempt = 0usize;
                loop {
                    match self.anthropic_client
                        .send_message_stream(
                            &self.provider,
                            conversation_messages.clone(),
                            self.system_prompt.as_deref(),
                            tools.clone(),
                            self.max_tokens,
                            self.reasoning_effort.as_deref(),
                            self.extended_thinking,
                        )
                        .await
                    {
                        Ok(s) => break s,
                        Err(e) => {
                            let retryable = crate::orchestration::is_retryable_error(&e.to_string());
                            if retryable && attempt < waits.len() {
                                let w = waits[attempt];
                                attempt += 1;
                                tracing::warn!(target: "tool_loop", "Anthropic provider request failed ({}), retry {}/{} in {}s", e, attempt, waits.len(), w);
                                tokio::time::sleep(std::time::Duration::from_secs(w)).await;
                            } else {
                                let suffix = if retryable { format!("（已自动重试 {} 次）", attempt) } else { String::new() };
                                let _ = self.event_tx.send(EngineEvent::Error(format!("模型供应商请求失败: {}{}", e, suffix))).await;
                                return Err(e);
                            }
                        }
                    }
                }
            };

            let mut sse_buffer = String::new();
            let mut has_tool_use = false;
            let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
            let mut current_text = String::new();
            let mut current_thinking = String::new();
            let mut current_tool_use_id: Option<String> = None;
            let mut current_tool_name: Option<String> = None;
            let mut tool_results: Vec<AnthropicMessage> = Vec::new();
            let mut pending_tools: Vec<PendingToolCall> = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                if self.event_tx.is_closed() {
                    break;
                }
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = self.event_tx.send(EngineEvent::Error(format!("Stream error: {}", e))).await;
                        drop(stream);
                        break;
                    }
                };

                sse_buffer.push_str(&chunk);
                let consumed = consume_sse_payloads(&sse_buffer);
                sse_buffer = consumed.remainder;

                for payload in &consumed.payloads {
                    let event: Value = match serde_json::from_str(payload) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    match event_type {
                        "message_start" => {
                            if let Some(message) = event.get("message") {
                                if let Some(model) = message.get("model").and_then(|m| m.as_str()) {
                                    let _ = self.event_tx.send(EngineEvent::MessageStart {
                                        model: model.to_string(),
                                    }).await;
                                }
                                if let Some(usage) = message.get("usage") {
                                    let _ = self.event_tx.send(EngineEvent::Usage(usage.clone())).await;
                                }
                            }
                        }
                        "content_block_start" => {
                            let block = event.get("content_block");
                            let block_type = block.and_then(|b| b.get("type")).and_then(|t| t.as_str()).unwrap_or("");

                            match block_type {
                                "text" => {
                                    current_text.clear();
                                }
                                "thinking" => {
                                    current_thinking.clear();
                                }
                                "tool_use" => {
                                    has_tool_use = true;
                                    let id = block.and_then(|b| b.get("id")).and_then(|i| i.as_str()).unwrap_or("").to_string();
                                    let name = block.and_then(|b| b.get("name")).and_then(|n| n.as_str()).unwrap_or("").to_string();
                                    current_tool_use_id = Some(id.clone());
                                    current_tool_name = Some(name.clone());

                                    let _ = self.event_tx.send(EngineEvent::ToolUseStart {
                                        tool_use_id: id,
                                        tool_name: name,
                                        tool_input: json!({}),
                                        text_before: full_text.clone(),
                                    }).await;
                                }
                                _ => {}
                            }
                        }
                        "content_block_delta" => {
                            let delta = event.get("delta");
                            let delta_type = delta.and_then(|d| d.get("type")).and_then(|t| t.as_str()).unwrap_or("");

                            match delta_type {
                                "text_delta" => {
                                    let text = delta.and_then(|d| d.get("text")).and_then(|t| t.as_str()).unwrap_or("");
                                    if !text.is_empty() {
                                        current_text.push_str(text);
                                        full_text.push_str(text);
                                        let _ = self.event_tx.send(EngineEvent::Text(text.to_string())).await;
                                    }
                                }
                                "thinking_delta" => {
                                    let thinking = delta.and_then(|d| d.get("thinking")).and_then(|t| t.as_str()).unwrap_or("");
                                    if !thinking.is_empty() {
                                        current_thinking.push_str(thinking);
                                        let _ = self.event_tx.send(EngineEvent::Thinking(thinking.to_string())).await;
                                    }
                                }
                                "input_json_delta" => {
                                    let partial = delta.and_then(|d| d.get("partial_json")).and_then(|p| p.as_str()).unwrap_or("");
                                    if !partial.is_empty() {
                                        if let (Some(ref id), Some(ref name)) = (&current_tool_use_id, &current_tool_name) {
                                            self.handle_streaming_tool_arg_delta(id, name, partial);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        "content_block_stop" => {
                            let _index = event.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                            if !current_text.is_empty() {
                                assistant_blocks.push(ContentBlock::Text { text: current_text.clone() });
                                current_text.clear();
                            } else if !current_thinking.is_empty() {
                                assistant_blocks.push(ContentBlock::Thinking {
                                    thinking: current_thinking.clone(),
                                    signature: None,
                                });
                                current_thinking.clear();
                            } else if current_tool_use_id.is_some() {
                                let id = current_tool_use_id.clone().unwrap_or_default();
                                let name = current_tool_name.clone().unwrap_or_default();
                                let input = self.finalize_streaming_tool_args(&id);

                                // Collect tool call for parallel execution (don't execute yet)
                                pending_tools.push(PendingToolCall {
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                });

                                assistant_blocks.push(ContentBlock::ToolUse {
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                });

                                current_tool_use_id = None;
                                current_tool_name = None;
                            }
                        }
                        "message_delta" => {
                            let delta = event.get("delta");
                            let sr = delta.and_then(|d| d.get("stop_reason")).and_then(|s| s.as_str()).map(String::from);
                            if sr.is_some() {
                                stop_reason = sr.clone();
                                let _ = self.event_tx.send(EngineEvent::MessageDelta {
                                    stop_reason: sr,
                                }).await;
                            }
                            if let Some(usage) = event.get("usage") {
                                let _ = self.event_tx.send(EngineEvent::Usage(usage.clone())).await;
                            }
                        }
                        "message_stop" => {}
                        "ping" => {}
                        _ => {}
                    }
                }
            }

            if !sse_buffer.is_empty() {
                let consumed = consume_sse_payloads(&sse_buffer);
                for payload in &consumed.payloads {
                    let event: Value = match serde_json::from_str(payload) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if event_type == "message_delta" {
                        let delta = event.get("delta");
                        let sr = delta.and_then(|d| d.get("stop_reason")).and_then(|s| s.as_str()).map(String::from);
                        if sr.is_some() {
                            stop_reason = sr.clone();
                            let _ = self.event_tx.send(EngineEvent::MessageDelta {
                                stop_reason: sr,
                            }).await;
                        }
                    }
                }
            }

            if has_tool_use {
                conversation_messages.push(AnthropicMessage {
                    role: "assistant".to_string(),
                    content: AnthropicContent::Blocks(assistant_blocks),
                });

                // Execute pending tools — in parallel if multiple
                if !pending_tools.is_empty() {
                    let num_tools = pending_tools.len();
                    tracing::debug!(target: "tool_loop", "Executing {} tool calls{}", num_tools,
                        if num_tools > 1 { " in parallel" } else { "" });

                    if num_tools == 1 {
                        // Single tool — execute directly
                        let tool = pending_tools.remove(0);
                        let (_id, output, is_error) = self.execute_tool_call(&tool.name, &tool.input, &tool.id).await;
                        self.completed_tool_calls.push((
                            tool.id.clone(),
                            tool.name.clone(),
                            serde_json::to_string(&tool.input).unwrap_or_default(),
                            output.clone(),
                            is_error,
                        ));

                        let _ = self.event_tx.send(EngineEvent::ToolUseDone {
                            tool_use_id: tool.id.clone(),
                            tool_name: tool.name.clone(),
                            tool_input: tool.input.clone(),
                            output: output.clone(),
                            is_error,
                        }).await;

                        tool_results.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: AnthropicContent::Blocks(vec![ContentBlock::ToolResult {
                                tool_use_id: tool.id,
                                content: Self::strip_image_base64(&output),
                            is_error: Some(is_error),
                        }]),
                    });
                        } else {
                        // Multiple tools — execute in parallel with panic protection
                        let event_tx = self.event_tx.clone();
                        let mcp_registry = self.mcp_registry.clone();
                        let workspace_cwd = self.workspace_cwd.clone();
                        let permission_manager = self.permission_manager.clone();
                        let answer_waiters = self.answer_waiters.clone();
                        let conv_id = self.conv_id.clone();
                        let retry_config = self.retry_config.clone();

                        // Spawn each tool in its own task so a panic in one
                        // doesn't kill the whole tool loop.
                        let mut handles = Vec::new();
                        for tool in pending_tools.drain(..) {
                            let event_tx = event_tx.clone();
                            let mcp_registry = mcp_registry.clone();
                            let workspace_cwd = workspace_cwd.clone();
                            let permission_manager = permission_manager.clone();
                            let answer_waiters = answer_waiters.clone();
                            let conv_id = conv_id.clone();
                            let retry_config = retry_config.clone();

                            let handle = tokio::spawn(async move {
                                let max_attempts = retry_config.max_retries + 1;
                                let mut last_output = String::new();
                                let mut last_is_error = false;

                                for attempt in 0..max_attempts {
                                    let (output, is_error) = execute_tool_static(
                                        &tool.name, &tool.input, &workspace_cwd,
                                        mcp_registry.as_ref(), permission_manager.as_ref(),
                                        &conv_id, &answer_waiters,
                                    ).await;

                                    if !is_error {
                                        return (tool.id, tool.name, tool.input, output, false);
                                    }

                                    let tool_err = crate::tools::retry::ToolError::classify(&output);
                                    if !tool_err.is_retryable() || attempt + 1 >= max_attempts {
                                        return (tool.id, tool.name, tool.input, output, true);
                                    }

                                    let delay = retry_config.delay_for_attempt(attempt);
                                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                                    last_output = output;
                                    last_is_error = is_error;
                                }
                                (tool.id, tool.name, tool.input, last_output, last_is_error)
                            });
                            handles.push(handle);
                        }

                        // Collect results — catch any panicking tasks
                        for handle in handles {
                            match handle.await {
                                Ok((id, name, input, output, is_error)) => {
                                    self.completed_tool_calls.push((
                                        id.clone(),
                                        name.clone(),
                                        serde_json::to_string(&input).unwrap_or_default(),
                                        output.clone(),
                                        is_error,
                                    ));
                                    let _ = event_tx.send(EngineEvent::ToolUseDone {
                                        tool_use_id: id.clone(),
                                        tool_name: name.clone(),
                                        tool_input: input.clone(),
                                        output: output.clone(),
                                        is_error,
                                    }).await;

                                    tool_results.push(AnthropicMessage {
                                        role: "user".to_string(),
                                        content: AnthropicContent::Blocks(vec![ContentBlock::ToolResult {
                                            tool_use_id: id,
                                            content: Self::strip_image_base64(&output),
                                            is_error: Some(is_error),
                                        }]),
                                    });
                                }
                                Err(join_err) => {
                                    tracing::error!(target: "tool_loop",
                                        "Tool task panicked: {}", join_err);
                                    // Continue with other tools instead of crashing
                                }
                            }
                        }
                    }
                }

                for tool_result_msg in tool_results.drain(..) {
                    conversation_messages.push(tool_result_msg);
                }
            } else {
                break;
            }

            if iteration == self.max_tool_iterations - 1 {
                let _ = self.event_tx.send(EngineEvent::Error("Max tool iterations reached".to_string())).await;
                break;
            }
        }

        Ok((full_text, stop_reason))
    }

    async fn execute_openai_loop(&mut self) -> Result<(String, Option<String>)> {
        let mut conversation_messages: Vec<OpenAIMessage> = self.build_openai_messages();
        let tools: Vec<_> = get_tool_definitions().into_iter()
            .filter(|t| self.web_search_enabled || t.name != "WebSearch")
            .collect();
        let mut full_text = String::new();
        let mut stop_reason = None;

    for iteration in 0..self.max_tool_iterations {
        self.streaming_tool_args.clear();
        tracing::info!(target: "tool_loop", "[openai-iter {}] msgs={}", iteration, conversation_messages.len());
        // 同 Anthropic 循环：自动重试 + 失败时发出 Error 事件
        let mut sse_buffer = String::new();
            let mut has_tool_calls = false;
            let mut assistant_content: Option<OpenAIContent> = None;
            let mut assistant_reasoning: Option<String> = None;
            let mut assistant_tool_calls: Vec<crate::native_engine::openai_client::OpenAIToolCall> = Vec::new();
            let mut tool_results: Vec<OpenAIMessage> = Vec::new();
            // 并行工具调用累积。寻址必须统一：id 块创建的条目要记录供应商 index，
            // 后续参数块（无 id）先按 index 匹配已有条目——否则 id 块在 pos=0 而参数块
            // 声明 index=1 时会补出幻影条目，参数写错位，执行时变成空参数
            let mut openai_tool_calls: Vec<(String, String, String, Option<u64>)> = Vec::new(); // (id, name, args, provider_index)

        let mut stream: Pin<Box<dyn Stream<Item = Result<String>> + Send>> = if self.streaming {
            let waits = [2u64, 5, 10];
            let mut attempt = 0usize;
            let s = loop {
                match self.openai_client
                    .send_message_stream(
                        &self.provider,
                        conversation_messages.clone(),
                        self.system_prompt.as_deref(),
                        tools.clone(),
                        self.max_tokens,
                        self.reasoning_effort.as_deref(),
                        self.extended_thinking,
                    )
                    .await
                {
                    Ok(s) => break s,
                    Err(e) => {
                        let retryable = crate::orchestration::is_retryable_error(&e.to_string());
                        if retryable && attempt < waits.len() {
                            let w = waits[attempt];
                            attempt += 1;
                            tracing::warn!(target: "tool_loop", "OpenAI provider request failed ({}), retry {}/{} in {}s", e, attempt, waits.len(), w);
                            tokio::time::sleep(std::time::Duration::from_secs(w)).await;
                        } else {
                            let suffix = if retryable { format!("（已自动重试 {} 次）", attempt) } else { String::new() };
                            let _ = self.event_tx.send(EngineEvent::Error(format!("模型供应商请求失败: {}{}", e, suffix))).await;
                            return Err(e);
                        }
                    }
                }
            };
            s
        } else {
            // 非流式：一次拿到完整响应，再模拟为逐块事件，保证工具调用路径的稳定性
            let waits = [2u64, 5, 10];
            let mut attempt = 0usize;
            let resp = loop {
                match self.openai_client
                    .send_message(
                        &self.provider,
                        conversation_messages.clone(),
                        self.system_prompt.as_deref(),
                        tools.clone(),
                        self.max_tokens,
                        self.reasoning_effort.as_deref(),
                        self.extended_thinking,
                    )
                    .await
                {
                    Ok(r) => break r,
                    Err(e) => {
                        let retryable = crate::orchestration::is_retryable_error(&e.to_string());
                        if retryable && attempt < waits.len() {
                            let w = waits[attempt];
                            attempt += 1;
                            tracing::warn!(target: "tool_loop", "OpenAI provider request (non-stream) failed ({}), retry {}/{} in {}s", e, attempt, waits.len(), w);
                            tokio::time::sleep(std::time::Duration::from_secs(w)).await;
                        } else {
                            let suffix = if retryable { format!("（已自动重试 {} 次）", attempt) } else { String::new() };
                            let _ = self.event_tx.send(EngineEvent::Error(format!("模型供应商请求失败: {}{}", e, suffix))).await;
                            return Err(e);
                        }
                    }
                }
            };

            let _ = self.event_tx.send(EngineEvent::MessageStart {
                model: self.provider.model.id.clone(),
            }).await;

            if let Some(choice) = resp.choices.first() {
                let msg = &choice.message;
                if let OpenAIContent::Text(t) = &msg.content {
                    if !t.is_empty() {
                        assistant_content = Some(OpenAIContent::Text(t.clone()));
                        // 模拟流式：按小块发送 Text，前端实现逐字/逐块出字
                        const CHUNK: usize = 6;
                        for c in t.chars().collect::<Vec<_>>().chunks(CHUNK) {
                            let piece: String = c.iter().collect();
                            full_text.push_str(&piece);
                            let _ = self.event_tx.send(EngineEvent::Text(piece)).await;
                        }
                    } else {
                        assistant_content = Some(OpenAIContent::Text(String::new()));
                    }
                }
                if let Some(r) = &msg.reasoning_content {
                    if !r.is_empty() {
                        assistant_reasoning = Some(r.clone());
                        let _ = self.event_tx.send(EngineEvent::Thinking(r.clone())).await;
                    }
                }
                if let Some(tcs) = &msg.tool_calls {
                    if !tcs.is_empty() {
                        has_tool_calls = true;
                        for (i, tc) in tcs.iter().enumerate() {
                            openai_tool_calls.push((
                                tc.id.clone(),
                                tc.function.name.clone(),
                                tc.function.arguments.clone(),
                                Some(i as u64),
                            ));
                        }
                    }
                }
                if let Some(fr) = &choice.finish_reason {
                    if fr != "tool_calls" && stop_reason.is_none() {
                        stop_reason = Some(fr.clone());
                        let _ = self.event_tx.send(EngineEvent::MessageDelta {
                            stop_reason: Some(fr.clone()),
                        }).await;
                    }
                }
            }
            if let Some(usage) = &resp.usage {
                let _ = self.event_tx.send(EngineEvent::Usage(
                    serde_json::to_value(usage).unwrap_or_else(|_| json!({}))
                )).await;
            }
            Box::pin(futures::stream::empty()) as Pin<Box<dyn Stream<Item = Result<String>> + Send>>
        };

            while let Some(chunk_result) = stream.next().await {
                if self.event_tx.is_closed() {
                    break;
                }
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = self.event_tx.send(EngineEvent::Error(format!("Stream error: {}", e))).await;
                        drop(stream);
                        break;
                    }
                };

                sse_buffer.push_str(&chunk);
                let consumed = consume_sse_payloads(&sse_buffer);
                sse_buffer = consumed.remainder;

                for payload in &consumed.payloads {
                    if payload == "[DONE]" {
                        continue;
                    }

                    let event: Value = match serde_json::from_str(payload) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let choices = match event.get("choices").and_then(|c| c.as_array()) {
                        Some(c) => c,
                        None => continue,
                    };

                    for choice in choices {
                        let delta = match choice.get("delta") {
                            Some(d) => d,
                            None => continue,
                        };

                        if let Some(role) = delta.get("role").and_then(|r| r.as_str()) {
                            if role == "assistant" {
                                let _ = self.event_tx.send(EngineEvent::MessageStart {
                                    model: self.provider.model.id.clone(),
                                }).await;
                            }
                        }

                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                            if !content.is_empty() {
                                full_text.push_str(content);
                                let _ = self.event_tx.send(EngineEvent::Text(content.to_string())).await;
                                match &assistant_content {
                                    None => {
                                        assistant_content = Some(OpenAIContent::Text(content.to_string()));
                                    }
                                    Some(OpenAIContent::Text(existing)) => {
                                        assistant_content = Some(OpenAIContent::Text(format!("{}{}", existing, content)));
                                    }
                                    Some(OpenAIContent::Multi(_)) => {}
                                }
                            }
                        }

                        if let Some(reasoning) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
                            if !reasoning.is_empty() {
                                match &mut assistant_reasoning {
                                    None => assistant_reasoning = Some(reasoning.to_string()),
                                    Some(r) => r.push_str(reasoning),
                                }
                                let _ = self.event_tx.send(EngineEvent::Thinking(reasoning.to_string())).await;
                            }
                        }

                        // Also handle "thinking" field (some providers use this instead of reasoning_content)
                        if let Some(thinking) = delta.get("thinking").and_then(|t| t.as_str()) {
                            if !thinking.is_empty() {
                                match &mut assistant_reasoning {
                                    None => assistant_reasoning = Some(thinking.to_string()),
                                    Some(r) => r.push_str(thinking),
                                }
                                let _ = self.event_tx.send(EngineEvent::Thinking(thinking.to_string())).await;
                            }
                        }

                        if let Some(tool_calls_arr) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                            has_tool_calls = true;
                            for tc_delta in tool_calls_arr {
                                // index 可能是数字也可能是字符串（部分代理实现）
                                let index = tc_delta.get("index").and_then(|i| {
                                    i.as_u64().or_else(|| i.as_str().and_then(|s| s.parse::<u64>().ok()))
                                });
                                let delta_id = tc_delta.get("id").and_then(|i| i.as_str()).map(String::from);
                                let func = tc_delta.get("function");
                                let delta_name = func.and_then(|f| f.get("name")).and_then(|n| n.as_str()).map(String::from);
                                // arguments 规范是字符串，但部分供应商直接给 JSON 对象
                                let delta_args = func.and_then(|f| f.get("arguments"))
                                    .map(|a| if let Some(s) = a.as_str() { s.to_string() } else { a.to_string() })
                                    .unwrap_or_default();

                                // 定位或创建目标条目
                                let slot: Option<usize> = if let Some(ref id) = delta_id {
                                    if let Some(pos) = openai_tool_calls.iter().position(|c| &c.0 == id) {
                                        Some(pos)
                                    } else {
                                        openai_tool_calls.push((id.clone(), delta_name.clone().unwrap_or_default(), String::new(), index));
                                        Some(openai_tool_calls.len() - 1)
                                    }
                                } else if let Some(idx) = index {
                                    // 先匹配已记录该 index 的条目（id 块与参数块的 index 一致）
                                    if let Some(pos) = openai_tool_calls.iter().position(|c| c.3 == Some(idx)) {
                                        Some(pos)
                                    } else {
                                        while openai_tool_calls.len() <= idx as usize {
                                            openai_tool_calls.push((String::new(), String::new(), String::new(), Some(idx)));
                                        }
                                        Some(idx as usize)
                                    }
                                } else if delta_name.is_some() {
                                    openai_tool_calls.push((String::new(), delta_name.clone().unwrap_or_default(), String::new(), None));
                                    Some(openai_tool_calls.len() - 1)
                                } else if !openai_tool_calls.is_empty() {
                                    Some(openai_tool_calls.len() - 1)
                                } else {
                                    None
                                };

                                if let Some(pos) = slot {
                                    let entry = &mut openai_tool_calls[pos];
                                    if let Some(ref id) = delta_id { entry.0 = id.clone(); }
                                    if let Some(ref name) = delta_name { entry.1 = name.clone(); }
                                    if entry.3.is_none() { entry.3 = index; }
                                    entry.2.push_str(&delta_args);
                                    tracing::info!(target: "tool_loop", "[tc-delta] pos={} index={:?} id={:?} name={:?} args_len+={}", pos, index, delta_id, delta_name, delta_args.len());
                                }
                            }
                        }

                        if let Some(finish) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                            if finish != "tool_calls" {
                                stop_reason = Some(finish.to_string());
                                let _ = self.event_tx.send(EngineEvent::MessageDelta {
                                    stop_reason: Some(finish.to_string()),
                                }).await;
                            }
                        }
                    }

                    if let Some(usage) = event.get("usage") {
                        let _ = self.event_tx.send(EngineEvent::Usage(usage.clone())).await;
                    }
                }
            }

            if has_tool_calls {
                // 缺 id 的调用合成稳定 id（tool 结果与 assistant.tool_calls 需一一对应），
                // 空 name 的条目是 index 稀疏产生的空位，丢弃
                for (i, c) in openai_tool_calls.iter_mut().enumerate() {
                    if c.0.is_empty() { c.0 = format!("call_{}", i); }
                }
                openai_tool_calls.retain(|c| !c.1.is_empty());

                for (id, name, args_str, _) in openai_tool_calls.iter() {
                    tracing::info!(target: "tool_loop", "[tc-final] name='{}' args_len={} head={}", name, args_str.len(), args_str.chars().take(100).collect::<String>());
                    let mut input: Value = serde_json::from_str(args_str).unwrap_or_else(|_| {
                        recover_malformed_tool_input(name, args_str).unwrap_or(json!({}))
                    });
                    // 某些模型（特别是 MiniMax 系列）会把工具名作为 key 写进 arguments
                    // JSON（例：{"name":"Bash","command":"..."}），污染 input 字段。
                    // 解析成功时剥掉它，工具按 schema 取参数即可。
                    if let Some(obj) = input.as_object_mut() {
                        if obj.get("name").and_then(|v| v.as_str()) == Some(name.as_str()) {
                            obj.remove("name");
                            tracing::info!(target: "tool_loop", "[tc-fix] stripped extraneous 'name' key from {}", name);
                        }
                    }

                    let _ = self.event_tx.send(EngineEvent::ToolUseStart {
                        tool_use_id: id.clone(),
                        tool_name: name.clone(),
                        tool_input: input.clone(),
                        text_before: full_text.clone(),
                    }).await;

                    let (.., output_str, is_error) = self.execute_tool_call(name, &input, id).await;
                    self.completed_tool_calls.push((
                        id.clone(),
                        name.clone(),
                        serde_json::to_string(&input).unwrap_or_default(),
                        output_str.clone(),
                        is_error,
                    ));

                    let _ = self.event_tx.send(EngineEvent::ToolUseDone {
                        tool_use_id: id.clone(),
                        tool_name: name.clone(),
                        tool_input: input.clone(),
                        output: output_str.clone(),
                        is_error,
                    }).await;

                    assistant_tool_calls.push(crate::native_engine::openai_client::OpenAIToolCall {
                        id: id.clone(),
                        call_type: "function".to_string(),
                        function: crate::native_engine::openai_client::FunctionCall {
                            name: name.clone(),
                            arguments: args_str.clone(),
                        },
                    });

                    tool_results.push(OpenAIMessage {
                        role: "tool".to_string(),
                        content: self.tool_output_content(&output_str),
                        tool_calls: None,
                        tool_call_id: Some(id.clone()),
                        reasoning_content: None,
                    });
                }

                tracing::info!(target: "tool_loop", "[tc-round] feeding back: assistant tool_calls={}, tool_results={}", assistant_tool_calls.len(), tool_results.len());

                conversation_messages.push(OpenAIMessage {
                    role: "assistant".to_string(),
                    content: assistant_content.unwrap_or(OpenAIContent::Text(String::new())),
                    tool_calls: Some(assistant_tool_calls),
                    tool_call_id: None,
                    reasoning_content: assistant_reasoning,
                });

                for tool_result_msg in tool_results {
                    conversation_messages.push(tool_result_msg);
                }
            } else {
                break;
            }

            if iteration == self.max_tool_iterations - 1 {
                let _ = self.event_tx.send(EngineEvent::Error("Max tool iterations reached".to_string())).await;
                break;
            }
        }

        Ok((full_text, stop_reason))
    }

    fn build_anthropic_messages(&self) -> Vec<AnthropicMessage> {
        let window = self.windowed_history();
        window.iter().filter_map(|msg| {
            let role = msg.get("role")?.as_str()?;
            let content = msg.get("content")?;

            let anthropic_content = if content.is_string() {
                AnthropicContent::Text(content.as_str()?.to_string())
            } else if content.is_array() {
                let blocks: Vec<ContentBlock> = content.as_array()?.iter().filter_map(|block| {
                    let block_type = block.get("type")?.as_str()?;
                    match block_type {
                        "text" => {
                            let text = block.get("text")?.as_str()?.to_string();
                            Some(ContentBlock::Text { text })
                        }
                        "image" => {
                            let source = block.get("source")?;
                            Some(ContentBlock::Image {
                                source: crate::native_engine::anthropic_client::ImageSource {
                                    source_type: source.get("type")?.as_str()?.to_string(),
                                    media_type: source.get("media_type")?.as_str()?.to_string(),
                                    data: source.get("data")?.as_str()?.to_string(),
                                },
                            })
                        }
                        "tool_result" => {
                            let raw = block.get("content")?.as_str()?.to_string();
                            Some(ContentBlock::ToolResult {
                                tool_use_id: block.get("tool_use_id")?.as_str()?.to_string(),
                                content: Self::strip_image_base64(&raw),
                                is_error: block.get("is_error").and_then(|v| v.as_bool()),
                            })
                        }
                        _ => None,
                    }
                }).collect();
                AnthropicContent::Blocks(blocks)
            } else {
                return None;
            };

            Some(AnthropicMessage {
                role: role.to_string(),
                content: anthropic_content,
            })
        }).collect()
    }

    /// 历史窗口化：从后往前保留消息，直到内容字符累计超过上限为止。
    /// 返回截取窗口的引用切片。
    fn windowed_history(&self) -> Vec<&serde_json::Value> {
        // 阈值按模型上下文窗口推导：留 40% 余量给输出，token→char 粗算 ×2，保底 60k
        let ctx_tokens = crate::native_engine::provider_manager::get_default_context_size(&self.provider.model.id);
        let max_chars = (((ctx_tokens as f64) * 0.6 * 2.0) as usize).max(60_000);
        let total: usize = self.messages.iter().map(|m| {
            m.get("content").and_then(|c| c.as_str()).unwrap_or("").chars().count()
        }).sum();
        if total <= max_chars {
            return self.messages.iter().collect();
        }
        let mut acc: usize = 0;
        let mut start = self.messages.len();
        for (i, m) in self.messages.iter().enumerate().rev() {
            let len = m.get("content").and_then(|c| c.as_str()).unwrap_or("").chars().count();
            acc += len;
            if acc > max_chars {
                break;
            }
            start = i;
        }
        if start >= self.messages.len() || start == 0 {
            start = self.messages.len().saturating_sub(1);
        }
        self.messages[start..].iter().collect()
    }

    /// 把 DB/前端历史重建为 OpenAI 工具调用结构。过长历史先经 windowed_history
    /// 窗口化，避免弱工具模型退化为纯文本叙述并降低内存占用（panic=abort 下内存
    /// 不足会直接闪退）。
    fn build_openai_messages(&self) -> Vec<OpenAIMessage> {
        let window = self.windowed_history();
        let mut out: Vec<OpenAIMessage> = Vec::new();
        for (msg_idx, msg) in window.iter().enumerate() {
            let role = match msg.get("role").and_then(|v| v.as_str()) {
                Some(r) => r.to_string(),
                None => continue,
            };
            // content 为 null（OpenAI 规范：纯工具调用轮次）或缺失时按空文本处理，
            // 绝不能因内容为空就整条丢弃，否则该轮的 tool_calls 一并被丢，
            // 弱工具模型将退化为纯文本叙述（TDD-2 覆盖）。
            let openai_content = match msg.get("content") {
                Some(c) if c.is_string() => OpenAIContent::Text(c.as_str().unwrap_or_default().to_string()),
                Some(c) if c.is_array() => {
                    let parts: Vec<crate::native_engine::openai_client::OpenAIContentPart> =
                        c.as_array().map(|arr| arr.iter().filter_map(|part| {
                            let part_type = part.get("type")?.as_str()?;
                            match part_type {
                                "text" => {
                                    Some(crate::native_engine::openai_client::OpenAIContentPart::Text {
                                        text: part.get("text")?.as_str()?.to_string(),
                                    })
                                }
                                "image_url" => {
                                    let url_obj = part.get("image_url")?;
                                    Some(crate::native_engine::openai_client::OpenAIContentPart::Image {
                                        image_url: crate::native_engine::openai_client::ImageUrl {
                                            url: url_obj.get("url")?.as_str()?.to_string(),
                                        },
                                    })
                                }
                                _ => None,
                            }
                        }).collect()).unwrap_or_default();
                    OpenAIContent::Multi(parts)
                }
                Some(c) if c.is_null() => OpenAIContent::Text(String::new()),
                None => OpenAIContent::Text(String::new()),
                _ => continue,
            };

            // 标准 OpenAI 形态的 tool_calls（含 function 字段）直接透传
            let std_tool_calls: Option<Vec<crate::native_engine::openai_client::OpenAIToolCall>> = msg
                .get("tool_calls")
                .and_then(|tc| serde_json::from_value(tc.clone()).ok());

            // 历史轮次的工具调用重建：前端历史里工具记录以 toolCalls 卡片
            // （{id,name,input,result}）或 DB 形态 tool_calls（{id,name,input,output}）
            // 存在，此前被静默丢弃，弱工具调用模型（如 MiniMax-M3）只看到
            // "纯文本叙述的执行过程"，会模仿叙述而不是发起 tool_calls。
            // 必须重建成 assistant.tool_calls + role:"tool" 结果的标准结构。
            let mut hist_calls: Vec<crate::native_engine::openai_client::OpenAIToolCall> = Vec::new();
            let mut hist_results: Vec<(String, String)> = Vec::new();
            let mut hist_items: Vec<&serde_json::Value> = Vec::new();
            if let Some(arr) = msg.get("toolCalls").and_then(|v| v.as_array()) {
                hist_items.extend(arr.iter());
            }
            if std_tool_calls.is_none() {
                if let Some(arr) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                    hist_items.extend(arr.iter());
                }
            }
            for (i, item) in hist_items.iter().enumerate() {
                let name = match item.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                let id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("call_hist_{}_{}", msg_idx, i));
                let input_val = item.get("input").cloned().unwrap_or(serde_json::json!({}));
                let args = serde_json::to_string(&input_val).unwrap_or_else(|_| "{}".to_string());
                let output = item
                    .get("result")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("output").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                hist_calls.push(crate::native_engine::openai_client::OpenAIToolCall {
                    id: id.clone(),
                    call_type: "function".to_string(),
                    function: crate::native_engine::openai_client::FunctionCall {
                        name: name.to_string(),
                        arguments: args,
                    },
                });
                hist_results.push((id, output));
            }

            let tool_calls = std_tool_calls.or_else(|| {
                if hist_calls.is_empty() { None } else { Some(hist_calls) }
            });

            let tool_call_id = msg.get("tool_call_id").and_then(|id| id.as_str()).map(String::from);
            let reasoning_content = msg.get("reasoning_content").and_then(|r| r.as_str()).map(String::from);

            out.push(OpenAIMessage {
                role,
                content: openai_content,
                tool_calls,
                tool_call_id,
                reasoning_content,
            });

            // 紧随其后回放工具结果（tool_call_id 与 assistant.tool_calls 一一对应）。
            // 输出截断，防止历史膨胀撑爆上下文。
            for (tcid, output) in hist_results {
                let cleaned = Self::strip_image_base64(&output);
                let out_str = if cleaned.trim().is_empty() {
                    "(no output)".to_string()
                } else {
                    Self::truncate_chars(&cleaned, 1500)
                };
                out.push(OpenAIMessage {
                    role: "tool".to_string(),
                    content: OpenAIContent::Text(out_str),
                    tool_calls: None,
                    tool_call_id: Some(tcid),
                    reasoning_content: None,
                });
            }
        }
        out
    }

    fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{}\n…[truncated]", head)
    }
}

    /// 从工具输出 JSON 中剥离 `__image_base64` 巨大字段：
    /// - 若输出是含 base64 的对象，去掉它并以说明字段占位；
    /// - 纯文本模型/文本工具结果：该方法保证不把海量 base64 当纯文本塞给模型
    ///   （浪费 token 且模型识别不了）。
    fn strip_image_base64(output: &str) -> String {
        if let Ok(v) = serde_json::from_str::<Value>(output) {
            if v.get("__image_base64").is_some() {
                if let Some(obj) = v.as_object() {
                    let mut copy = obj.clone();
                    copy.remove("__image_base64");
                    copy.insert(
                        "_image_notice".to_string(),
                        json!("截图已生成并在侧边栏实时面板可见；图像数据已从文本工具结果中省略。"),
                    );
                    if let Ok(s) = serde_json::to_string_pretty(&Value::Object(copy)) {
                        return s;
                    }
                }
            }
        }
        output.to_string()
    }

    /// 把工具输出转成发给模型的 OpenAI 内容块。若含 `__image_base64` 截图：
    /// - 支持视觉的模型：生成「文本(去图) + 图片(data:image/png;base64)」多内容块，让模型“看得见”；
    /// - 纯文本模型：仅返回剥离 base64 后的文本与说明，不给模型塞它无法处理的图片内容块。
    fn tool_output_content(&self, output: &str) -> OpenAIContent {
        let text_only = Self::strip_image_base64(output);
        if self.provider.model.supports_vision {
            if let Ok(v) = serde_json::from_str::<Value>(output) {
                if let Some(b64) = v.get("__image_base64").and_then(|b| b.as_str()) {
                    use crate::native_engine::openai_client::{ImageUrl, OpenAIContentPart};
                    return OpenAIContent::Multi(vec![
                        OpenAIContentPart::Text { text: text_only },
                        OpenAIContentPart::Image {
                            image_url: ImageUrl {
                                url: format!("data:image/png;base64,{}", b64),
                            },
                        },
                    ]);
                }
            }
        }
        OpenAIContent::Text(text_only)
    }

    fn get_workspace_cwd(&self) -> &str {
        &self.workspace_cwd
    }

    fn handle_streaming_tool_arg_delta(&mut self, tool_use_id: &str, tool_name: &str, delta: &str) {
        let prev_args = self.streaming_tool_args
            .get(tool_use_id)
            .map(|s| s.accumulated_args.clone())
            .unwrap_or_default();

        let merged = merge_tool_args(&prev_args, delta);

        let delta_to_emit = if merged.starts_with(&prev_args) && !prev_args.is_empty() {
            merged[prev_args.len()..].to_string()
        } else {
            delta.to_string()
        };

        self.streaming_tool_args.insert(
            tool_use_id.to_string(),
            StreamingToolCall {
                name: tool_name.to_string(),
                accumulated_args: merged,
            },
        );

        if !delta_to_emit.is_empty() {
            let _ = self.event_tx.try_send(EngineEvent::ToolArgDelta {
                tool_use_id: tool_use_id.to_string(),
                delta: delta_to_emit,
            });
        }
    }

    fn finalize_streaming_tool_args(&mut self, tool_use_id: &str) -> Value {
        if let Some(stc) = self.streaming_tool_args.remove(tool_use_id) {
            let parsed: Option<Value> = serde_json::from_str(&stc.accumulated_args).ok();
            parsed.or_else(|| recover_malformed_tool_input(&stc.name, &stc.accumulated_args))
                .unwrap_or(json!({}))
        } else {
            json!({})
        }
    }
}

/// Static tool execution function for parallel use (no &mut self required).
/// Checks MCP registry first, falls back to built-in tools.
async fn execute_tool_static(
    tool_name: &str,
    tool_input: &Value,
    workspace_cwd: &str,
    mcp_registry: Option<&Arc<McpToolRegistry>>,
    _permission_manager: Option<&Arc<PermissionManager>>,
    _conv_id: &Option<String>,
    _answer_waiters: &Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
) -> (String, bool) {
    if let Some(registry) = mcp_registry {
        if registry.is_mcp_tool(tool_name).await {
            let result = registry.execute_tool(tool_name, tool_input.clone()).await;
            return match result {
                Ok(val) => (serde_json::to_string_pretty(&val).unwrap_or_default(), false),
                Err(e) => (format!("Error: {}", e), true),
            };
        }
    }
    let result = crate::tools::execute_tool_async(tool_name, tool_input.clone(), workspace_cwd).await;
    match &result {
        Ok(val) => (serde_json::to_string_pretty(val).unwrap_or_default(), false),
        Err(e) => (format!("Error: {}", e), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_executor(messages: Vec<Value>) -> ToolLoopExecutor {
        let provider = ResolvedProvider {
            provider: crate::native_engine::provider_manager::Provider {
                id: "test".into(),
                name: "test".into(),
                base_url: "http://localhost:1/v1".into(),
                api_key: "k".into(),
                api_format: ApiFormat::OpenAI,
                models: vec![],
                enabled: true,
                web_search_strategy: None,
            },
            model: crate::native_engine::provider_manager::ModelConfig {
                id: "m".into(),
                name: "m".into(),
                enabled: true,
                max_tokens: None,
                context_window: None,
                context_size: None,
                supports_vision: false,
                supports_web_search: false,
            },
        };
        let (tx, _rx) = mpsc::channel(16);
        ToolLoopExecutor::new(provider, messages, None, 1024, tx, "C:/nonexistent".into())
    }

    fn text_of(m: &OpenAIMessage) -> String {
        match &m.content {
            OpenAIContent::Text(s) => s.clone(),
            OpenAIContent::Multi(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    crate::native_engine::openai_client::OpenAIContentPart::Text { text } => {
                        Some(text.clone())
                    }
                    _ => None,
                })
                .collect(),
        }
    }

    // TDD-1: 前端历史 toolCalls 卡片（{id,name,input,result}）必须重建为
    // assistant.tool_calls + role:"tool" 标准结构，否则弱工具模型会退化为纯文本叙述
    #[test]
    fn test_history_toolcalls_rebuilt_as_standard_struct() {
        let messages = vec![json!({
            "role": "assistant",
            "content": "我来读取文件",
            "toolCalls": [{
                "id": "call_1",
                "name": "Read",
                "input": {"file_path": "C:/x/a.js"},
                "result": "{\"content\": \"file body\"}"
            }]
        })];
        let out = make_executor(messages).build_openai_messages();
        assert_eq!(out.len(), 2, "应为 assistant + tool 两条消息");
        let a = &out[0];
        assert_eq!(a.role, "assistant");
        let tcs = a.tool_calls.as_ref().expect("必须有 tool_calls");
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "call_1");
        assert_eq!(tcs[0].call_type, "function");
        assert_eq!(tcs[0].function.name, "Read");
        let parsed_args: Value = serde_json::from_str(&tcs[0].function.arguments).unwrap();
        assert_eq!(parsed_args["file_path"], "C:/x/a.js");
        let t = &out[1];
        assert_eq!(t.role, "tool");
        assert_eq!(t.tool_call_id.as_deref(), Some("call_1"));
        assert!(text_of(t).contains("file body"));
    }

    // TDD-W: 过长历史必须被窗口化。旧会话累积大量消息后，弱工具模型（MiniMax-M3）
    // 会退化为纯文本叙述且内存占用暴涨（panic=abort 下直接闪退）。windowed_history
    // 应从后往前保留到字符上限为止，且最新消息必然保留。
    #[test]
    fn test_windowed_history_drops_long_tail() {
        let mut messages: Vec<Value> = Vec::new();
        // 前面塞 40 条超长（各 4000 字符 → content 共 160K，超过 120K 上限）
        for i in 0..40 {
            messages.push(json!({
                "role": if i % 2 == 0 { "user" } else { "assistant" },
                "content": "甲".repeat(4000),
            }));
        }
        // 最后一条短消息——必须被保留
        messages.push(json!({
            "role": "user",
            "content": "把 DingTalk 翻译成英文",
        }));

        let ex = make_executor(messages);
        let win = ex.windowed_history();
        assert!(
            win.len() < ex.messages.len(),
            "窗口化后条数应显著少于原始 {}，实际 {}",
            ex.messages.len(),
            win.len()
        );
        // 最新一条永远在窗口内
        let last_content = win.last().unwrap().get("content").and_then(|c| c.as_str()).unwrap();
        assert_eq!(last_content, "把 DingTalk 翻译成英文");
        // 窗口起始内容字符总和不超上限
        let total: usize = win.iter().map(|m| {
            m.get("content").and_then(|c| c.as_str()).unwrap_or("").chars().count()
        }).sum();
        let cap: usize = 120_000;
        assert!(total <= cap + 4000, "窗口内容应≤上限+单条，实际 {total}");
        // 且构建出的消息与窗口条数一致（首条 user 前的 head 被丢弃，无 panic）
        let out = ex.build_openai_messages();
        assert_eq!(out.len(), win.len(), "构建结果应等于窗口长度");
    }

    // TDD-2: DB 形态 tool_calls（{id,name,input,output}）同样重建
    #[test]
    fn test_db_form_tool_calls_with_output() {
        let messages = vec![json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call_db",
                "name": "Bash",
                "input": {"command": "ls"},
                "output": "{\"exit_code\": 0}"
            }]
        })];
        let out = make_executor(messages).build_openai_messages();
        assert_eq!(out.len(), 2);
        assert!(out[0].tool_calls.is_some());
        assert_eq!(out[0].tool_calls.as_ref().unwrap()[0].function.name, "Bash");
        assert_eq!(out[1].role, "tool");
        assert!(text_of(&out[1]).contains("exit_code"));
    }

    // TDD-3: 标准 OpenAI 形态（含 function 字段）直接透传，不重复重建
    #[test]
    fn test_std_openai_tool_calls_passthrough() {
        let messages = vec![json!({
            "role": "assistant",
            "content": "调用工具",
            "tool_calls": [{
                "id": "call_std",
                "type": "function",
                "function": {"name": "Grep", "arguments": "{\"pattern\":\"x\"}"}
            }]
        })];
        let out = make_executor(messages).build_openai_messages();
        assert_eq!(out.len(), 1, "标准形态只输出一条，不追加 tool 消息");
        let tcs = out[0].tool_calls.as_ref().unwrap();
        assert_eq!(tcs[0].id, "call_std");
        assert_eq!(tcs[0].function.name, "Grep");
        assert_eq!(tcs[0].function.arguments, "{\"pattern\":\"x\"}");
    }

    // TDD-4: 纯文本消息不受影响，不产生 tool_calls
    #[test]
    fn test_plain_messages_unchanged() {
        let messages = vec![
            json!({"role": "user", "content": "你好"}),
            json!({"role": "assistant", "content": "我在纯文本叙述执行过程"}),
        ];
        let out = make_executor(messages).build_openai_messages();
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|m| m.tool_calls.is_none()));
        assert_eq!(out[0].role, "user");
    }

    // TDD-5: 超长工具输出截断到 1500 字符，防止历史膨胀
    #[test]
    fn test_long_output_truncated() {
        let long_result = "x".repeat(5000);
        let messages = vec![json!({
            "role": "assistant",
            "content": "",
            "toolCalls": [{
                "id": "c",
                "name": "Bash",
                "input": {},
                "result": long_result
            }]
        })];
        let out = make_executor(messages).build_openai_messages();
        let t = text_of(&out[1]);
        assert!(t.chars().count() <= 1515, "截断后不应超长");
        assert!(t.contains("[truncated]"));
    }

    // TDD-6: 空输出给占位符，OpenAI 网关拒绝空 content 的 tool 消息
    #[test]
    fn test_empty_output_placeholder() {
        let messages = vec![json!({
            "role": "assistant",
            "content": "",
            "toolCalls": [{
                "id": "c2",
                "name": "Read",
                "input": {},
                "result": "   "
            }]
        })];
        let out = make_executor(messages).build_openai_messages();
        assert_eq!(text_of(&out[1]), "(no output)");
    }

    // TDD-7: toolCalls 与非标准 tool_calls 同时存在时合并
    #[test]
    fn test_mixed_camel_and_snake_merged() {
        let messages = vec![json!({
            "role": "assistant",
            "content": "",
            "toolCalls": [{
                "id": "c_a",
                "name": "Read",
                "input": {},
                "result": "r1"
            }],
            "tool_calls": [{
                "id": "c_b",
                "name": "Bash",
                "input": {},
                "output": "r2"
            }]
        })];
        let out = make_executor(messages).build_openai_messages();
        assert_eq!(out.len(), 3, "assistant + 2 条 tool 消息");
        let tcs = out[0].tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 2);
        assert_eq!(out[1].tool_call_id.as_deref(), Some("c_a"));
        assert_eq!(out[2].tool_call_id.as_deref(), Some("c_b"));
    }

    // TDD-8: 缺 id 的历史记录生成回退 id，不 panic
    #[test]
    fn test_missing_id_gets_fallback() {
        let messages = vec![json!({
            "role": "assistant",
            "content": "",
            "toolCalls": [{
                "name": "Read",
                "input": {},
                "result": "ok"
            }]
        })];
        let out = make_executor(messages).build_openai_messages();
        assert_eq!(out.len(), 2);
        assert!(out[1].tool_call_id.as_ref().unwrap().starts_with("call_hist_"));
    }

    // TDD-9: 真实工具执行链路——Write 落盘 + Read 读回（端到端验证工具层正常）
    #[tokio::test]
    async fn test_execute_tool_write_then_read_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("tdd_tool_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let cwd = tmp.to_string_lossy().to_string();
        let write_input = json!({"file_path": cwd.clone() + "/probe.txt", "content": "hello-tdd"});
        let (out, is_err) = execute_tool_static("Write", &write_input, &cwd, None, None, &None, &Default::default()).await;
        assert!(!is_err, "Write 不应报错: {}", out);
        let read_input = json!({"file_path": cwd.clone() + "/probe.txt"});
        let (out2, is_err2) = execute_tool_static("Read", &read_input, &cwd, None, None, &None, &Default::default()).await;
        assert!(!is_err2, "Read 不应报错: {}", out2);
        assert!(out2.contains("hello-tdd"), "应读回写入内容: {}", out2);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // TDD-10: Bash 工具真实执行
    #[tokio::test]
    async fn test_execute_tool_bash_runs() {
        let tmp = std::env::temp_dir().join(format!("tdd_bash_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let cwd = tmp.to_string_lossy().to_string();
        let input = json!({"command": "echo tdd-ok"});
        let (out, is_err) = execute_tool_static("Bash", &input, &cwd, None, None, &None, &Default::default()).await;
        assert!(!is_err, "Bash 不应报错: {}", out);
        assert!(out.contains("tdd-ok"), "echo 输出应包含标记: {}", out);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

