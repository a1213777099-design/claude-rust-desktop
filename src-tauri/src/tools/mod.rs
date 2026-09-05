pub mod retry;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "Read".to_string(),
            description: "Read a file from the local filesystem. Returns content with line numbers.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The path to the file, relative to the workspace root (e.g. src/main.rs) or absolute" },
                    "offset": { "type": "number", "description": "Line number to start reading from (1-based)" },
                    "limit": { "type": "number", "description": "Max number of lines to read" }
                },
                "required": ["file_path"]
            }),
        },
        ToolDefinition {
            name: "Write".to_string(),
            description: "Write content to a file. Creates the file and parent directories if needed.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The path to the file, relative to the workspace root or absolute" },
                    "content": { "type": "string", "description": "The full content to write" }
                },
                "required": ["file_path", "content"]
            }),
        },
        ToolDefinition {
            name: "Edit".to_string(),
            description: "Make an exact string replacement in a file.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The path to the file, relative to the workspace root or absolute" },
                    "old_string": { "type": "string", "description": "The exact text to find" },
                    "new_string": { "type": "string", "description": "The replacement text" },
                    "replace_all": { "type": "boolean", "description": "If true, replace ALL occurrences" }
                },
                "required": ["file_path", "old_string", "new_string"]
            }),
        },
        ToolDefinition {
            name: "Bash".to_string(),
            description: "Execute a shell command and return stdout/stderr.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The shell command to execute" },
                    "timeout": { "type": "number", "description": "Timeout in seconds (default: 60)" }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "Glob".to_string(),
            description: "Find files matching a glob pattern.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern" },
                    "path": { "type": "string", "description": "Base directory to search in, relative to the workspace root or absolute" }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "Grep".to_string(),
            description: "Search file contents using regex.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern to search for" },
                    "path": { "type": "string", "description": "File or directory to search in, relative to the workspace root or absolute" },
                    "include": { "type": "string", "description": "Glob to filter files" }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "ListDir".to_string(),
            description: "List the contents of a directory.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list, relative to the workspace root or absolute" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "WebFetch".to_string(),
            description: "Fetch content from a URL.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The URL to fetch" },
                    "headers": { "type": "object", "description": "Optional HTTP headers" }
                },
                "required": ["url"]
            }),
        },
        ToolDefinition {
            name: "WebSearch".to_string(),
            description: "Search the web for information.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search query" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "MultiEdit".to_string(),
            description: "Make multiple string replacements in a file at once.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The path to the file" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_string": { "type": "string" },
                                "new_string": { "type": "string" }
                            }
                        }
                    }
                },
                "required": ["file_path", "edits"]
            }),
        },
        ToolDefinition {
            name: "AskUserQuestion".to_string(),
            description: "Ask the user a question with multiple options. Returns the user's selected options and any custom input.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "The question to ask the user" },
                    "description": { "type": "string", "description": "Additional context or description for the question" },
                    "options": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string", "description": "The display label for this option" },
                                "description": { "type": "string", "description": "Additional description for this option" }
                            },
                            "required": ["label"]
                        },
                        "description": "The available options for the user to choose from"
                    },
                    "multiSelect": { "type": "boolean", "description": "Whether the user can select multiple options (default: false)" }
                },
                "required": ["question", "options"]
            }),
        },
        ToolDefinition {
            name: "git_status".to_string(),
            description: "Runs git status in the workspace directory to see changed, staged, and untracked files.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The workspace directory path to run git status in" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "git_diff".to_string(),
            description: "Runs git diff to show changes in the working directory or staging area.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The workspace directory path" },
                    "staged": { "type": "boolean", "description": "If true, show staged changes (git diff --staged)" },
                    "file": { "type": "string", "description": "Optional specific file path to diff" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "git_log".to_string(),
            description: "Runs git log to show commit history.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The workspace directory path" },
                    "count": { "type": "number", "description": "Number of commits to show (default: 10)" },
                    "oneline": { "type": "boolean", "description": "Use oneline format (default: true)" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "git_commit".to_string(),
            description: "Stages all changes and commits them with the given message. Runs git add -A then git commit.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The workspace directory path" },
                    "message": { "type": "string", "description": "The commit message" }
                },
                "required": ["path", "message"]
            }),
        },
        ToolDefinition {
            name: "git_add".to_string(),
            description: "Stages specific files for the next commit. Runs git add with the specified file paths.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The workspace directory path" },
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Array of file paths to stage"
                    }
                },
                "required": ["path", "files"]
            }),
        },
        ToolDefinition {
            name: "computer_use".to_string(),
            description: "Control the computer: move mouse, click, type text, press keys, take screenshots. Use this to interact with the desktop GUI.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action_type": {
                        "type": "string",
                        "enum": ["MouseMove", "MouseClick", "MouseDown", "MouseUp", "MouseScroll", "KeyPress", "KeyDown", "KeyUp", "TypeText", "Screenshot", "Wait"],
                        "description": "The type of computer action to perform"
                    },
                    "coordinate": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "integer", "description": "X coordinate (pixels)" },
                            "y": { "type": "integer", "description": "Y coordinate (pixels)" }
                        },
                        "description": "Screen coordinate for mouse actions"
                    },
                    "button": {
                        "type": "string",
                        "enum": ["Left", "Right", "Middle", "Back", "Forward"],
                        "description": "Mouse button for click actions (default: Left)"
                    },
                    "key": {
                        "type": "string",
                        "description": "Key name for keyboard actions (e.g. 'Enter', 'Tab', 'Escape', 'a', 'F1')"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type for TypeText action"
                    },
                    "scroll_y": {
                        "type": "integer",
                        "description": "Vertical scroll amount (positive = down, negative = up)"
                    },
                    "scroll_x": {
                        "type": "integer",
                        "description": "Horizontal scroll amount (positive = right, negative = left)"
                    },
                    "duration_ms": {
                        "type": "integer",
                        "description": "Duration in milliseconds for Wait action"
                    }
                },
                "required": ["action_type"]
            }),
        },
        ToolDefinition {
            name: "browser_use".to_string(),
            description: "Control an in-app real browser (headless Edge) via CDP. Recommended MCP-style workflow: 1) call action_type='snapshot' to get interactive elements with stable refs; 2) act by ref with 'click_ref' / 'fill' / 'select' / 'hover_ref' — far more reliable than screenshot coordinates. Also supports navigate/screenshot/scroll/key/text. Every screenshot result includes page_text so text-only models can understand the page without vision. Use this instead of computer_use (which controls the real desktop).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action_type": {
                        "type": "string",
                        "enum": ["navigate", "goto", "snapshot", "click_ref", "fill", "select", "hover_ref", "screenshot", "click", "move", "scroll", "type", "key", "text", "read", "url", "load", "home", "wait"],
                        "description": "'snapshot' lists interactive elements with refs (start here); 'click_ref'/'fill'/'select'/'hover_ref' act by ref; 'text'/'read' returns page text (for non-vision models); 'wait' waits ms then screenshots."
                    },
                    "url": {
                        "type": "string",
                        "description": "URL to open (for navigate/goto)"
                    },
                    "ref": {
                        "type": "string",
                        "description": "Element ref from snapshot, e.g. 'e12' (for click_ref/fill/select/hover_ref)"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to fill (for fill/type). For fill, set submit=true to press Enter afterwards."
                    },
                    "submit": {
                        "type": "boolean",
                        "description": "For fill: press Enter after filling (submit forms/search)"
                    },
                    "value": {
                        "type": "string",
                        "description": "Option value to select (for select)"
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["up", "down"],
                        "description": "Scroll direction (default down)"
                    },
                    "amount": {
                        "type": "integer",
                        "description": "Scroll pixels (default 600)"
                    },
                    "ms": {
                        "type": "integer",
                        "description": "Milliseconds to wait (for wait, max 5000)"
                    },
                    "coordinate": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "integer", "description": "X coordinate (pixels)" },
                            "y": { "type": "integer", "description": "Y coordinate (pixels)" }
                        },
                        "description": "Page coordinate for legacy click/move/scroll (prefer snapshot+click_ref)"
                    },
                    "key": {
                        "type": "string",
                        "description": "Key name for key action (e.g. 'Enter', 'Tab', 'Escape', 'ArrowDown')"
                    }
                },
                "required": ["action_type"]
            }),
        },
    ]
}

