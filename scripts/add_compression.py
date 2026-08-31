import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src-tauri\src\memory\compression.rs'
impl = """/// Smart context compression for managing token limits.
use anyhow::Result;
use std::sync::Arc;
use crate::memory::embedding::EmbeddingEngine;

pub struct ContextCompressor {
    max_tokens: usize,
    embedding_engine: Option<Arc<EmbeddingEngine>>,
}

impl ContextCompressor {
    pub fn new(max_tokens: usize, embedding_engine: Option<Arc<EmbeddingEngine>>) -> Self {
        Self { max_tokens, embedding_engine }
    }

    pub fn compress(&self, messages: Vec<CompressibleMessage>) -> Result<Vec<CompressibleMessage>> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }
        let total_tokens: usize = messages.iter().map(|m| estimate_tokens(&m.content)).sum();
        if total_tokens <= self.max_tokens {
            return Ok(messages);
        }
        let keep_count = (messages.len() / 5).max(2);
        let split_point = messages.len().saturating_sub(keep_count);
        let (older, newer) = messages.split_at(split_point);
        let summary_content = older.iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\\n");
        let summary = CompressibleMessage {
            role: "system".to_string(),
            content: format!("[Previous context summary: {}]", &summary_content[..summary_content.len().min(500)]),
            timestamp: older.first().map(|m| m.timestamp).unwrap_or(0),
        };
        let mut result = vec![summary];
        result.extend(newer.to_vec());
        Ok(result)
    }

    pub fn merge_consecutive(&self, messages: Vec<CompressibleMessage>) -> Vec<CompressibleMessage> {
        if messages.is_empty() {
            return Vec::new();
        }
        let mut result: Vec<CompressibleMessage> = Vec::new();
        let mut current = messages[0].clone();
        for msg in messages.into_iter().skip(1) {
            if msg.role == current.role {
                current.content.push_str("\\n");
                current.content.push_str(&msg.content);
            } else {
                result.push(current);
                current = msg;
            }
        }
        result.push(current);
        result
    }
}

#[derive(Debug, Clone)]
pub struct CompressibleMessage {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

fn estimate_tokens(text: &str) -> usize {
    text.len() / 4 + 1
}
"""
with open(path, 'w', encoding='utf-8') as f:
    f.write(impl)
print('Added implementation')
