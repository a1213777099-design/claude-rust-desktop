//! TencentDB Agent Memory client (SDK-style, inlined into our project)
//!
//! Calls TencentDB Agent Memory's v3 HTTP API directly. If the server is not
//! reachable, the wrapper falls back to our local tiered store so the caller
//! always gets an answer.
//!
//! Endpoints used (see INSTALL.md + TencentDB Agent Memory README):
//!   POST {base_url}/v3/memory/search     — Hybrid (BM25 + vector) search
//!   POST {base_url}/v3/memory/add        — Insert L1 atomic fact
//!   POST {base_url}/v3/memory/promote    — Promote to L2/L3
//!   GET  {base_url}/v3/persona/get       — L3 persona
//!   POST {base_url}/v3/persona/update    — L3 persona update
//!   GET  {base_url}/health               — Service health
//!   POST {base_url}/v3/meta/auth/verify  — Validate x-tdai-user-key
//!
//! Auth: x-tdai-user-key header
//! Isolation: every request carries teamId / agentId / userId (v3 strict isolation)

use crate::db::DbManager;
use crate::db::memory_repo::{search_memories_hybrid, MemoryRow};
use crate::memory::tiered::{
    ensure_schema as ensure_tiered_schema, format_memories_for_prompt, insert_tiered_memory,
    promote_tier, search_tiered_hybrid, tiered_stats, Tier, TieredMemoryRow, Visibility,
};
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Per-user TencentDB configuration. Persisted in the `tdai_config` table
/// (one row, id = 'default') and exposed via the bridge.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TencentDBConfig {
    /// base URL of the Memory Core, e.g. http://localhost:8420
    pub base_url: String,
    /// x-tdai-user-key (TencentDB-issued user key)
    pub user_key: String,
    /// team id (v3 strict isolation)
    pub team_id: String,
    /// agent id
    pub agent_id: String,
    /// user id
    pub user_id: String,
    /// space id (used in Anthropic proxy base_url, e.g. "default")
    pub space_id: String,
    /// when true, route all memory ops to TencentDB; when false, use local tiered store only
    pub enabled: bool,
}

/// Service health probe result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthInfo {
    pub reachable: bool,
    pub base_url: String,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    pub services: Option<serde_json::Value>,
}

/// API search hit returned by TencentDB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdaiSearchHit {
    pub id: String,
    pub content: String,
    pub tier: i32,
    pub visibility: String,
    pub importance: i32,
    pub tags: String,
    pub created_at: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TdaiSearchResponse {
    pub hits: Vec<TdaiSearchHit>,
    pub source: String, // "remote" or "local-fallback"
}

/// Auth verify response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthVerifyResponse {
    pub valid: bool,
    pub user_id: Option<String>,
    pub team_id: Option<String>,
    pub agent_id: Option<String>,
    pub error: Option<String>,
}

/// HTTP client wrapper. Cheap to clone.
#[derive(Clone)]
pub struct TdaiClient {
    http: reqwest::Client,
    cfg: Arc<RwLock<TencentDBConfig>>,
}