pub fn execute_tool(name: &str, input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    match name {
        "Read" => tool_read(input, cwd),
        "Write" => tool_write(input, cwd),
        "Edit" => tool_edit(input, cwd),
        "Bash" => tool_bash(input, cwd),
        "Glob" => tool_glob(input, cwd),
        "Grep" => tool_grep(input, cwd),
        "ListDir" => tool_list_dir(input, cwd),
        "WebFetch" => tool_web_fetch_blocking(input),
        "WebSearch" => tool_web_search_blocking(input),
        "MultiEdit" => tool_multi_edit(input, cwd),
        "AskUserQuestion" => tool_ask_user_question(input),
        "git_status" => tool_git_status(input),
        "git_diff" => tool_git_diff(input),
        "git_log" => tool_git_log(input),
        "git_commit" => tool_git_commit(input),
        "git_add" => tool_git_add(input),
        "computer_use" => tool_computer_use(input),
        _ => Ok(serde_json::json!({ "error": format!("Unknown tool: {}", name) })),
    }
}

pub async fn execute_tool_async(name: &str, input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    match name {
        "Read" => tool_read(input, cwd),
        "Write" => tool_write(input, cwd),
        "Edit" => tool_edit(input, cwd),
        "Bash" => tool_bash_async(input, cwd).await,
        "Glob" => tool_glob(input, cwd),
        "Grep" => tool_grep(input, cwd),
        "ListDir" => tool_list_dir(input, cwd),
        "WebFetch" => tool_web_fetch_async(input).await,
        "WebSearch" => tool_web_search_async(input).await,
        "MultiEdit" => tool_multi_edit(input, cwd),
        "AskUserQuestion" => tool_ask_user_question(input),
        "git_status" => tool_git_status(input),
        "git_diff" => tool_git_diff(input),
        "git_log" => tool_git_log(input),
        "git_commit" => tool_git_commit(input),
        "git_add" => tool_git_add(input),
        "computer_use" => tool_computer_use(input),
        "browser_use" => crate::browser_use::execute_browser_action(input).await,
        _ => Ok(serde_json::json!({ "error": format!("Unknown tool: {}", name) })),
    }
}

fn resolve_path(file_path: &str, cwd: &str) -> String {
    if Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        Path::new(cwd).join(file_path).to_string_lossy().to_string()
    }
}

fn tool_read(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let file_path = input["file_path"].as_str().ok_or_else(|| anyhow!("file_path required"))?;
    let path = resolve_path(file_path, cwd);

    if !Path::new(&path).exists() {
        return Ok(serde_json::json!({ "content": format!("File not found: {}", path), "is_error": true }));
    }

    let content = fs::read_to_string(&path)?;
    let offset = input["offset"].as_u64().unwrap_or(1) as usize;
    let limit = input["limit"].as_u64().unwrap_or(2000) as usize;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let selected: Vec<String> = lines
        .iter()
        .skip(offset.saturating_sub(1))
        .take(limit)
        .enumerate()
        .map(|(i, line)| format!("{:>6}\t{}", i + offset, line))
        .collect();

    Ok(serde_json::json!({
        "content": selected.join("\n"),
        "lines": total_lines,
        "truncated": total_lines > limit
    }))
}

