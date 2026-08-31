/// Agentic tool loop for MetaGPT actions.
/// LLM decides when to stop (returns text without tool_calls).
/// Safety: anti-stuck detection + total timeout + hard ceiling.
use anyhow::{anyhow, Result};
use crate::native_engine::provider_manager::{ApiFormat, ResolvedProvider};
use crate::orchestration::agent_loop::{get_agent_tool_definitions, execute_agent_tool};
use crate::orchestration::WorkflowEvent;
use serde_json::Value;
use crate::tools::ToolDefinition;
use std::sync::RwLock;

static PROGRESS_TX: RwLock<Option<tokio::sync::broadcast::Sender<WorkflowEvent>>> = RwLock::new(None);

pub fn set_progress_sender(tx: tokio::sync::broadcast::Sender<WorkflowEvent>) {
    if let Ok(mut guard) = PROGRESS_TX.write() {
        *guard = Some(tx);
    }
}

fn emit_progress(role: &str, msg: String) {
    emit_phase(role, "info", msg, None);
}

/// Structured progress event: `phase` marks the stage, `data` carries
/// tool/iteration/chars for the frontend card renderer. `message` is only
/// a fallback text for consumers without structured data.
fn emit_phase(role: &str, phase: &str, msg: String, extra: Option<Value>) {
    if let Ok(guard) = PROGRESS_TX.read() {
    if let Some(ref tx) = *guard {
        let mut data = serde_json::json!({ "phase": phase });
        if let (Some(obj), Some(ext)) = (data.as_object_mut(), extra.as_ref().and_then(|v| v.as_object())) {
            for (k, v) in ext { obj.insert(k.clone(), v.clone()); }
        }
        let _ = tx.send(WorkflowEvent {
            event_type: "task_progress".to_string(),
            task_id: Some(role.to_string()),
            message: msg,
            data: Some(data),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        });
    }
    }
}

/// Char-boundary-safe byte truncation. The old `&s[..8000]` slices could
/// split a multi-byte UTF-8 char and panic, silently killing the whole role
/// task (seen as the workflow stalling mid-run).
fn truncate_bytes_safe(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes { return s.to_string(); }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) { end -= 1; }
    format!("{}...[truncated, {} chars total]", &s[..end], s.chars().count())
}

const HARD_MAX_ITERATIONS: usize = 200;
const TOTAL_TIMEOUT_SECS: u64 = 600;
const STUCK_THRESHOLD: usize = 3;
const CALL_TIMEOUT_SECS: u64 = 120;

/// Simple LLM call without tools ? for lightweight actions.
pub async fn run_simple(prompt: &str, system: &str, provider: &ResolvedProvider) -> Result<String> {
    run_with_tools_named(prompt, system, provider, "", "Agent").await
}

pub async fn run_with_tools(prompt: &str, system: &str, provider: &ResolvedProvider, workspace: &str) -> Result<String> {
    run_with_tools_named(prompt, system, provider, workspace, "Agent").await
}

pub async fn run_with_tools_named(prompt: &str, system: &str, provider: &ResolvedProvider, workspace: &str, role_name: &str) -> Result<String> {
    let tools = get_agent_tool_definitions();
    match provider.provider.api_format {
        ApiFormat::OpenAI => run_openai(prompt, system, provider, &tools, workspace, role_name).await,
        ApiFormat::Anthropic => run_anthropic(prompt, system, provider, &tools, workspace, role_name).await,
    }
}

fn detect_stuck(recent_calls: &[(String, String)]) -> bool {
    if recent_calls.len() < STUCK_THRESHOLD { return false; }
    let last = &recent_calls[recent_calls.len() - 1];
    let count = recent_calls.iter().rev().take(STUCK_THRESHOLD)
        .filter(|(name, args)| name == &last.0 && args == &last.1)
        .count();
    count >= STUCK_THRESHOLD
}