impl TdaiClient {
    pub fn new(cfg: TencentDBConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(3))
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
            cfg: Arc::new(RwLock::new(cfg)),
        }
    }

    pub async fn config(&self) -> TencentDBConfig {
        self.cfg.read().await.clone()
    }

    pub async fn update_config(&self, new_cfg: TencentDBConfig) {
        let mut g = self.cfg.write().await;
        *g = new_cfg;
    }

    /// Probe the TencentDB service. Returns reachability + latency. Does not require auth.
    pub async fn health(&self) -> HealthInfo {
        let cfg = self.cfg.read().await.clone();
        let base = cfg.base_url.trim_end_matches('/').to_string();
        if base.is_empty() {
            return HealthInfo {
                reachable: false,
                base_url: base,
                latency_ms: None,
                error: Some("base_url is empty".into()),
                services: None,
            };
        }
        let url = format!("{}/health", base);
        let started = std::time::Instant::now();
        let resp = self.http.get(&url).send().await;
        match resp {
            Ok(r) => {
                let latency = started.elapsed().as_millis() as u64;
                let status = r.status();
                let body = r.json::<serde_json::Value>().await.ok();
                HealthInfo {
                    reachable: status.is_success(),
                    base_url: base,
                    latency_ms: Some(latency),
                    error: if status.is_success() { None } else { Some(format!("HTTP {}", status)) },
                    services: body,
                }
            }
            Err(e) => HealthInfo {
                reachable: false,
                base_url: base,
                latency_ms: Some(started.elapsed().as_millis() as u64),
                error: Some(e.to_string()),
                services: None,
            },
        }
    }

    /// Verify the configured user_key against the v3 auth endpoint.
    pub async fn verify_auth(&self) -> AuthVerifyResponse {
        let cfg = self.cfg.read().await.clone();
        if cfg.base_url.is_empty() || cfg.user_key.is_empty() {
            return AuthVerifyResponse {
                valid: false,
                user_id: None,
                team_id: None,
                agent_id: None,
                error: Some("base_url or user_key empty".into()),
            };
        }
        let url = format!("{}/v3/meta/auth/verify", cfg.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "teamId": cfg.team_id,
            "agentId": cfg.agent_id,
            "userId": cfg.user_id,
            "userKey": cfg.user_key,
        });
        let resp = self.http.post(&url).json(&body).send().await;
        match resp {
            Ok(r) if r.status().is_success() => {
                let v: serde_json::Value = r.json().await.unwrap_or_default();
                AuthVerifyResponse {
                    valid: v.get("valid").and_then(|x| x.as_bool()).unwrap_or(false),
                    user_id: v.get("userId").and_then(|x| x.as_str()).map(String::from),
                    team_id: v.get("teamId").and_then(|x| x.as_str()).map(String::from),
                    agent_id: v.get("agentId").and_then(|x| x.as_str()).map(String::from),
                    error: None,
                }
            }
            Ok(r) => AuthVerifyResponse {
                valid: false,
                user_id: None,
                team_id: None,
                agent_id: None,
                error: Some(format!("HTTP {}", r.status())),
            },
            Err(e) => AuthVerifyResponse {
                valid: false,
                user_id: None,
                team_id: None,
                agent_id: None,
                error: Some(e.to_string()),
            },
        }
    }

    /// Hybrid search. Tries TencentDB first, falls back to local tiered store.
    ///
    /// The DB handle (`db`) is used ONLY for the synchronous local fallback.
    /// The remote (network) call runs without holding the DB lock, so we never
    /// block other database access while awaiting a network request. A prior
    /// version held a `MutexGuard` across `block_on(search_remote(...).await)`,
    /// which serialized all DB traffic behind a single network round-trip.
    pub async fn search(
        &self,
        workspace_path: &str,
        query: &str,
        top_k: i64,
        db: &DbManager,
    ) -> Result<TdaiSearchResponse> {
        let cfg = self.cfg.read().await.clone();
        if cfg.enabled && !cfg.base_url.is_empty() && !cfg.user_key.is_empty() {
            match self.search_remote(&cfg, query, top_k).await {
                Ok(hits) => return Ok(TdaiSearchResponse { hits, source: "remote".into() }),
                Err(e) => {
                    // log + fall through to local
                    eprintln!("[tdai] remote search failed: {}, falling back to local", e);
                }
            }
        }
        // local fallback — acquire the DB lock ONLY for this synchronous call
        let team_id = if cfg.team_id.is_empty() { "default".into() } else { cfg.team_id };
        let local = db
            .with_conn(|conn| search_tiered_hybrid(conn, workspace_path, &team_id, query, None, top_k))??;
        let hits = local
            .into_iter()
            .enumerate()
            .map(|(i, m)| TdaiSearchHit {
                id: m.id,
                content: m.content,
                tier: m.tier.as_i32(),
                visibility: m.visibility.as_str().to_string(),
                importance: m.importance,
                tags: m.tags,
                created_at: m.created_at,
                score: 1.0 / (i as f64 + 1.0),
            })
            .collect();
        Ok(TdaiSearchResponse { hits, source: "local-fallback".into() })
    }

    async fn search_remote(
        &self,
        cfg: &TencentDBConfig,
        query: &str,
        top_k: i64,
    ) -> Result<Vec<TdaiSearchHit>> {
        let url = format!("{}/v3/memory/search", cfg.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "teamId": cfg.team_id,
            "agentId": cfg.agent_id,
            "userId": cfg.user_id,
            "query": query,
            "topK": top_k,
        });
        let resp = self
            .http
            .post(&url)
            .header("x-tdai-user-key", &cfg.user_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("send: {}", e))?;
        if !resp.status().is_success() {
            return Err(anyhow!("HTTP {}", resp.status()));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| anyhow!("decode: {}", e))?;
        let arr = v.get("hits").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let mut hits: Vec<TdaiSearchHit> = Vec::with_capacity(arr.len());
        for item in arr {
            let h = TdaiSearchHit {
                id: item.get("id").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                content: item.get("content").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                tier: item.get("tier").and_then(|x| x.as_i64()).unwrap_or(1) as i32,
                visibility: item.get("visibility").and_then(|x| x.as_str()).unwrap_or("private").to_string(),
                importance: item.get("importance").and_then(|x| x.as_i64()).unwrap_or(3) as i32,
                tags: item.get("tags").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                created_at: item.get("created_at").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                score: item.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0),
            };
            hits.push(h);
        }
        Ok(hits)
    }

    /// Insert a new atomic fact with explicit tier. Tries remote first, always mirrors to local.
    pub async fn add_memory_with_tier(
        &self,
        workspace_path: &str,
        content: &str,
        importance: i32,
        tags: &str,
        tier: i32,
        conn: &Connection,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let cfg = self.cfg.read().await.clone();
        let team_id = if cfg.team_id.is_empty() { "default".into() } else { cfg.team_id.clone() };

        // remote (best effort)
        if cfg.enabled && !cfg.base_url.is_empty() && !cfg.user_key.is_empty() {
            let url = format!("{}/v3/memory/add", cfg.base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "teamId": cfg.team_id,
                "agentId": cfg.agent_id,
                "userId": cfg.user_id,
                "content": content,
                "tier": tier,
                "importance": importance,
                "tags": tags,
            });
            let _ = self
                .http
                .post(&url)
                .header("x-tdai-user-key", &cfg.user_key)
                .json(&body)
                .send()
                .await;
        }

        // local mirror (always)
        ensure_tiered_schema(conn)?;
        let local_tier = Tier::from_i32(tier);
        let row = TieredMemoryRow {
            id: id.clone(),
            workspace_path: workspace_path.to_string(),
            team_id,
            conversation_id: String::new(),
            tier: local_tier,
            visibility: Visibility::Private,
            content: content.to_string(),
            tags: tags.to_string(),
            importance,
            created_at: now,
        };
        let inserted = insert_tiered_memory(conn, &row)?;
        if !inserted {
            // dedup hit; query for an existing id
            let prefix: String = content.chars().take(80).collect();
            let like_check = format!("%{}%", prefix);
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM tiered_memories WHERE workspace_path = ?1 AND tier = ?2 AND content LIKE ?3 LIMIT 1",
                    rusqlite::params![workspace_path, tier, like_check],
                    |r| r.get(0),
                )
                .ok();
            if let Some(eid) = existing {
                return Ok(eid);
            }
        }
        Ok(id)
    }

    /// Backward-compatible alias: always L1.
    pub async fn add_memory(
        &self,
        workspace_path: &str,
        content: &str,
        importance: i32,
        tags: &str,
        conn: &Connection,
    ) -> Result<String> {
        self.add_memory_with_tier(workspace_path, content, importance, tags, 1, conn).await
    }

    /// Promote a memory to a higher tier (L1→L2→L3). Local-only for now.
    pub fn promote(&self, conn: &Connection, id: &str) -> Result<Option<TieredMemoryRow>> {
        promote_tier(conn, id)
    }

    /// Stats per tier (local). Always available.
    pub fn stats(&self, conn: &Connection, workspace_path: &str) -> Result<std::collections::HashMap<String, i64>> {
        tiered_stats(conn, workspace_path)
    }

    /// Build a formatted system-prompt section from the search results.
    /// Used by engine_core.rs when injecting memories into the prompt.
    pub fn format_for_prompt(&self, hits: &[TdaiSearchHit]) -> String {
        if hits.is_empty() {
            return String::new();
        }
        let now = chrono::Utc::now();
        let items: Vec<String> = hits
            .iter()
            .map(|h| {
                let tier_name = match h.tier {
                    0 => "raw",
                    1 => "fact",
                    2 => "scene",
                    _ => "persona",
                };
                let created = chrono::DateTime::parse_from_rfc3339(&h.created_at)
                    .ok()
                    .map(|dt| format!("{} days ago", now.signed_duration_since(dt).num_days()))
                    .unwrap_or_else(|| "recent".to_string());
                format!("- [L{} {} | importance:{}/5 | {}] {}", h.tier, tier_name, h.importance, created, h.content)
            })
            .collect();
        format!("[Relevant Memories — TencentDB]\n{}", items.join("\n"))
    }
}