fn tool_write(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let file_path = input["file_path"].as_str().ok_or_else(|| anyhow!("file_path required"))?;
    let content = input["content"].as_str().ok_or_else(|| anyhow!("content required"))?;
    let path = resolve_path(file_path, cwd);

    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&path, content)?;

    Ok(serde_json::json!({
        "success": true,
        "content": format!("Successfully wrote to {}", path),
        "bytes_written": content.len()
    }))
}

fn tool_edit(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let file_path = input["file_path"].as_str().ok_or_else(|| anyhow!("file_path required"))?;
    let old_string = input["old_string"].as_str().ok_or_else(|| anyhow!("old_string required"))?;
    let new_string = input["new_string"].as_str().ok_or_else(|| anyhow!("new_string required"))?;
    let replace_all = input["replace_all"].as_bool().unwrap_or(false);
    let path = resolve_path(file_path, cwd);

    let content = fs::read_to_string(&path)?;

    let (new_content, replacements) = if replace_all {
        let count = content.matches(old_string).count();
        (content.replace(old_string, new_string), count)
    } else {
        let count = content.matches(old_string).count();
        if count == 0 {
            return Ok(serde_json::json!({
                "success": false,
                "error": "old_string not found in file"
            }));
        }
        if count > 1 {
            return Ok(serde_json::json!({
                "success": false,
                "error": format!("old_string found {} times, use replace_all=true", count)
            }));
        }
        (content.replacen(old_string, new_string, 1), 1)
    };

    fs::write(&path, new_content)?;

    Ok(serde_json::json!({
        "success": true,
        "content": format!("Successfully replaced {} occurrence(s) in {}", replacements, path),
        "replacements": replacements
    }))
}

fn tool_multi_edit(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let file_path = input["file_path"].as_str().ok_or_else(|| anyhow!("file_path required"))?;
    let edits = input["edits"].as_array().ok_or_else(|| anyhow!("edits array required"))?;
    let path = resolve_path(file_path, cwd);

    let content = fs::read_to_string(&path)?;
    let mut new_content = content;
    let mut total_replacements = 0;
    let mut failed_edits: Vec<String> = Vec::new();

    for edit in edits {
        let old_string = edit.get("old_string").and_then(|s| s.as_str());
        let new_string = edit.get("new_string").and_then(|s| s.as_str());

        if let (Some(old), Some(new)) = (old_string, new_string) {
            if new_content.contains(old) {
                new_content = new_content.replace(old, new);
                total_replacements += 1;
            } else {
                failed_edits.push(old.to_string());
            }
        }
    }

    if !failed_edits.is_empty() {
        return Ok(serde_json::json!({
            "success": false,
            "error": format!("Some edits failed: {:?}", failed_edits),
            "replacements": total_replacements
        }));
    }

    fs::write(&path, new_content)?;

    Ok(serde_json::json!({
        "success": true,
        "content": format!("Successfully applied {} edits to {}", total_replacements, path),
        "replacements": total_replacements
    }))
}

fn tool_ask_user_question(input: serde_json::Value) -> Result<serde_json::Value> {
    let question = input["question"].as_str().ok_or_else(|| anyhow!("question required"))?;
    let options = input["options"].as_array().ok_or_else(|| anyhow!("options array required"))?;
    
    if options.is_empty() {
        return Ok(serde_json::json!({
            "content": "Question must have at least one option",
            "is_error": true
        }));
    }

    let description = input["description"].as_str().unwrap_or("");
    let multi_select = input["multiSelect"].as_bool().unwrap_or(false);

    let options_list: Vec<serde_json::Value> = options.iter().map(|opt| {
        serde_json::json!({
            "label": opt["label"].as_str().unwrap_or(""),
            "description": opt["description"].as_str().unwrap_or("")
        })
    }).collect();

    Ok(serde_json::json!({
        "type": "ask_user_question",
        "question": question,
        "description": description,
        "options": options_list,
        "multiSelect": multi_select,
        "content": format!("Waiting for user response to: {}", question),
        "requires_user_input": true
    }))
}

fn tool_computer_use(input: serde_json::Value) -> Result<serde_json::Value> {
    use crate::computer_use::{
        ComputerAction, ComputerActionType, ComputerUseConfig, ComputerUseManager, MouseButton,
        ScreenCoordinate,
    };

    let action_type_str = input["action_type"]
        .as_str()
        .ok_or_else(|| anyhow!("action_type required"))?;

    let action_type = match action_type_str {
        "MouseMove" => ComputerActionType::MouseMove,
        "MouseClick" => ComputerActionType::MouseClick,
        "MouseDown" => ComputerActionType::MouseDown,
        "MouseUp" => ComputerActionType::MouseUp,
        "MouseScroll" => ComputerActionType::MouseScroll,
        "KeyPress" => ComputerActionType::KeyPress,
        "KeyDown" => ComputerActionType::KeyDown,
        "KeyUp" => ComputerActionType::KeyUp,
        "TypeText" => ComputerActionType::TypeText,
        "Screenshot" => ComputerActionType::Screenshot,
        "Wait" => ComputerActionType::Wait,
        _ => return Ok(serde_json::json!({ "error": format!("Unknown action_type: {}", action_type_str), "is_error": true })),
    };

    let coordinate = input
        .get("coordinate")
        .and_then(|c| {
            Some(ScreenCoordinate {
                x: c.get("x")?.as_i64()? as i32,
                y: c.get("y")?.as_i64()? as i32,
            })
        });

    let button = input["button"]
        .as_str()
        .and_then(|b| match b {
            "Left" => Some(MouseButton::Left),
            "Right" => Some(MouseButton::Right),
            "Middle" => Some(MouseButton::Middle),
            "Back" => Some(MouseButton::Back),
            "Forward" => Some(MouseButton::Forward),
            _ => None,
        });

    let key = input["key"].as_str().map(|s| s.to_string());
    let text = input["text"].as_str().map(|s| s.to_string());
    let scroll_y = input["scroll_y"].as_i64().map(|v| v as i32);
    let scroll_x = input["scroll_x"].as_i64().map(|v| v as i32);
    let duration_ms = input["duration_ms"].as_u64();

    let action = ComputerAction {
        action_type,
        coordinate,
        button,
        key,
        text,
        scroll_y,
        scroll_x,
        duration_ms,
    };

    let manager = ComputerUseManager::new(ComputerUseConfig::default());

    // panic.log 9/3 连续 4 次崩溃的根因：同步函数在 tokio 上下文里（execute_tool_async
    // 路径）直接 handle.block_on → "Cannot start a runtime from within a runtime" panic。
    // 修复：有 tokio 上下文时把执行移到独立阻塞线程 + 独立 runtime（绝不嵌套 block_on）；
    // 无上下文时（spawn_blocking 线程路径）维持原有临时 runtime。
    let rt = tokio::runtime::Handle::try_current();
    let result: crate::computer_use::ComputerActionResult = match rt {
        Ok(_handle) => {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let res = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .and_then(|rt| rt.block_on(manager.execute_action(action)).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{}", e))));
                let _ = tx.send(res);
            });
            let joined = rx.recv().map_err(|e| anyhow!("computer_use worker thread dropped: {}", e))?
                .map_err(|e| anyhow!("computer_use runtime/task failed: {}", e))?;
            joined
        }
        Err(_) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(manager.execute_action(action))?
        }
    };

    Ok(serde_json::json!({
        "success": result.success,
        "action": action_type_str,
        "screenshot": result.screenshot,
        "error": result.error,
        "duration_ms": result.duration_ms,
    }))
}

