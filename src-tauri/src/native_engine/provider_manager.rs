use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub api_format: ApiFormat,
    pub models: Vec<ModelConfig>,
    pub enabled: bool,
    pub web_search_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiFormat {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAI,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub max_tokens: Option<u32>,
    pub context_window: Option<u32>,
    pub context_size: Option<u32>,   // 模型上下文大小（token 数）
    pub supports_vision: bool,
    pub supports_web_search: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub provider: Provider,
    pub model: ModelConfig,
}

pub struct ProviderManager {
    providers: Vec<Provider>,
    config_path: PathBuf,
}

impl ProviderManager {
    pub fn new(config_path: PathBuf) -> Self {
        let mut manager = Self {
            providers: Vec::new(),
            config_path,
        };
        manager.load();
        manager
    }

    pub fn from_providers(providers: Vec<Provider>, config_path: PathBuf) -> Self {
        let manager = Self {
            providers,
            config_path,
        };
        if let Err(e) = manager.save() {
            tracing::error!(target: "providermanager", "Failed to save synced providers: {}", e);
        }
        manager
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub fn load(&mut self) {
        if self.config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&self.config_path) {
                if let Ok(providers) = serde_json::from_str::<Vec<Provider>>(&content) {
                    self.providers = providers;
                    tracing::info!(target: "providermanager", "Loaded {} providers from {}", self.providers.len(), self.config_path.display());
                }
            }
        }
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.providers)?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn resolve_provider(&self, model_id: &str) -> Option<ResolvedProvider> {
        let mut first_match: Option<ResolvedProvider> = None;
        // Strip -thinking suffix for model lookup (thinking is a parameter, not a separate model)
        let base_model = model_id.strip_suffix("-thinking").unwrap_or(model_id);
        
        // Model alias mapping for common mismatches
        let aliases: std::collections::HashMap<&str, Vec<&str>> = [
            ("deepseek-v4-pro", vec!["deepseek-chat"]),
            ("deepseek-v4-flash", vec!["deepseek-reasoner"]),
        ].iter().cloned().collect();
        
        // Build list of IDs to try: original + aliases
        let mut ids_to_try = vec![base_model];
        if let Some(alias_list) = aliases.get(base_model) {
            ids_to_try.extend(alias_list.iter().copied());
        }
        // Also check if model_id is an alias of another model
        for (canonical, alias_list) in &aliases {
            if alias_list.contains(&base_model) {
                ids_to_try.push(canonical);
            }
        }
        
        for try_id in &ids_to_try {
            for provider in &self.providers {
                if !provider.enabled {
                    continue;
                }
                
                for model in &provider.models {
                    if model.id == *try_id && model.enabled {
                        if first_match.is_none() {
                            first_match = Some(ResolvedProvider {
                                provider: provider.clone(),
                                model: model.clone(),
                            });
                        } else {
                            tracing::warn!(target: "provider_manager", "model \"{}\" exists in multiple providers. Using first match.", model_id);
                        }
                        break;
                    }
                }
                
                if first_match.is_some() {
                    break;
                }
            }
            if first_match.is_some() {
                break;
            }
        }

        if let Some(resolved) = &first_match {
            tracing::info!(target: "provider_manager", "Resolved \"{}\" -> \"{}\" ({})", model_id, resolved.provider.name, resolved.provider.base_url);
        } else {
            tracing::warn!(target: "provider_manager", "No provider found for \"{}\"", model_id);
        }

        first_match
    }

    pub fn normalize_base_url(url: &str) -> String {
        let clean = url.trim_end_matches('/');
        let clean = clean
            .strip_suffix("/chat/completions")
            .or_else(|| clean.strip_suffix("/messages"))
            .or_else(|| clean.strip_suffix("/anthropic"))
            .or_else(|| clean.strip_suffix("/v1"))
            .unwrap_or(clean);
        clean.trim_end_matches('/').to_string()
    }

    pub fn list_providers(&self) -> &[Provider] {
        &self.providers
    }

    pub fn update_provider(&mut self, id: &str, provider: Provider) {
        if let Some(idx) = self.providers.iter().position(|p| p.id == id) {
            self.providers[idx] = provider;
            tracing::info!(target: "providermanager", "Updated provider: {}", id);
        } else {
            self.providers.push(provider);
            tracing::info!(target: "providermanager", "Added new provider: {}", id);
        }
        if let Err(e) = self.save() {
            tracing::error!(target: "providermanager", "Failed to save providers: {}", e);
        } else {
            tracing::info!(target: "providermanager", "Providers saved successfully to {}", self.config_path.display());
        }
    }

    pub fn delete_provider(&mut self, id: &str) {
        self.providers.retain(|p| p.id != id);
        tracing::info!(target: "providermanager", "Deleted provider: {}", id);
        if let Err(e) = self.save() {
            tracing::error!(target: "providermanager", "Failed to save providers after deletion: {}", e);
        } else {
            tracing::info!(target: "providermanager", "Providers saved successfully after deletion");
        }
    }
}

pub fn get_default_context_size(model_id: &str) -> u32 {
    match model_id {
        id if id.contains("opus") || id.contains("claude-opus") => 200_000,
        id if id.contains("haiku") || id.contains("claude-haiku") => 200_000,
        id if id.contains("gpt-4o") || id.contains("claude-3.5") || id.contains("claude-sonnet") || id.contains("sonnet") => 200_000,
        id if id.contains("gpt-4-turbo") || id.contains("claude-3") => 128_000,
        id if id.contains("gpt-4") => 8_192,
        id if id.contains("gpt-3.5") => 16_384,
        id if id.contains("claude-2") => 100_000,
        id if id.contains("deepseek") => 128_000,
        id if id.contains("qwen") || id.contains("qwq") => 128_000,
        id if id.contains("gemini") => 1_000_000,
        _ => 200_000,
    }
}