async fn run_openai(prompt: &str, system: &str, provider: &ResolvedProvider, tools: &[crate::orchestration::agent_loop::AgentTool], workspace: &str, role_name: &str) -> Result<String> {
    use crate::native_engine::openai_client::*;
    let client = OpenAIClient::new();
    let tool_defs: Vec<ToolDefinition> = tools.iter().map(|t| ToolDefinition { name: t.name.clone(), description: t.description.clone(), input_schema: t.input_schema.clone() }).collect();
    let mut messages: Vec<OpenAIMessage> = vec![OpenAIMessage { role: "user".to_string(), content: OpenAIContent::Text(prompt.to_string()), tool_calls: None, tool_call_id: None, reasoning_content: None }];
    let mut accumulated_text = String::new();
    let mut recent_calls: Vec<(String, String)> = Vec::new();
    let start = std::time::Instant::now();

    for iteration in 0..HARD_MAX_ITERATIONS {
        if start.elapsed().as_secs() > TOTAL_TIMEOUT_SECS {
            tracing::warn!(target: "metagpt::tool_loop", "OpenAI total timeout ({}s), returning accumulated text ({} chars)", TOTAL_TIMEOUT_SECS, accumulated_text.len());
            return if !accumulated_text.is_empty() { Ok(accumulated_text) } else { Err(anyhow!("Tool loop timeout after {}s with no output", TOTAL_TIMEOUT_SECS)) };
        }

        emit_phase(role_name, "thinking", format!("[{}] thinking (iter {})", role_name, iteration + 1), Some(serde_json::json!({ "iteration": iteration + 1 })));
        let hb_role = role_name.to_string();
        let hb = tokio::spawn(async move {
            let mut iv = tokio::time::interval(std::time::Duration::from_secs(10));
            iv.tick().await;
            let mut n = 1u32;
            loop { iv.tick().await; emit_phase(&hb_role, "waiting", format!("[{}] waiting for LLM ({}s)", hb_role, n*10), Some(serde_json::json!({ "elapsed_s": n*10 }))); n += 1; }
        });
        let _rr = tokio::time::timeout(std::time::Duration::from_secs(CALL_TIMEOUT_SECS), client.send_message(provider, messages.clone(), Some(system), tool_defs.clone(), 8192, None, false)).await;
        hb.abort();
        let resp = _rr.map_err(|_| anyhow!("LLM call timeout"))??;
        let choice = resp.choices.first().ok_or_else(|| anyhow!("No choices in LLM response"))?;
        let msg = &choice.message;
        let resp_text = match &msg.content { OpenAIContent::Text(t) => t.clone(), OpenAIContent::Multi(parts) => { parts.iter().filter_map(|p| match p { OpenAIContentPart::Text { text } => Some(text.as_str()), _ => None }).collect::<Vec<_>>().join("") } };
        if !resp_text.is_empty() { let preview = truncate_bytes_safe(&resp_text, 600); emit_phase(role_name, "output", format!("[{}] output", role_name), Some(serde_json::json!({ "preview": preview }))); accumulated_text = resp_text.clone(); }

        if let Some(ref tool_calls) = msg.tool_calls {
            if !tool_calls.is_empty() {
                messages.push(OpenAIMessage { role: "assistant".to_string(), content: msg.content.clone(), tool_calls: Some(tool_calls.clone()), tool_call_id: None, reasoning_content: None });
                for tc in tool_calls {
                    let args_str = tc.function.arguments.clone();
                    let args: Value = serde_json::from_str(&args_str).unwrap_or(serde_json::json!({}));
                    recent_calls.push((tc.function.name.clone(), args_str.clone()));
                    if detect_stuck(&recent_calls) {
                        tracing::warn!(target: "metagpt::tool_loop", "OpenAI stuck: {} x{}", tc.function.name, STUCK_THRESHOLD);
                        return if !accumulated_text.is_empty() { Ok(accumulated_text) } else { Err(anyhow!("Tool loop stuck: {} repeated {} times", tc.function.name, STUCK_THRESHOLD)) };
                    }
                    emit_phase(role_name, "tool", format!("[{}] tool: {}", role_name, tc.function.name), Some(serde_json::json!({ "tool": tc.function.name })));
                    let tool_name_c = tc.function.name.clone();
                    let args_c = args.clone();
                    let ws_c = workspace.to_string();
                    let (output, _success) = tokio::task::spawn_blocking(move || execute_agent_tool(&tool_name_c, &args_c, &ws_c, None)).await.unwrap_or((format!("Tool execution panicked"), false));
                    let truncated = truncate_bytes_safe(&output, 8000);
                    let tlen = truncated.len();
                    tracing::info!(target: "metagpt::tool_loop", "[iter {}] Tool: {} => {} chars ({}s)", iteration + 1, tc.function.name, tlen, start.elapsed().as_secs());
                    messages.push(OpenAIMessage { role: "tool".to_string(), content: OpenAIContent::Text(truncated), tool_calls: None, tool_call_id: Some(tc.id.clone()), reasoning_content: None });
                }
                continue;
            }
        }
        tracing::info!(target: "metagpt::tool_loop", "OpenAI done after {} iters, {} chars", iteration + 1, resp_text.len());
        emit_phase(role_name, "output_done", format!("[{}] output done ({} chars)", role_name, resp_text.chars().count()), Some(serde_json::json!({ "chars": resp_text.chars().count() })));
        return Ok(resp_text);
    }
    if !accumulated_text.is_empty() {
        tracing::warn!(target: "metagpt::tool_loop", "OpenAI hit hard ceiling ({}), {} chars", HARD_MAX_ITERATIONS, accumulated_text.len());
        Ok(accumulated_text)
    } else {
        Err(anyhow!("Tool loop exceeded hard ceiling ({}) with no text output", HARD_MAX_ITERATIONS))
    }
}