// ─── Persistence of the config itself ───────────────────────────────────────

const TDAICONFIG_TABLE: &str = "tdai_config";
const TDAICONFIG_ID: &str = "default";

pub fn ensure_config_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {} (
            id TEXT PRIMARY KEY,
            base_url TEXT NOT NULL DEFAULT '',
            user_key TEXT NOT NULL DEFAULT '',
            team_id TEXT NOT NULL DEFAULT '',
            agent_id TEXT NOT NULL DEFAULT '',
            user_id TEXT NOT NULL DEFAULT '',
            space_id TEXT NOT NULL DEFAULT 'default',
            enabled INTEGER NOT NULL DEFAULT 0
        );",
        TDAICONFIG_TABLE
    ))?;
    Ok(())
}

pub fn load_config(conn: &Connection) -> Result<TencentDBConfig> {
    ensure_config_table(conn)?;
    let row: Option<(String, String, String, String, String, String, i64)> = conn
        .query_row(
            &format!(
                "SELECT base_url, user_key, team_id, agent_id, user_id, space_id, enabled FROM {} WHERE id = ?1",
                TDAICONFIG_TABLE
            ),
            rusqlite::params![TDAICONFIG_ID],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get::<_, i64>(6)?,
                ))
            },
        )
        .ok();
    Ok(match row {
        Some((b, k, t, a, u, s, e)) => TencentDBConfig {
            base_url: b,
            user_key: k,
            team_id: t,
            agent_id: a,
            user_id: u,
            space_id: s,
            enabled: e != 0,
        },
        None => TencentDBConfig::default(),
    })
}

