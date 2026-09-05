use crate::native_engine::provider_manager::ResolvedProvider;
use crate::tools::ToolDefinition;
use anyhow::Result;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::pin::Pin;
use tokio_stream::StreamExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: String,
    pub content: OpenAIContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OpenAIContent {
    Text(String),
    Multi(Vec<OpenAIContentPart>),
}

// 手写反序列化：网关返回的 content 可能是 null、数组、甚至对象，
// 严格的 unagged 派生会整体解析失败（表现为 error decoding response body）
impl<'de> Deserialize<'de> for OpenAIContent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        match v {
            Value::Null => Ok(OpenAIContent::Text(String::new())),
            Value::String(s) => Ok(OpenAIContent::Text(s)),
            Value::Array(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    match p.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                                out.push(OpenAIContentPart::Text { text: t.to_string() });
                            }
                        }
                        Some("image_url") => {
                            if let Some(img) = p.get("image_url").cloned()
                                .and_then(|i| serde_json::from_value::<ImageUrl>(i).ok()) {
                                out.push(OpenAIContentPart::Image { image_url: img });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(OpenAIContent::Multi(out))
            }
            other => Ok(OpenAIContent::Text(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OpenAIContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    Image { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

// 部分网关把 arguments 返回成对象而非字符串，这里统一转成字符串
impl<'de> Deserialize<'de> for FunctionCall {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let name = v.get("name").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        let arguments = match v.get("arguments") {
            Some(Value::String(s)) => s.clone(),
            Some(other) if other.is_object() || other.is_array() => other.to_string(),
            _ => String::new(),
        };
        Ok(FunctionCall { name, arguments })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIResponse {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub model: String,
    pub choices: Vec<OpenAIChoice>,
    pub usage: Option<OpenAIUsage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIChoice {
    #[serde(default)]
    pub index: usize,
    pub message: OpenAIMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIStreamChunk {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub model: String,
    pub choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIStreamChoice {
    #[serde(default)]
    pub index: usize,
    pub delta: OpenAIDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<FunctionCallDelta>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCallDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

pub struct OpenAIClient {
    client: Client,
}

impl OpenAIClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|e| { tracing::warn!(target: "http", "Client build failed: {}, using default", e); Client::new() }),
        }
    }

    pub async fn send_message(
        &self,
        provider: &ResolvedProvider,
        messages: Vec<OpenAIMessage>,
        system_prompt: Option<&str>,
        tools: Vec<ToolDefinition>,
        max_tokens: u32,
        _reasoning_effort: Option<&str>,
        extended_thinking: bool,
    ) -> Result<OpenAIResponse> {
        let base_url = crate::native_engine::provider_manager::ProviderManager::normalize_base_url(&provider.provider.base_url);
        let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));

        let mut body_messages = Vec::new();
        
        if let Some(system) = system_prompt {
            body_messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: OpenAIContent::Text(system.to_string()),
                tool_calls: None,
                tool_call_id: None,
                reasoning_content: None,
            });
        }
        
        body_messages.extend(messages);

        let mut body = json!({
            "model": provider.model.id,
            "max_tokens": max_tokens,
            "messages": body_messages,
        });

        if let Some(effort) = _reasoning_effort {
            body["reasoning_effort"] = json!(effort);
        }

        if extended_thinking {
            // Anthropic 形态（部分网关识别）+ OpenAI o 系形态 + Qwen/DashScope 形态
            // 三者同时下发：不识别的字段会被标准 OpenAI 端点忽略，识别其一即生效
            body["thinking"] = json!({"type": "enabled", "budget_tokens": 10000});
            body["reasoning"] = json!({"effort": "medium"});
            body["enable_thinking"] = json!(true);
        }

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools.iter().map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            }).collect();
            body["tools"] = json!(tool_defs);
        }

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", provider.provider.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            let head: String = text.chars().take(300).collect();
            anyhow::bail!("OpenAI API error {}: {}", status, head);
        }

        let data: OpenAIResponse = serde_json::from_str(&text).map_err(|e| {
            let head: String = text.chars().take(300).collect();
            anyhow::anyhow!("OpenAI response parse error ({}): body head: {}", e, head)
        })?;
        Ok(data)
    }

    pub async fn send_message_stream(
        &self,
        provider: &ResolvedProvider,
        messages: Vec<OpenAIMessage>,
        system_prompt: Option<&str>,
        tools: Vec<ToolDefinition>,
        max_tokens: u32,
        _reasoning_effort: Option<&str>,
        extended_thinking: bool,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let base_url = crate::native_engine::provider_manager::ProviderManager::normalize_base_url(&provider.provider.base_url);
        let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));

        let mut body_messages = Vec::new();
        
        if let Some(system) = system_prompt {
            body_messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: OpenAIContent::Text(system.to_string()),
                tool_calls: None,
                tool_call_id: None,
                reasoning_content: None,
            });
        }
        
        body_messages.extend(messages);

        let mut body = json!({
            "model": provider.model.id,
            "max_tokens": max_tokens,
            "messages": body_messages,
            "stream": true,
        });

        if let Some(effort) = _reasoning_effort {
            body["reasoning_effort"] = json!(effort);
        }

        if extended_thinking {
            // Anthropic 形态（部分网关识别）+ OpenAI o 系形态 + Qwen/DashScope 形态
            // 三者同时下发：不识别的字段会被标准 OpenAI 端点忽略，识别其一即生效
            body["thinking"] = json!({"type": "enabled", "budget_tokens": 10000});
            body["reasoning"] = json!({"effort": "medium"});
            body["enable_thinking"] = json!(true);
        }

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools.iter().map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            }).collect();
            body["tools"] = json!(tool_defs);
        }

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", provider.provider.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, text);
        }

        let stream = response.bytes_stream();
        let event_stream = stream
            .map(|chunk| {
                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        Ok(text.to_string())
                    }
                    Err(e) => Err(anyhow::anyhow!("Stream error: {}", e)),
                }
            });

        Ok(Box::pin(event_stream))
    }
}

impl Default for OpenAIClient {
    fn default() -> Self {
        Self::new()
    }
}