#[allow(dead_code)]
async fn tool_computer_use_async(input: serde_json::Value) -> Result<serde_json::Value> {
    use crate::computer_use::{
        ComputerAction, ComputerActionType, ComputerUseConfig, ComputerUseManager, MouseButton,
        ScreenCoordinate,
    };

    let action_type_str = input["action_type"]
        .as_str()
        .ok_or_else(|| anyhow!("action_type required"))?;

    let action_type = match action_type_str {
        "MouseMove" => ComputerActionType::MouseMove,
        "MouseClick" => ComputerActionType::MouseClick,
        "MouseDown" => ComputerActionType::MouseDown,
        "MouseUp" => ComputerActionType::MouseUp,
        "MouseScroll" => ComputerActionType::MouseScroll,
        "KeyPress" => ComputerActionType::KeyPress,
        "KeyDown" => ComputerActionType::KeyDown,
        "KeyUp" => ComputerActionType::KeyUp,
        "TypeText" => ComputerActionType::TypeText,
        "Screenshot" => ComputerActionType::Screenshot,
        "Wait" => ComputerActionType::Wait,
        _ => return Ok(serde_json::json!({ "error": format!("Unknown action_type: {}", action_type_str), "is_error": true })),
    };

    let coordinate = input
        .get("coordinate")
        .and_then(|c| {
            Some(ScreenCoordinate {
                x: c.get("x")?.as_i64()? as i32,
                y: c.get("y")?.as_i64()? as i32,
            })
        });

    let button = input["button"]
        .as_str()
        .and_then(|b| match b {
            "Left" => Some(MouseButton::Left),
            "Right" => Some(MouseButton::Right),
            "Middle" => Some(MouseButton::Middle),
            "Back" => Some(MouseButton::Back),
            "Forward" => Some(MouseButton::Forward),
            _ => None,
        });

    let key = input["key"].as_str().map(|s| s.to_string());
    let text = input["text"].as_str().map(|s| s.to_string());
    let scroll_y = input["scroll_y"].as_i64().map(|v| v as i32);
    let scroll_x = input["scroll_x"].as_i64().map(|v| v as i32);
    let duration_ms = input["duration_ms"].as_u64();

    let action = ComputerAction {
        action_type,
        coordinate,
        button,
        key,
        text,
        scroll_y,
        scroll_x,
        duration_ms,
    };

    let manager = ComputerUseManager::new(ComputerUseConfig::default());
    let result: crate::computer_use::ComputerActionResult = manager.execute_action(action).await?;

    Ok(serde_json::json!({
        "success": result.success,
        "action": action_type_str,
        "screenshot": result.screenshot,
        "error": result.error,
        "duration_ms": result.duration_ms,
    }))
}

async fn tool_bash_async(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let command = input["command"].as_str().ok_or_else(|| anyhow!("command required"))?;
    let timeout_secs = input["timeout"].as_u64().unwrap_or(60).min(600);

    // Dangerous command detection
    if let Some(warning) = detect_dangerous_command(command) {
        tracing::warn!(target: "bash", "Potentially dangerous command: {} - {}", command, warning);
    }

    let (shell, flag) = if cfg!(target_os = "windows") {
        let git_bash = find_git_bash();
        if let Some(git_bash_path) = git_bash {
            (git_bash_path, "-c".to_string())
        } else {
            ("cmd".to_string(), "/C".to_string())
        }
    } else {
        ("sh".to_string(), "-c".to_string())
    };

    let mut cmd = Command::new(&shell);
    cmd.arg(&flag)
        .arg(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        cmd.output()
    ).await;

    match output {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            // Truncate large output
            const MAX_OUTPUT: usize = 102_400; // 100KB
            let (stdout, stdout_truncated) = truncate_str(&stdout, MAX_OUTPUT);
            let (stderr, stderr_truncated) = truncate_str(&stderr, MAX_OUTPUT);

            Ok(serde_json::json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": exit_code,
                "success": output.status.success(),
                "stdout_truncated": stdout_truncated,
                "stderr_truncated": stderr_truncated
            }))
        }
        Ok(Err(e)) => Ok(serde_json::json!({
            "error": format!("Command failed: {}", e),
            "is_error": true
        })),
        Err(_) => Ok(serde_json::json!({
            "error": format!("Command timed out after {} seconds", timeout_secs),
            "is_error": true,
            "timed_out": true
        })),
    }
}

