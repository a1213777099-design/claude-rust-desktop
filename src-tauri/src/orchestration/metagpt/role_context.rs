/// Shared mutable context between roles.
///
/// Matches MetaGPT's RoleContext (rc) which allows roles to
/// share state like project info, config, and accumulated knowledge.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct RoleContext {
    inner: Arc<RwLock<RoleContextInner>>,
}

#[derive(Debug, Clone)]
struct RoleContextInner {
    /// Shared key-value store accessible by all roles
    pub data: HashMap<String, Value>,
    /// Project workspace path
    pub workspace: String,
    /// Current goal/description
    pub goal: String,
    /// Cost tracking
    pub total_cost: f64,
    /// Max token budget (None = unlimited)
    pub max_token_budget: Option<u64>,
    /// Current token usage
    pub tokens_used: u64,
}

impl RoleContext {
    pub fn new(workspace: &str, goal: &str) -> Self {
        Self {
            inner: Arc::new(RwLock::new(RoleContextInner {
                data: HashMap::new(),
                workspace: workspace.to_string(),
                goal: goal.to_string(),
                total_cost: 0.0,
                max_token_budget: None,
                tokens_used: 0,
            }))
        }
    }

    pub async fn get_workspace(&self) -> String {
        self.inner.read().await.workspace.clone()
    }

    pub async fn get_goal(&self) -> String {
        self.inner.read().await.goal.clone()
    }

    pub async fn set(&self, key: &str, value: Value) {
        self.inner.write().await.data.insert(key.to_string(), value);
    }

    pub async fn get(&self, key: &str) -> Option<Value> {
        self.inner.read().await.data.get(key).cloned()
    }

    pub async fn add_cost(&self, cost: f64) {
        self.inner.write().await.total_cost += cost;
    }

    pub async fn get_cost(&self) -> f64 {
        self.inner.read().await.total_cost
    }

    pub async fn add_tokens(&self, tokens: u64) {
        let mut inner = self.inner.write().await;
        inner.tokens_used += tokens;
    }

    pub async fn get_tokens(&self) -> u64 {
        self.inner.read().await.tokens_used
    }

    pub async fn is_over_budget(&self) -> bool {
        let inner = self.inner.read().await;
        match inner.max_token_budget {
            Some(budget) => inner.tokens_used >= budget,
            None => false,
        }
    }
}
