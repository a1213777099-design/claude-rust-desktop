/// Embedding engine: fastembed local ONNX + API-based + TF-IDF fallback.
/// Priority: local ONNX > API > TF-IDF hashing trick.
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::OnceCell;

/// Global fastembed model instance (lazy init, shared across threads).
static LOCAL_MODEL: OnceCell<Arc<fastembed::TextEmbedding>> = OnceCell::const_new();

/// Embedding engine that generates vector representations of text.
#[derive(Debug)]
pub struct EmbeddingEngine {
    cache: RwLock<HashMap<String, Vec<f32>>>,
    pub dimension: usize,
    api_base: Option<String>,
    api_key: Option<String>,
    model: String,
    idf_cache: RwLock<HashMap<String, f64>>,
    doc_count: RwLock<usize>,
    /// Whether to prefer local ONNX inference
    use_local: bool,
}

impl EmbeddingEngine {
    /// Create engine with API-based provider.
    pub fn new_api(api_base: &str, api_key: &str, model: &str, dimension: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            dimension,
            api_base: Some(api_base.to_string()),
            api_key: Some(api_key.to_string()),
            model: model.to_string(),
            idf_cache: RwLock::new(HashMap::new()),
            doc_count: RwLock::new(0),
            use_local: false,
        }
    }

    /// Create a local ONNX engine (fastembed, no network needed).
    pub fn new_local(dimension: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            dimension,
            api_base: None,
            api_key: None,
            model: "fastembed-local".to_string(),
            idf_cache: RwLock::new(HashMap::new()),
            doc_count: RwLock::new(0),
            use_local: true,
        }
    }

    /// Create a TF-IDF only engine (no API calls, no ONNX).
    pub fn new_tfidf(dimension: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            dimension,
            api_base: None,
            api_key: None,
            model: "tfidf".to_string(),
            idf_cache: RwLock::new(HashMap::new()),
            doc_count: RwLock::new(0),
            use_local: false,
        }
    }

    /// Best-effort constructor: try local ONNX, fallback to TF-IDF.
    pub async fn new_auto(dimension: usize) -> Self {
        // Try to init fastembed
        let init_result = tokio::task::spawn_blocking(|| {
            {
                let mut opts = fastembed::InitOptions::default();
                opts.model_name = fastembed::EmbeddingModel::AllMiniLML6V2;
                fastembed::TextEmbedding::try_new(opts)
            }
        }).await;

        match init_result {
            Ok(Ok(model)) => {
                tracing::info!(target: "memory::embedding", "fastembed local ONNX model loaded (AllMiniLML6V2, 384d)");
                let _ = LOCAL_MODEL.set(Arc::new(model));
                Self::new_local(dimension)
            }
            Ok(Err(e)) => {
                tracing::warn!(target: "memory::embedding", "fastembed init failed ({}), falling back to TF-IDF", e);
                Self::new_tfidf(dimension)
            }
            Err(e) => {
                tracing::warn!(target: "memory::embedding", "fastembed spawn failed ({}), falling back to TF-IDF", e);
                Self::new_tfidf(dimension)
            }
        }
    }

    /// Spawn background fastembed model loading (non-blocking).
    pub fn spawn_local_init() {
        tokio::spawn(async {
            let result = tokio::task::spawn_blocking(|| {
                let cache_dir = dirs::cache_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("huggingface").join("hub");
                let mut opts = fastembed::InitOptions::default();
                opts.model_name = fastembed::EmbeddingModel::AllMiniLML6V2;
                opts.cache_dir = cache_dir;
                opts.show_download_progress = false;
                fastembed::TextEmbedding::try_new(opts)
            }).await;
            match result {
                Ok(Ok(model)) => {
                    tracing::info!(target: "memory::embedding", "fastembed local ONNX model loaded (AllMiniLML6V2)");
                    let _ = LOCAL_MODEL.set(Arc::new(model));
                }
                Ok(Err(e)) => tracing::warn!(target: "memory::embedding", "fastembed init failed: {}", e),
                Err(e) => tracing::warn!(target: "memory::embedding", "fastembed spawn failed: {}", e),
            }
        });
    }

    /// Generate embedding for a single text.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let cache_key = self.make_cache_key(text);
        if let Some(cached) = self.cache.read().ok().and_then(|c| c.get(&cache_key).cloned()) {
            return Ok(cached);
        }

        // Priority: local ONNX (if loaded) > API > TF-IDF
        let has_local = LOCAL_MODEL.get().is_some();
        let embedding = if self.use_local || has_local {
            self.embed_local(text).await.unwrap_or_else(|e| {
                tracing::warn!(target: "memory::embedding", "Local ONNX failed ({}), TF-IDF fallback", e);
                self.embed_tfidf(text)
            })
        } else if self.api_base.is_some() {
            self.embed_via_api(text).await.unwrap_or_else(|e| {
                tracing::warn!(target: "memory::embedding", "API embedding failed ({}), TF-IDF fallback", e);
                self.embed_tfidf(text)
            })
        } else {
            self.embed_tfidf(text)
        };

        if let Ok(mut cache) = self.cache.write() {
            cache.insert(cache_key, embedding.clone());
            if cache.len() > 10000 {
                let keys: Vec<String> = cache.keys().take(5000).cloned().collect();
                for k in keys { cache.remove(&k); }
            }
        }
        Ok(embedding)
    }

    /// Batch embedding.
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let has_local = LOCAL_MODEL.get().is_some();
        if self.use_local || has_local {
            match self.embed_batch_local(texts).await {
                Ok(r) => return Ok(r),
                Err(e) => tracing::warn!(target: "memory::embedding", "Batch local failed: {}", e),
            }
        }
        if self.api_base.is_some() {
            match self.embed_batch_via_api(texts).await {
                Ok(r) => return Ok(r),
                Err(e) => tracing::warn!(target: "memory::embedding", "Batch API failed: {}", e),
            }
        }
        let mut results = Vec::with_capacity(texts.len());
        for text in texts { results.push(self.embed(text).await?); }
        Ok(results)
    }

    /// Local ONNX embedding via fastembed.
    async fn embed_local(&self, text: &str) -> Result<Vec<f32>> {
        let model = LOCAL_MODEL.get()
            .ok_or_else(|| anyhow!("Local model not initialized"))?
            .clone();
        let text_owned = text.to_string();
        let embeddings = tokio::task::spawn_blocking(move || {
            model.embed(vec![text_owned], None)
        }).await.map_err(|e| anyhow!("spawn_blocking failed: {}", e))?
        .map_err(|e| anyhow!("fastembed error: {}", e))?;
        embeddings.into_iter().next()
            .ok_or_else(|| anyhow!("No embedding returned"))
    }

    /// Batch local ONNX embedding.
    async fn embed_batch_local(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let model = LOCAL_MODEL.get()
            .ok_or_else(|| anyhow!("Local model not initialized"))?
            .clone();
        let owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        tokio::task::spawn_blocking(move || {
            model.embed(owned, None)
        }).await.map_err(|e| anyhow!("spawn_blocking failed: {}", e))?
        .map_err(|e| anyhow!("fastembed batch error: {}", e))
    }

    /// Call OpenAI-compatible /v1/embeddings endpoint.
    async fn embed_via_api(&self, text: &str) -> Result<Vec<f32>> {
        let api_base = self.api_base.as_ref().ok_or_else(|| anyhow!("No API base"))?;
        let api_key = self.api_key.as_ref().ok_or_else(|| anyhow!("No API key"))?;
        let url = format!("{}/embeddings", api_base.trim_end_matches('/'));
        let client = reqwest::Client::new();
        let resp = client.post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({"input": text, "model": self.model, "dimensions": self.dimension}))
            .timeout(std::time::Duration::from_secs(30))
            .send().await?;
        if !resp.status().is_success() { return Err(anyhow!("Embedding API returned {}", resp.status())); }
        let body: serde_json::Value = resp.json().await?;
        let arr = body["data"][0]["embedding"].as_array()
            .ok_or_else(|| anyhow!("Invalid embedding response"))?;
        let embedding: Vec<f32> = arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect();
        if embedding.is_empty() { return Err(anyhow!("Empty embedding")); }
        Ok(embedding)
    }

    /// Batch API embedding.
    async fn embed_batch_via_api(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let api_base = self.api_base.as_ref().ok_or_else(|| anyhow!("No API base"))?;
        let api_key = self.api_key.as_ref().ok_or_else(|| anyhow!("No API key"))?;
        let url = format!("{}/embeddings", api_base.trim_end_matches('/'));
        let client = reqwest::Client::new();
        let resp = client.post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({"input": texts, "model": self.model, "dimensions": self.dimension}))
            .timeout(std::time::Duration::from_secs(60))
            .send().await?;
        if !resp.status().is_success() { return Err(anyhow!("Batch API returned {}", resp.status())); }
        let body: serde_json::Value = resp.json().await?;
        let data = body["data"].as_array().ok_or_else(|| anyhow!("Invalid batch response"))?;
        Ok(data.iter().map(|item| {
            item["embedding"].as_array().unwrap_or(&vec![]).iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()
        }).collect())
    }

    /// TF-IDF hashing trick fallback.
    fn embed_tfidf(&self, text: &str) -> Vec<f32> {
        let tokens = tokenize(text);
        let mut vector = vec![0.0f32; self.dimension];
        for token in &tokens {
            let idx = hash_token(token) as usize % self.dimension;
            vector[idx] += 1.0;
            let idx2 = hash_token(&format!("{}_2", token)) as usize % self.dimension;
            vector[idx2] += 0.5;
        }
        if let Ok(idf) = self.idf_cache.read() {
            if !idf.is_empty() {
                for (i, val) in vector.iter_mut().enumerate() {
                    if let Some(&v) = idf.get(&format!("bucket_{}", i)) { *val *= v as f32; }
                }
            }
        }
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 { for v in vector.iter_mut() { *v /= norm; } }
        vector
    }

    pub fn update_idf(&self, documents: &[&str]) {
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        for doc in documents {
            let seen: std::collections::HashSet<String> = tokenize(doc).into_iter().collect();
            for t in seen { *doc_freq.entry(t).or_insert(0) += 1; }
        }
        let n = documents.len().max(1) as f64;
        let idf: HashMap<String, f64> = doc_freq.into_iter()
            .map(|(k, df)| (k, (n / df.max(1) as f64).ln())).collect();
        if let Ok(mut c) = self.idf_cache.write() { *c = idf; }
        if let Ok(mut c) = self.doc_count.write() { *c = documents.len(); }
    }

    fn make_cache_key(&self, text: &str) -> String {
        text.chars().take(200).collect()
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase().split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2).map(|w| w.to_string()).collect()
}

fn hash_token(token: &str) -> u64 {
    let mut hash: u64 = 5381;
    for b in token.bytes() { hash = hash.wrapping_mul(33).wrapping_add(b as u64); }
    hash
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { return 0.0; }
    dot / (na * nb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tfidf_embedding() {
        let engine = EmbeddingEngine::new_tfidf(384);
        let emb = engine.embed_tfidf("rust programming language systems");
        assert_eq!(emb.len(), 384);
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &a) - 1.0).abs() < 0.01);
        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c) < 0.01);
    }
}