/// Detect potentially dangerous shell commands.
fn detect_dangerous_command(cmd: &str) -> Option<&'static str> {
    let lower = cmd.trim().to_lowercase();
    if lower.contains("rm -rf /") || lower.contains("rm -rf /*") {
        return Some("Recursive delete from root filesystem");
    }
    if lower.starts_with("git push") && lower.contains("--force") && (lower.contains("main") || lower.contains("master")) {
        return Some("Force push to main/master branch");
    }
    if lower.starts_with("git reset") && lower.contains("--hard") {
        return Some("Hard reset discards all changes");
    }
    if lower.contains("mkfs") || lower.contains("dd if=") {
        return Some("Low-level disk operation");
    }
    if lower.contains("chmod 777") || lower.contains("chmod -r 777") {
        return Some("Setting world-writable permissions");
    }
    None
}

/// Truncate a string to max_len bytes, returning (truncated_string, was_truncated).
fn truncate_str(s: &str, max_len: usize) -> (String, bool) {
    if s.len() <= max_len {
        (s.to_string(), false)
    } else {
        // 回退到 UTF-8 字符边界：&s[..max_len] 若切在多字节字符中间会直接 panic
        // （panic.log 里 tools/mod.rs 的多次崩溃源于此）。is_char_boundary 回退修复。
        let mut safe_end = max_len;
        while safe_end > 0 && !s.is_char_boundary(safe_end) {
            safe_end -= 1;
        }
        let truncated = &s[..safe_end];
        (format!("{}\n... (truncated, {} bytes total)", truncated, s.len()), true)
    }
}

fn tool_bash(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let command = input["command"].as_str().ok_or_else(|| anyhow!("command required"))?;
    let timeout_secs = input["timeout"].as_u64().unwrap_or(60).min(300);

    let (shell, flag) = if cfg!(target_os = "windows") {
        let git_bash = find_git_bash();
        if let Some(git_bash_path) = git_bash {
            (git_bash_path, "-c".to_string())
        } else {
            ("cmd".to_string(), "/C".to_string())
        }
    } else {
        ("sh".to_string(), "-c".to_string())
    };

    let mut cmd = std::process::Command::new(&shell);
    cmd.arg(&flag)
        .arg(command)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = cmd.spawn().map_err(|e| anyhow!("Failed to spawn: {}", e))?;
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(serde_json::json!({
                        "error": format!("Command timed out after {} seconds", timeout_secs),
                        "is_error": true,
                        "timed_out": true
                    }));
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => return Ok(serde_json::json!({
                "error": format!("Wait error: {}", e),
                "is_error": true
            })),
        }
    }

    let output = child.wait_with_output().map_err(|e| anyhow!("Output error: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok(serde_json::json!({
        "stdout": stdout,
        "stderr": stderr,
        "exit_code": exit_code,
        "success": output.status.success()
    }))
}

fn find_git_bash() -> Option<String> {
    let candidates: Vec<String> = if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Git\bin\bash.exe".to_string(),
            r"C:\Program Files (x86)\Git\bin\bash.exe".to_string(),
        ]
    } else {
        vec!["/usr/bin/bash".to_string(), "/bin/bash".to_string()]
    };

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.clone());
        }
    }

    None
}

fn tool_glob(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let pattern = input["pattern"].as_str().ok_or_else(|| anyhow!("pattern required"))?;
    let base_path = input["path"].as_str().unwrap_or(cwd);

    // Dirs to always skip
    const SKIP_DIRS: &[&str] = &[
        ".git", "node_modules", "target", "__pycache__", ".venv", "venv",
        ".next", "dist", "build", ".idea", ".vscode", ".cache",
    ];

    let start_time = std::time::Instant::now();
    let max_duration = std::time::Duration::from_secs(30);
    let max_files = 10000usize;
    let mut matches: Vec<String> = Vec::new();
    let mut file_count = 0usize;

    let base = std::path::Path::new(base_path);
    let Ok(glob_pattern) = glob::Pattern::new(pattern) else {
        return Ok(serde_json::json!({"files": [], "count": 0, "error": "Invalid glob pattern"}));
    };

    for entry in walkdir::WalkDir::new(base_path)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            !SKIP_DIRS.contains(&name)
        })
        .filter_map(|e| e.ok())
    {
        if start_time.elapsed() > max_duration { break; }
        if file_count >= max_files { break; }

        let path = entry.path();
        file_count += 1;

        let Some(path_str) = path.to_str() else { continue };

        // Match against filename for simple patterns (*.tsx, *.rs)
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Match against relative path for deep patterns (src/**/*.tsx)
        let rel_path = path.strip_prefix(base).ok()
            .and_then(|p| p.to_str())
            .unwrap_or(path_str);

        if glob_pattern.matches(file_name) || glob_pattern.matches(rel_path) || glob_pattern.matches(path_str) {
            matches.push(path_str.to_string());
        }
    }

    Ok(serde_json::json!({
        "files": matches,
        "count": matches.len()
    }))
}