pub fn save_config(conn: &Connection, cfg: &TencentDBConfig) -> Result<()> {
    ensure_config_table(conn)?;
    conn.execute(
        &format!(
            "INSERT INTO {} (id, base_url, user_key, team_id, agent_id, user_id, space_id, enabled) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(id) DO UPDATE SET \
                base_url=excluded.base_url, user_key=excluded.user_key, \
                team_id=excluded.team_id, agent_id=excluded.agent_id, user_id=excluded.user_id, \
                space_id=excluded.space_id, enabled=excluded.enabled",
            TDAICONFIG_TABLE
        ),
        rusqlite::params![
            TDAICONFIG_ID,
            cfg.base_url,
            cfg.user_key,
            cfg.team_id,
            cfg.agent_id,
            cfg.user_id,
            cfg.space_id,
            if cfg.enabled { 1i64 } else { 0i64 },
        ],
    )?;
    Ok(())
}

// Legacy compatibility: search_with_fallback now routes via TdaiClient if present.
// (engine_core.rs:332 still calls the pure-local path; this is a convenience helper
// for callers that already hold a TdaiClient.)

/// Pure local fallback used when TdaiClient is unavailable (e.g. legacy path).
pub fn local_search_only(
    conn: &Connection,
    workspace_path: &str,
    query: &str,
    top_k: i64,
) -> Result<Vec<MemoryRow>> {
    let raw = search_memories_hybrid(conn, workspace_path, query, None, top_k).unwrap_or_default();
    Ok(raw)
}

/// Format a batch of TieredMemoryRow for the system prompt (local path).
pub fn format_tiered_for_prompt(rows: &[TieredMemoryRow]) -> String {
    format_memories_for_prompt(rows)
}
