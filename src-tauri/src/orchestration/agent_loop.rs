/// Agent execution loop with tool calling, memory injection, and review feedback.
///
/// Each agent can call tools (Read/Write/Edit/Bash/Grep/Glob) during execution,
/// receives relevant memories as context, and can participate in review loops.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool definition sent to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Result of a single agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub text: String,
    pub tool_calls_made: Vec<ToolCallRecord>,
    pub approved: bool,
    pub feedback: Option<String>,
}

/// Record of a tool call made during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    pub input: serde_json::Value,
    pub output: String,
    pub success: bool,
}

/// Review verdict from the Reviewer agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewVerdict {
    pub approved: bool,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
    pub summary: String,
}

/// Maximum tool calls per agent task. Set very high to support large projects.
const MAX_TOOL_CALLS_DEVELOPER: usize = 10000;
const MAX_TOOL_CALLS_REVIEWER: usize = 10000;
const MAX_TOOL_CALLS_DEFAULT: usize = 10000;

/// Get max tool calls for a given role.
pub fn max_tool_calls_for_role(role: &str) -> usize {
    match role {
        "Software Developer" | "Developer" => MAX_TOOL_CALLS_DEVELOPER,
        "Code Reviewer" | "Reviewer" => MAX_TOOL_CALLS_REVIEWER,
        "Solution Architect" | "Architect" => MAX_TOOL_CALLS_DEFAULT,
        "Product Manager" => MAX_TOOL_CALLS_REVIEWER,
        "DevOps Engineer" | "DevOps" => MAX_TOOL_CALLS_DEFAULT,
        "Data Analyst" | "Analyst" => MAX_TOOL_CALLS_DEFAULT,
        "UX Designer" | "Designer" => MAX_TOOL_CALLS_REVIEWER,
        _ => MAX_TOOL_CALLS_DEFAULT,
    }
}

/// Get tool definitions available to agents.
pub fn get_agent_tool_definitions() -> Vec<AgentTool> {
    vec![
        AgentTool {
            name: "Read".to_string(),
            description: "Read a file from disk. Returns file contents.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Absolute path to the file" },
                    "offset": { "type": "number", "description": "Line number to start reading from (0-based)" },
                    "limit": { "type": "number", "description": "Max lines to read" }
                },
                "required": ["file_path"]
            }),
        },
        AgentTool {
            name: "Write".to_string(),
            description: "Write content to a file. Creates parent directories if needed.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Absolute path" },
                    "content": { "type": "string", "description": "File content" }
                },
                "required": ["file_path", "content"]
            }),
        },
        AgentTool {
            name: "Edit".to_string(),
            description: "Replace text in a file. Use old_string/new_string for exact replacement.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Absolute path" },
                    "old_string": { "type": "string", "description": "Text to find" },
                    "new_string": { "type": "string", "description": "Replacement text" }
                },
                "required": ["file_path", "old_string", "new_string"]
            }),
        },
        AgentTool {
            name: "Bash".to_string(),
            description: "Execute a shell command. Returns stdout/stderr.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "timeout": { "type": "number", "description": "Timeout in seconds (default 60)" }
                },
                "required": ["command"]
            }),
        },
        AgentTool {
            name: "Glob".to_string(),
            description: "Find files matching a glob pattern.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern (e.g. **/*.rs)" },
                    "path": { "type": "string", "description": "Base directory" }
                },
                "required": ["pattern"]
            }),
        },
        AgentTool {
            name: "Grep".to_string(),
            description: "Search file contents using regex.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern" },
                    "path": { "type": "string", "description": "File or directory to search" },
                    "include": { "type": "string", "description": "Glob filter for files" }
                },
                "required": ["pattern"]
            }),
        },
        AgentTool {
            name: "AskAgent".to_string(),
            description: "Ask another agent role a question. Use when you need input from a different expertise (e.g. ask Architect about design, ask Reviewer about code quality). Returns the other agent's response.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "role": { "type": "string", "description": "Agent role to ask: Product Manager, Solution Architect, Software Developer, Code Reviewer, DevOps Engineer, Data Analyst, UX Designer" },
                    "question": { "type": "string", "description": "The question to ask" }
                },
                "required": ["role", "question"]
            }),
        },
    ]
}