fn tool_grep(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let pattern = input["pattern"].as_str().ok_or_else(|| anyhow!("pattern required"))?;
    let search_path = input["path"].as_str().unwrap_or(cwd);
    let include_glob = input["include"].as_str();
    let context_lines = input["context"].as_u64().unwrap_or(0) as usize;
    let max_results: usize = input["max_results"].as_u64().unwrap_or(500) as usize;

    let re = regex::Regex::new(pattern)?;
    let mut results: Vec<serde_json::Value> = Vec::new();

    // Dirs to always skip — prevent walkdir from entering them
    const SKIP_DIRS: &[&str] = &[
        ".git", "node_modules", "target", "__pycache__", ".venv", "venv",
        ".next", "dist", "build", ".idea", ".vscode", ".cache",
    ];

    // Max file size to search (1MB)
    const MAX_FILE_SIZE: u64 = 1_048_576;
    // Max files to scan (prevent hanging on huge dirs)
    const MAX_FILES_SCANNED: usize = 10_000;
    // Max search duration (30 seconds)
    let start_time = std::time::Instant::now();
    let max_duration = std::time::Duration::from_secs(30);

    let mut files_scanned: usize = 0;

    let walker = walkdir::WalkDir::new(search_path)
        .into_iter()
        .filter_entry(|e| {
            // Prevent descending into skip directories
            if e.file_type().is_dir() {
                if let Some(name) = e.file_name().to_str() {
                    if SKIP_DIRS.contains(&name) {
                        return false;
                    }
                }
            }
            true
        });

    for entry in walker.filter_map(|e| e.ok()) {
        // Check limits
        if results.len() >= max_results { break; }
        if files_scanned >= MAX_FILES_SCANNED { break; }
        if start_time.elapsed() > max_duration { break; }

        let path = entry.path();

        if !path.is_file() { continue; }
        files_scanned += 1;

        // Apply include glob filter
        if let Some(glob_pattern) = include_glob {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !glob_match(glob_pattern, file_name) {
                continue;
            }
        }

        // Skip large files
        if let Ok(metadata) = path.metadata() {
            if metadata.len() > MAX_FILE_SIZE { continue; }
        }

        // Skip binary files (quick check: first 512 bytes for null)
        if let Ok(bytes) = std::fs::read(path) {
            let check_len = bytes.len().min(512);
            if bytes[..check_len].contains(&0) { continue; }
        } else {
            continue;
        }

        if let Ok(content) = fs::read_to_string(path) {
            let all_lines: Vec<&str> = content.lines().collect();
            for (line_idx, line) in all_lines.iter().enumerate() {
                if results.len() >= max_results { break; }
                if re.is_match(line) {
                    let line_num = line_idx + 1;
                    let mut match_entry = serde_json::json!({
                        "file": path.to_string_lossy(),
                        "line": line_num,
                        "content": line
                    });

                    // Add context lines if requested
                    if context_lines > 0 {
                        let start = line_idx.saturating_sub(context_lines);
                        let end = (line_idx + context_lines + 1).min(all_lines.len());
                        let context: Vec<String> = all_lines[start..end]
                            .iter()
                            .enumerate()
                            .map(|(i, l)| format!("{:>6}: {}", start + i + 1, l))
                            .collect();
                        if let Some(obj) = match_entry.as_object_mut() {
                            obj.insert("context".to_string(), serde_json::json!(context.join("\n")));
                        }
                    }

                    results.push(match_entry);
                }
            }
        }
    }

    let timed_out = start_time.elapsed() > max_duration;

    Ok(serde_json::json!({
        "matches": results,
        "count": results.len(),
        "truncated": results.len() >= max_results,
        "files_scanned": files_scanned,
        "timed_out": timed_out
    }))
}

fn glob_match(pattern: &str, name: &str) -> bool {
    // Simple glob matching: * matches any chars, ? matches single char
    let regex_pattern: String = pattern
        .chars()
        .fold((String::new(), false), |(mut s, escaped), c| {
            if escaped {
                s.push(c);
                (s, false)
            } else {
                match c {
                    '*' => { s.push_str(".*"); (s, false) }
                    '?' => { s.push('.'); (s, false) }
                    '.' | '[' | ']' | '(' | ')' | '{' | '}' | '+' | '^' | '$' | '|' | '\\' => {
                        s.push('\\'); s.push(c); (s, false)
                    }
                    _ => { s.push(c); (s, false) }
                }
            }
        }).0;
    regex::Regex::new(&format!("^{}$", regex_pattern))
        .map(|re| re.is_match(name))
        .unwrap_or(false)
}

fn tool_list_dir(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let dir_path = input["path"].as_str().unwrap_or(cwd);

    let path = std::path::Path::new(dir_path);
    if !path.exists() {
        return Ok(serde_json::json!({ "entries": [], "error": format!("Path does not exist: {}", dir_path) }));
    }
    if !path.is_dir() {
        return Ok(serde_json::json!({ "entries": [], "error": format!("Not a directory: {}", dir_path) }));
    }

    let mut entries: Vec<serde_json::Value> = Vec::new();
    let max_entries = 500usize;

    let read_dir = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(e) => return Ok(serde_json::json!({ "entries": [], "error": format!("Permission denied or error: {}", e) })),
    };

    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,  // Skip entries we can't read
        };
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,  // Skip entries we can't stat
        };
        let name = entry.file_name().to_string_lossy().to_string();
        let size = if file_type.is_file() {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        } else { 0 };

        entries.push(serde_json::json!({
            "name": name,
            "is_dir": file_type.is_dir(),
            "is_file": file_type.is_file(),
            "is_symlink": file_type.is_symlink(),
            "size": size
        }));

        if entries.len() >= max_entries { break; }
    }

    // Sort: dirs first, then by name
    entries.sort_by(|a, b| {
        let a_dir = a["is_dir"].as_bool().unwrap_or(false);
        let b_dir = b["is_dir"].as_bool().unwrap_or(false);
        b_dir.cmp(&a_dir).then_with(|| {
            let a_name = a["name"].as_str().unwrap_or("");
            let b_name = b["name"].as_str().unwrap_or("");
            a_name.to_lowercase().cmp(&b_name.to_lowercase())
        })
    });

    Ok(serde_json::json!({
        "entries": entries,
        "count": entries.len(),
        "path": dir_path
    }))
}