async fn run_anthropic(prompt: &str, system: &str, provider: &ResolvedProvider, tools: &[crate::orchestration::agent_loop::AgentTool], workspace: &str, role_name: &str) -> Result<String> {
    use crate::native_engine::anthropic_client::*;
    use serde_json::json;
    let client = AnthropicClient::new();
    let tool_defs: Vec<Value> = tools.iter().map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema})).collect();
    let mut messages: Vec<AnthropicMessage> = vec![AnthropicMessage { role: "user".to_string(), content: AnthropicContent::Text(prompt.to_string()) }];
    let mut accumulated_text = String::new();
    let mut recent_calls: Vec<(String, String)> = Vec::new();
    let start = std::time::Instant::now();

    for iteration in 0..HARD_MAX_ITERATIONS {
        if start.elapsed().as_secs() > TOTAL_TIMEOUT_SECS {
            tracing::warn!(target: "metagpt::tool_loop", "Anthropic timeout ({}s)", TOTAL_TIMEOUT_SECS);
            return if !accumulated_text.is_empty() { Ok(accumulated_text) } else { Err(anyhow!("Tool loop timeout after {}s", TOTAL_TIMEOUT_SECS)) };
        }

        emit_phase(role_name, "thinking", format!("[{}] thinking (iter {})", role_name, iteration + 1), Some(serde_json::json!({ "iteration": iteration + 1 })));
        let hb_role = role_name.to_string();
        let hb = tokio::spawn(async move {
            let mut iv = tokio::time::interval(std::time::Duration::from_secs(10));
            iv.tick().await;
            let mut n = 1u32;
            loop { iv.tick().await; emit_phase(&hb_role, "waiting", format!("[{}] waiting for LLM ({}s)", hb_role, n*10), Some(serde_json::json!({ "elapsed_s": n*10 }))); n += 1; }
        });
        let _rr = tokio::time::timeout(std::time::Duration::from_secs(CALL_TIMEOUT_SECS), client.send_message(provider, messages.clone(), Some(system), tool_defs.iter().map(|v| crate::tools::ToolDefinition { name: v["name"].as_str().unwrap_or("").to_string(), description: v["description"].as_str().unwrap_or("").to_string(), input_schema: v["input_schema"].clone() }).collect(), 8192, None, false)).await;
        hb.abort();
        let resp = _rr.map_err(|_| anyhow!("LLM call timeout"))??;
        let mut tool_uses: Vec<&ContentBlock> = Vec::new();
        let mut text_parts: Vec<String> = Vec::new();
        let mut has_tool_use = false;
        for block in &resp.content {
            match block { ContentBlock::ToolUse { .. } => { tool_uses.push(block); has_tool_use = true; } ContentBlock::Text { text } => { text_parts.push(text.clone()); } _ => {} }
        }
        let resp_text = text_parts.join("\n");
        if !resp_text.is_empty() { accumulated_text = resp_text.clone(); }

        if has_tool_use {
            messages.push(AnthropicMessage { role: "assistant".to_string(), content: AnthropicContent::Blocks(resp.content.clone()) });
            let mut result_blocks: Vec<ContentBlock> = Vec::new();
            for tu in &tool_uses {
                if let ContentBlock::ToolUse { id, name, input } = tu {
                    let input_str = serde_json::to_string(input).unwrap_or_default();
                    recent_calls.push((name.clone(), input_str.clone()));
                    if detect_stuck(&recent_calls) {
                        tracing::warn!(target: "metagpt::tool_loop", "Anthropic stuck: {} x{}", name, STUCK_THRESHOLD);
                        return if !accumulated_text.is_empty() { Ok(accumulated_text) } else { Err(anyhow!("Tool loop stuck: {} x{}", name, STUCK_THRESHOLD)) };
                    }
                    emit_phase(role_name, "tool", format!("[{}] tool: {}", role_name, name), Some(serde_json::json!({ "tool": name })));
                    let name_c = name.clone();
                    let input_c = input.clone();
                    let ws_c = workspace.to_string();
                    let (output, success) = tokio::task::spawn_blocking(move || execute_agent_tool(&name_c, &input_c, &ws_c, None)).await.unwrap_or((format!("Tool execution panicked"), false));
                    let truncated = truncate_bytes_safe(&output, 8000);
                    tracing::info!(target: "metagpt::tool_loop", "[iter {}] Tool: {} => {} chars ({}s)", iteration + 1, name, truncated.len(), start.elapsed().as_secs());
                    result_blocks.push(ContentBlock::ToolResult { tool_use_id: id.clone(), content: truncated, is_error: if success { None } else { Some(true) } });
                }
            }
            messages.push(AnthropicMessage { role: "user".to_string(), content: AnthropicContent::Blocks(result_blocks) });
            continue;
        }
        tracing::info!(target: "metagpt::tool_loop", "Anthropic done after {} iters, {} chars", iteration + 1, resp_text.len());
        emit_phase(role_name, "output_done", format!("[{}] output done ({} chars)", role_name, resp_text.chars().count()), Some(serde_json::json!({ "chars": resp_text.chars().count() })));
        return Ok(resp_text);
    }
    if !accumulated_text.is_empty() {
        tracing::warn!(target: "metagpt::tool_loop", "Anthropic hit hard ceiling ({}), {} chars", HARD_MAX_ITERATIONS, accumulated_text.len());
        Ok(accumulated_text)
    } else {
        Err(anyhow!("Tool loop exceeded hard ceiling ({}) with no text output", HARD_MAX_ITERATIONS))
    }
}