/// Convert agent tools to Anthropic API tool format.
pub fn tools_to_anthropic_format(tools: &[AgentTool]) -> Vec<serde_json::Value> {
    tools.iter().map(|t| {
        serde_json::json!({
            "name": t.name,
            "description": t.description,
            "input_schema": t.input_schema
        })
    }).collect()
}

/// Convert agent tools to OpenAI API tool format.
pub fn tools_to_openai_format(tools: &[AgentTool]) -> Vec<serde_json::Value> {
    tools.iter().map(|t| {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.input_schema
            }
        })
    }).collect()
}

/// Execute a tool call with sandbox isolation and return the result as a string.
pub fn execute_agent_tool(name: &str, input: &serde_json::Value, cwd: &str, sandbox: Option<&super::sandbox::Sandbox>) -> (String, bool) {
    // Resolve paths through sandbox if available
    let resolved_input = if let Some(sb) = sandbox {
        resolve_tool_paths(name, input, sb)
    } else {
        input.clone()
    };

    let effective_cwd = if let Some(sb) = sandbox {
        sb.bash_cwd().to_string_lossy().to_string()
    } else {
        cwd.to_string()
    };

    match crate::tools::execute_tool(name, resolved_input, &effective_cwd) {
        Ok(result) => {
            let output = if result.is_object() {
                // Extract meaningful text from structured output
                if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
                    (format!("Error: {}", error), false)
                } else if let Some(stdout) = result.get("stdout").and_then(|v| v.as_str()) {
                    let stderr = result.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
                    let exit = result.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0);
                    let mut out = stdout.to_string();
                    if !stderr.is_empty() {
                        out.push_str(&format!("\n[stderr]: {}", stderr));
                    }
                    if exit != 0 {
                        out.push_str(&format!("\n[exit code: {}]", exit));
                    }
                    (out, exit == 0)
                } else if let Some(files) = result.get("files").and_then(|v| v.as_array()) {
                    let paths: Vec<&str> = files.iter().filter_map(|f| f.as_str()).collect();
                    (paths.join("\n"), true)
                } else if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
                    (content.to_string(), true)
                } else if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
                    (text.to_string(), true)
                } else {
                    (serde_json::to_string_pretty(&result).unwrap_or_default(), true)
                }
            } else {
                (result.to_string(), true)
            };
            output
        }
        Err(e) => (format!("Tool error: {}", e), false),
    }
}

/// Resolve tool input paths through the sandbox.
/// - Write/Edit: paths go to sandbox (isolated)
/// - Read: paths resolve to workspace first, then sandbox
/// - Bash: cwd is sandbox root
fn resolve_tool_paths(tool_name: &str, input: &serde_json::Value, sandbox: &super::sandbox::Sandbox) -> serde_json::Value {
    let mut resolved = input.clone();

    match tool_name {
        "Write" | "Edit" | "MultiEdit" => {
            if let Some(path) = resolved.get("file_path").and_then(|v| v.as_str()) {
                let new_path = sandbox.resolve_path(tool_name, path);
                if let Some(obj) = resolved.as_object_mut() {
                    obj.insert("file_path".to_string(), serde_json::json!(new_path.to_string_lossy()));
                }
            }
        }
        "Read" => {
            if let Some(path) = resolved.get("file_path").and_then(|v| v.as_str()) {
                let new_path = sandbox.resolve_path("Read", path);
                if let Some(obj) = resolved.as_object_mut() {
                    obj.insert("file_path".to_string(), serde_json::json!(new_path.to_string_lossy()));
                }
            }
        }
        "Glob" | "Grep" => {
            if let Some(path) = resolved.get("path").and_then(|v| v.as_str()) {
                let new_path = sandbox.resolve_path(tool_name, path);
                if let Some(obj) = resolved.as_object_mut() {
                    obj.insert("path".to_string(), serde_json::json!(new_path.to_string_lossy()));
                }
            }
        }
        "ListDir" => {
            if let Some(path) = resolved.get("path").and_then(|v| v.as_str()) {
                let new_path = sandbox.resolve_path("ListDir", path);
                if let Some(obj) = resolved.as_object_mut() {
                    obj.insert("path".to_string(), serde_json::json!(new_path.to_string_lossy()));
                }
            }
        }
        "Bash" => {
            // Bash cwd is handled by the caller (effective_cwd)
            // No path rewriting needed for the command itself
        }
        _ => {}
    }

    resolved
}