fn tool_web_fetch_blocking(input: serde_json::Value) -> Result<serde_json::Value> {
    let url = input["url"].as_str().ok_or_else(|| anyhow!("url required"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let mut request = client.get(url);
    if let Some(headers) = input.get("headers").and_then(|h| h.as_object()) {
        for (key, value) in headers {
            if let Some(value_str) = value.as_str() {
                request = request.header(key.as_str(), value_str);
            }
        }
    }

    let response = request.send()?;

    if !response.status().is_success() {
        return Ok(serde_json::json!({
            "error": format!("HTTP error: {}", response.status()),
            "status": response.status().as_u16()
        }));
    }

    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();

    let body = response.text()?;

    Ok(serde_json::json!({
        "content": body,
        "content_type": content_type,
        "url": url
    }))
}

async fn tool_web_fetch_async(input: serde_json::Value) -> Result<serde_json::Value> {
    let url = input["url"].as_str().ok_or_else(|| anyhow!("url required"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;

    let mut request = client.get(url);
    if let Some(headers) = input.get("headers").and_then(|h| h.as_object()) {
        for (key, value) in headers {
            if let Some(value_str) = value.as_str() {
                request = request.header(key.as_str(), value_str);
            }
        }
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        return Ok(serde_json::json!({
            "error": format!("HTTP error: {}", response.status()),
            "status": response.status().as_u16()
        }));
    }

    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();

    // Limit content size to 1MB
    const MAX_CONTENT_SIZE: usize = 1_048_576;
    let body = response.text().await?;
    let truncated = body.len() > MAX_CONTENT_SIZE;
    let body = if truncated { crate::truncate::safe_truncate(&body, MAX_CONTENT_SIZE) } else { &body };

    // For HTML content, convert to plain text
    let output = if content_type.contains("text/html") {
        html_to_text(body)
    } else {
        body.to_string()
    };

    Ok(serde_json::json!({
        "content": output,
        "content_type": content_type,
        "url": url,
        "truncated": truncated
    }))
}

/// Convert HTML to readable plain text by stripping tags.
fn html_to_text(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;

    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                // Check for script/style tags
                let remaining = &html[html.len() - html.len()..]; // placeholder
                if result.ends_with('\n') || result.is_empty() {
                    // Already have a newline
                }
            }
            '>' => {
                in_tag = false;
                if in_script {
                    in_script = false;
                }
                if in_style {
                    in_style = false;
                }
                result.push('\n');
            }
            _ if in_tag => {
                // Inside a tag, skip content but detect script/style
            }
            _ if in_script || in_style => {}
            _ => {
                result.push(c);
            }
        }
    }

    // Collapse multiple newlines and trim
    let mut final_result = String::new();
    let mut prev_newline = false;
    for line in result.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_newline {
                final_result.push('\n');
                prev_newline = true;
            }
        } else {
            final_result.push_str(trimmed);
            final_result.push('\n');
            prev_newline = false;
        }
    }
    final_result.trim().to_string()
}

fn tool_web_search_blocking(input: serde_json::Value) -> Result<serde_json::Value> {
    let query = input["query"].as_str().ok_or_else(|| anyhow!("query required"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let search_url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1",
        urlencoding::encode(query)
    );

    let response = client.get(&search_url).send()?;

    if !response.status().is_success() {
        return Ok(serde_json::json!({
            "error": format!("Search failed: {}", response.status()),
            "results": []
        }));
    }

    #[derive(Deserialize)]
    struct DuckDuckGoResponse {
        #[serde(rename = "RelatedTopics")]
        related_topics: Vec<RelatedTopic>,
    }

    #[derive(Deserialize)]
    struct RelatedTopic {
        #[serde(rename = "Text")]
        text: Option<String>,
        #[serde(rename = "URL")]
        url: Option<String>,
    }

    match response.json::<DuckDuckGoResponse>() {
        Ok(data) => {
            let results: Vec<serde_json::Value> = data.related_topics
                .iter()
                .filter(|t| t.text.is_some())
                .take(10)
                .map(|t| serde_json::json!({
                    "title": t.text.as_deref().unwrap_or(""),
                    "url": t.url.as_deref().unwrap_or("")
                }))
                .collect();

            Ok(serde_json::json!({
                "results": results,
                "query": query
            }))
        }
        Err(_) => Ok(serde_json::json!({
            "error": "Failed to parse search response",
            "results": []
        })),
    }
}

fn run_git_command(args: &[&str], cwd: &str) -> Result<std::process::Output> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    cmd.output().map_err(|e| anyhow!("Failed to execute git: {}", e))
}

fn tool_git_status(input: serde_json::Value) -> Result<serde_json::Value> {
    let path = input["path"].as_str().ok_or_else(|| anyhow!("path required"))?;

    let output = run_git_command(&["status", "--porcelain"], path)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() && !stderr.is_empty() {
        return Ok(serde_json::json!({
            "error": stderr.trim(),
            "is_error": true
        }));
    }

    // 守卫短行：porcelain 合法行恒为 XY + 空格 + 路径（>=3 字节），len<3 时切切片会 panic
    let files: Vec<serde_json::Value> = stdout.lines().filter(|l| l.len() >= 3).map(|line| {
        let status = &line[..2];
        let file_path = &line[3..];
        serde_json::json!({
            "status": status.trim(),
            "file": file_path
        })
    }).collect();

    Ok(serde_json::json!({
        "files": files,
        "count": files.len(),
        "raw": stdout
    }))
}

fn tool_git_diff(input: serde_json::Value) -> Result<serde_json::Value> {
    let path = input["path"].as_str().ok_or_else(|| anyhow!("path required"))?;
    let staged = input["staged"].as_bool().unwrap_or(false);
    let file = input["file"].as_str();

    let mut args: Vec<&str> = vec!["diff"];
    if staged {
        args.push("--staged");
    }
    if let Some(f) = file {
        args.push("--");
        args.push(f);
    }

    let output = run_git_command(&args, path)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() && !stderr.is_empty() {
        return Ok(serde_json::json!({
            "error": stderr.trim(),
            "is_error": true
        }));
    }

    Ok(serde_json::json!({
        "diff": stdout,
        "staged": staged,
        "file": file
    }))
}

