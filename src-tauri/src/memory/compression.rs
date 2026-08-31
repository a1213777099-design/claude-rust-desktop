/// Smart context compression using semantic embeddings and clustering.
/// Keeps latest N messages intact, compresses older ones by semantic similarity.
use anyhow::Result;
use std::sync::Arc;
use crate::memory::embedding::EmbeddingEngine;
use crate::memory::clustering;

pub struct ContextCompressor {
    max_tokens: usize,
    embedding_engine: Option<Arc<EmbeddingEngine>>,
    keep_recent: usize,
}

impl ContextCompressor {
    pub fn new(max_tokens: usize, embedding_engine: Option<Arc<EmbeddingEngine>>) -> Self {
        Self { max_tokens, embedding_engine, keep_recent: 3 }
    }

    pub fn with_keep_recent(mut self, n: usize) -> Self {
        self.keep_recent = n;
        self
    }

    /// Compress messages. Call compress_with_embeddings if you have pre-computed embeddings.
    pub fn compress(&self, messages: Vec<CompressibleMessage>) -> Result<Vec<CompressibleMessage>> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }
        let total_tokens: usize = messages.iter().map(|m| estimate_tokens(&m.content)).sum();
        if total_tokens <= self.max_tokens {
            return Ok(messages);
        }
        let split_point = messages.len().saturating_sub(self.keep_recent);
        let (older, newer) = messages.split_at(split_point);
        if older.is_empty() {
            return Ok(messages);
        }
        let compressed_older = self.simple_compress(older);
        let mut result = compressed_older;
        result.extend(newer.to_vec());
        Ok(result)
    }

    /// Compress with pre-computed embeddings for semantic clustering.
    pub fn compress_with_embeddings(
        &self,
        messages: Vec<CompressibleMessage>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<Vec<CompressibleMessage>> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }
        let total_tokens: usize = messages.iter().map(|m| estimate_tokens(&m.content)).sum();
        if total_tokens <= self.max_tokens {
            return Ok(messages);
        }
        let split_point = messages.len().saturating_sub(self.keep_recent);
        let (older, newer) = messages.split_at(split_point);
        if older.is_empty() {
            return Ok(messages);
        }
        let older_embeddings: Vec<Vec<f32>> = embeddings.into_iter().skip(split_point).collect();
        let compressed_older = if older_embeddings.len() == older.len() {
            self.semantic_compress(older, &older_embeddings)?
        } else {
            self.simple_compress(older)
        };
        let mut result = compressed_older;
        result.extend(newer.to_vec());
        Ok(result)
    }

    fn semantic_compress(&self, messages: &[CompressibleMessage], embeddings: &[Vec<f32>]) -> Result<Vec<CompressibleMessage>> {
        let pairs: Vec<(String, Vec<f32>)> = messages.iter()
            .zip(embeddings.iter())
            .enumerate()
            .map(|(i, (_, emb))| (i.to_string(), emb.clone()))
            .collect();

        let num_clusters = (messages.len() / 4).max(2).min(messages.len());
        let assignments = clustering::cluster_memories(&pairs, num_clusters)?;

        let mut clusters: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
        for (id_str, cluster_id) in &assignments {
            if let Ok(idx) = id_str.parse::<usize>() {
                clusters.entry(*cluster_id).or_default().push(idx);
            }
        }

        let mut compressed: Vec<CompressibleMessage> = Vec::new();
        for (_, indices) in &clusters {
            let best_idx = indices.iter()
                .max_by_key(|&&i| messages[i].content.len())
                .copied()
                .unwrap_or(indices[0]);
            if indices.len() == 1 {
                compressed.push(messages[best_idx].clone());
            } else {
                let mut msg = messages[best_idx].clone();
                let other_count = indices.len() - 1;
                msg.content = format!("[{} related messages merged] {}", other_count + 1, msg.content);
                compressed.push(msg);
            }
        }
        compressed.sort_by_key(|m| m.timestamp);
        Ok(compressed)
    }

    fn simple_compress(&self, messages: &[CompressibleMessage]) -> Vec<CompressibleMessage> {
        let joined: Vec<&str> = messages.iter().map(|m| m.content.as_str()).collect();
        let text = joined.join("\n");
        let max_chars = 500.min(text.len());
        vec![CompressibleMessage {
            role: "system".to_string(),
            content: format!("[Previous context summary ({} messages): {}]", messages.len(), &text[..max_chars]),
            timestamp: messages.first().map(|m| m.timestamp).unwrap_or(0),
        }]
    }

    pub fn merge_consecutive(&self, messages: Vec<CompressibleMessage>) -> Vec<CompressibleMessage> {
        if messages.is_empty() { return Vec::new(); }
        let mut result: Vec<CompressibleMessage> = Vec::new();
        let mut current = messages[0].clone();
        for msg in messages.into_iter().skip(1) {
            if msg.role == current.role {
                current.content.push_str("\n");
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