/// Inject relevant memories into agent context.
pub fn build_memory_context(memories: &[crate::db::memory_repo::MemoryRow], max_chars: usize) -> String {
    if memories.is_empty() {
        return String::new();
    }
    let mut context = String::from("## Relevant Memories\n");
    let mut total = 0;
    for mem in memories {
        let entry = format!("- [{}] {}\n", mem.memory_type, mem.summary);
        if total + entry.len() > max_chars { break; }
        context.push_str(&entry);
        total += entry.len();
    }
    context.push('\n');
    context
}

/// Build review prompt for the Reviewer agent.
pub fn build_review_prompt(task_name: &str, task_output: &str, task_description: &str) -> String {
    format!(
        r#"Review the following work output. Be strict but fair.

## Task: {}
## Requirements: {}

## Output to Review:
{}

Respond in this exact JSON format:
{{
  "approved": true/false,
  "issues": ["issue 1", "issue 2"],
  "suggestions": ["suggestion 1"],
  "summary": "Brief review summary"
}}

If approved=false, the developer will rework based on your issues and suggestions."#,
        task_name, task_description, task_output
    )
}

/// Build rework prompt incorporating reviewer feedback.
pub fn build_rework_prompt(
    task_name: &str,
    task_description: &str,
    original_output: &str,
    review_feedback: &ReviewVerdict,
) -> String {
    let issues = review_feedback.issues.iter()
        .map(|i| format!("- {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let suggestions = review_feedback.suggestions.iter()
        .map(|s| format!("- {}", s))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"Your previous work was NOT approved. Please rework it.

## Task: {}
## Requirements: {}

## Your Previous Output:
{}

## Reviewer's Issues:
{}

## Reviewer's Suggestions:
{}

Please fix all issues and incorporate the suggestions. Provide the complete reworked output."#,
        task_name, task_description, original_output, issues, suggestions
    )
}

/// Parse a review verdict from LLM text response.
pub fn parse_review_verdict(text: &str) -> ReviewVerdict {
    // Try to extract JSON from the response
    let json_str = text.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
        ReviewVerdict {
            approved: parsed.get("approved").and_then(|v| v.as_bool()).unwrap_or(false),
            issues: parsed.get("issues")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            suggestions: parsed.get("suggestions")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            summary: parsed.get("summary").and_then(|v| v.as_str()).unwrap_or(text).to_string(),
        }
    } else {
        // Fallback: if contains "approved" and "true", approve
        let lower = text.to_lowercase();
        ReviewVerdict {
            approved: lower.contains("\"approved\": true") || lower.contains("approved: true"),
            issues: Vec::new(),
            suggestions: Vec::new(),
            summary: text.chars().take(200).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_review_verdict_approved() {
        let text = r#"{"approved": true, "issues": [], "suggestions": [], "summary": "Looks good"}"#;
        let verdict = parse_review_verdict(text);
        assert!(verdict.approved);
        assert_eq!(verdict.summary, "Looks good");
    }

    #[test]
    fn test_parse_review_verdict_rejected() {
        let text = r#"{"approved": false, "issues": ["Missing error handling", "No tests"], "suggestions": ["Add unit tests"], "summary": "Needs rework"}"#;
        let verdict = parse_review_verdict(text);
        assert!(!verdict.approved);
        assert_eq!(verdict.issues.len(), 2);
        assert_eq!(verdict.suggestions.len(), 1);
    }

    #[test]
    fn test_build_memory_context() {
        let memories = vec![];
        assert_eq!(build_memory_context(&memories, 500), "");
    }

    #[test]
    fn test_build_review_prompt() {
        let prompt = build_review_prompt("Test Task", "output here", "do something");
        assert!(prompt.contains("Test Task"));
        assert!(prompt.contains("approved"));
    }
}