fn tool_git_log(input: serde_json::Value) -> Result<serde_json::Value> {
    let path = input["path"].as_str().ok_or_else(|| anyhow!("path required"))?;
    let count = input["count"].as_u64().unwrap_or(10);
    let oneline = input["oneline"].as_bool().unwrap_or(true);

    let mut args: Vec<String> = vec!["log".to_string()];
    if oneline {
        args.push("--oneline".to_string());
    }
    args.push(format!("-n{}", count));
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let output = run_git_command(&arg_refs, path)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() && !stderr.is_empty() {
        return Ok(serde_json::json!({
            "error": stderr.trim(),
            "is_error": true
        }));
    }

    let commits: Vec<serde_json::Value> = stdout.lines().filter(|l| !l.is_empty()).map(|line| {
        if oneline {
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            serde_json::json!({
                "hash": parts.first().unwrap_or(&""),
                "message": parts.get(1).unwrap_or(&"")
            })
        } else {
            serde_json::json!({ "raw": line })
        }
    }).collect();

    Ok(serde_json::json!({
        "commits": commits,
        "count": commits.len(),
        "raw": stdout
    }))
}

fn tool_git_commit(input: serde_json::Value) -> Result<serde_json::Value> {
    let path = input["path"].as_str().ok_or_else(|| anyhow!("path required"))?;
    let message = input["message"].as_str().ok_or_else(|| anyhow!("message required"))?;

    let add_output = run_git_command(&["add", "-A"], path)?;
    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr).to_string();
        if !stderr.is_empty() {
            return Ok(serde_json::json!({
                "error": format!("git add failed: {}", stderr.trim()),
                "is_error": true
            }));
        }
    }

    let commit_output = run_git_command(&["commit", "-m", message], path)?;

    let stdout = String::from_utf8_lossy(&commit_output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&commit_output.stderr).to_string();

    if !commit_output.status.success() {
        return Ok(serde_json::json!({
            "error": format!("git commit failed: {}", if stderr.is_empty() { &stdout } else { &stderr }.trim()),
            "is_error": true
        }));
    }

    Ok(serde_json::json!({
        "success": true,
        "output": stdout,
        "message": message
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolExecutionStatus {
    Running,
    Completed,
    Canceled,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionInfo {
    pub task_id: String,
    pub tool_name: String,
    pub status: ToolExecutionStatus,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub struct ToolExecutionManager {
    executions: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
}

impl ToolExecutionManager {
    pub fn new() -> Self {
        Self {
            executions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn execute_with_cancel(
        &self,
        task_id: String,
        tool_name: &str,
        input: serde_json::Value,
        cwd: &str,
    ) -> Result<serde_json::Value> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.executions.write().await.insert(task_id.clone(), tx);

        let result = tokio::select! {
            _ = rx => {
                Ok(serde_json::json!({
                    "error": "Tool execution canceled",
                    "is_error": true,
                    "canceled": true
                }))
            }
            res = execute_tool_async(tool_name, input, cwd) => res
        };

        self.executions.write().await.remove(&task_id);
        result
    }

    pub async fn cancel_execution(&self, task_id: &str) -> bool {
        if let Some(tx) = self.executions.write().await.remove(task_id) {
            let _ = tx.send(());
            true
        } else {
            false
        }
    }

    pub async fn is_running(&self, task_id: &str) -> bool {
        self.executions.read().await.contains_key(task_id)
    }

    pub async fn get_running_tasks(&self) -> Vec<String> {
        self.executions.read().await.keys().cloned().collect()
    }
}

fn tool_git_add(input: serde_json::Value) -> Result<serde_json::Value> {
    let path = input["path"].as_str().ok_or_else(|| anyhow!("path required"))?;
    let files = input["files"].as_array().ok_or_else(|| anyhow!("files array required"))?;

    let file_strs: Vec<String> = files.iter()
        .filter_map(|f| f.as_str().map(String::from))
        .collect();

    if file_strs.is_empty() {
        return Ok(serde_json::json!({
            "error": "No files specified",
            "is_error": true
        }));
    }

    let mut args: Vec<String> = vec!["add".to_string()];
    args.extend(file_strs.clone());
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let output = run_git_command(&arg_refs, path)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() && !stderr.is_empty() {
        return Ok(serde_json::json!({
            "error": stderr.trim(),
            "is_error": true
        }));
    }

    Ok(serde_json::json!({
        "success": true,
        "files": file_strs,
        "output": stdout
    }))
}

async fn tool_web_search_async(input: serde_json::Value) -> Result<serde_json::Value> {
    let query = input["query"].as_str().ok_or_else(|| anyhow!("query required"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let search_url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1",
        urlencoding::encode(query)
    );

    let response = client.get(&search_url).send().await?;

    if !response.status().is_success() {
        return Ok(serde_json::json!({
            "error": format!("Search failed: {}", response.status()),
            "results": []
        }));
    }

    #[derive(Deserialize)]
    struct DuckDuckGoResponse {
        #[serde(rename = "RelatedTopics")]
        related_topics: Vec<RelatedTopic>,
    }

    #[derive(Deserialize)]
    struct RelatedTopic {
        #[serde(rename = "Text")]
        text: Option<String>,
        #[serde(rename = "URL")]
        url: Option<String>,
    }

    match response.json::<DuckDuckGoResponse>().await {
        Ok(data) => {
            let results: Vec<serde_json::Value> = data.related_topics
                .iter()
                .filter(|t| t.text.is_some())
                .take(10)
                .map(|t| serde_json::json!({
                    "title": t.text.as_deref().unwrap_or(""),
                    "url": t.url.as_deref().unwrap_or("")
                }))
                .collect();

            Ok(serde_json::json!({
                "results": results,
                "query": query
            }))
        }
        Err(_) => Ok(serde_json::json!({
            "error": "Failed to parse search response",
            "results": []
        })),
    }
}
