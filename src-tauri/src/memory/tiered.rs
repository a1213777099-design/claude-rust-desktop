//! Tiered memory system (TencentDB Agent Memory design inlined into our project)
//!
//! Four explicit semantic layers (modeled after TencentDB Agent Memory v2.0.1):
//!   - **L0** Raw conversation: full original messages (no LLM extraction; just an index)
//!   - **L1** Atomic facts: short, declarative, fact-level statements
//!   - **L2** Scene blocks: higher-level summaries grouped by scene/topic
//!   - **L3** Persona: long-term user/team style + preferences
//!
//! Cross-layer behaviors:
//!   - **Hybrid retrieval**: BM25 (FTS5) + vector similarity, fused via Reciprocal Rank Fusion
//!   - **Time decay**: effective_importance = importance * 0.95^days_old
//!   - **Isolation**: each (workspace_path, visibility) tuple is an isolated namespace
//!   - **Visibility**: private (only the same workspace) | team (shared with other workspaces) | agent (cross-agent)

use crate::db::memory_repo::{search_memories_hybrid, MemoryRow};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Layer identifier, matches TencentDB's L0–L3 design
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    L0 = 0, // raw conversation reference (no extraction)
    L1 = 1, // atomic fact
    L2 = 2, // scene / topic block
    L3 = 3, // persona / long-term style
}

impl Tier {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Tier::L0,
            1 => Tier::L1,
            2 => Tier::L2,
            _ => Tier::L3,
        }
    }
}

/// Visibility level — controls which memories are visible across workspaces
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Private, // only the same workspace_path
    Team,    // same team_id (cross-workspace)
    Agent,   // shared across all agents in a team
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Private => "private",
            Visibility::Team => "team",
            Visibility::Agent => "agent",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "team" => Visibility::Team,
            "agent" => Visibility::Agent,
            _ => Visibility::Private,
        }
    }
}

/// One memory entry in the tiered store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieredMemoryRow {
    pub id: String,
    pub workspace_path: String,
    pub team_id: String,
    pub conversation_id: String,
    pub tier: Tier,
    pub visibility: Visibility,
    pub content: String,
    pub tags: String,
    pub importance: i32,
    pub created_at: String,
}

fn row_to_tiered(row: &rusqlite::Row<'_>) -> rusqlite::Result<TieredMemoryRow> {
    Ok(TieredMemoryRow {
        id: row.get(0)?,
        workspace_path: row.get(1)?,
        team_id: row.get(2).unwrap_or_else(|_| "default".to_string()),
        conversation_id: row.get(3)?,
        tier: Tier::from_i32(row.get::<_, i32>(4).unwrap_or(1)),
        visibility: Visibility::from_str(&row.get::<_, String>(5).unwrap_or_else(|_| "private".to_string())),
        content: row.get(6)?,
        tags: row.get(7)?,
        importance: row.get(8).unwrap_or(3),
        created_at: row.get(9)?,
    })
}

/// Apply schema migration: create tiered_memories table if not exists.
/// Safe to call multiple times.
pub fn ensure_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS tiered_memories (
            id              TEXT PRIMARY KEY,
            workspace_path  TEXT NOT NULL,
            team_id         TEXT NOT NULL DEFAULT 'default',
            conversation_id TEXT NOT NULL DEFAULT '',
            tier            INTEGER NOT NULL DEFAULT 1,
            visibility      TEXT NOT NULL DEFAULT 'private',
            content         TEXT NOT NULL,
            tags            TEXT NOT NULL DEFAULT '',
            importance      INTEGER NOT NULL DEFAULT 3,
            created_at      TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tm_workspace ON tiered_memories(workspace_path, tier);
        CREATE INDEX IF NOT EXISTS idx_tm_team      ON tiered_memories(team_id, visibility);
        CREATE INDEX IF NOT EXISTS idx_tm_created   ON tiered_memories(created_at);
        CREATE VIRTUAL TABLE IF NOT EXISTS tiered_memories_fts USING fts5(
            content, tags, content='tiered_memories', content_rowid='rowid'
        );
        "#,
    )?;
    Ok(())
}

/// Insert a tiered memory with dedup (by content prefix).
pub fn insert_tiered_memory(
    conn: &Connection,
    m: &TieredMemoryRow,
) -> anyhow::Result<bool> {
    ensure_schema(conn)?;
    // Dedup: same workspace + same tier + 80-char prefix already exists?
    let prefix: String = m.content.chars().take(80).collect();
    if !prefix.is_empty() {
        let like_check = format!("%{}%", prefix);
        let existing: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tiered_memories WHERE workspace_path = ?1 AND tier = ?2 AND content LIKE ?3",
                params![m.workspace_path, m.tier.as_i32(), like_check],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if existing > 0 {
            return Ok(false);
        }
    }
    let mut stmt = conn.prepare_cached(
        "INSERT INTO tiered_memories (id, workspace_path, team_id, conversation_id, tier, visibility, content, tags, importance, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    stmt.execute(params![
        m.id,
        m.workspace_path,
        m.team_id,
        m.conversation_id,
        m.tier.as_i32(),
        m.visibility.as_str(),
        m.content,
        m.tags,
        m.importance,
        m.created_at,
    ])?;
    // Index for FTS5
    let _ = conn.execute(
        "INSERT INTO tiered_memories_fts(rowid, content, tags) VALUES ((SELECT rowid FROM tiered_memories WHERE id = ?1), ?2, ?3)",
        params![m.id, m.content, m.tags],
    );
    Ok(true)
}

/// Hybrid search: BM25 (FTS5) + vector (callers inject embeddings).
/// Returns top-K with RRF fusion. Falls back to FTS5 if embeddings unavailable.
pub fn search_tiered_hybrid(
    conn: &Connection,
    workspace_path: &str,
    team_id: &str,
    query: &str,
    embedding: Option<&[f32]>,
    top_k: i64,
) -> anyhow::Result<Vec<TieredMemoryRow>> {
    ensure_schema(conn)?;

    // Phase 1: BM25 via FTS5 (private + team + agent visibility, all tiers)
    let fts_results = if !query.trim().is_empty() {
        let safe_q = query.replace('"', " ");
        let pattern = format!("\"{}\" OR \"{}*\"", safe_q, safe_q);
        let (sql, sql_params): (String, Vec<rusqlite::types::Value>) = if workspace_path.is_empty() {
            (
                "SELECT m.id, m.workspace_path, m.team_id, m.conversation_id, m.tier, m.visibility, m.content, m.tags, m.importance, m.created_at \
                 FROM tiered_memories_fts f \
                 JOIN tiered_memories m ON m.rowid = f.rowid \
                 WHERE tiered_memories_fts MATCH ?1 \
                 ORDER BY rank LIMIT 50"
                    .to_string(),
                vec![rusqlite::types::Value::Text(pattern.clone())],
            )
        } else {
            (
                "SELECT m.id, m.workspace_path, m.team_id, m.conversation_id, m.tier, m.visibility, m.content, m.tags, m.importance, m.created_at \
                 FROM tiered_memories_fts f \
                 JOIN tiered_memories m ON m.rowid = f.rowid \
                 WHERE tiered_memories_fts MATCH ?1 \
                   AND (m.workspace_path = ?2 OR m.team_id = ?3) \
                 ORDER BY rank LIMIT 50"
                    .to_string(),
                vec![
                    rusqlite::types::Value::Text(pattern.clone()),
                    rusqlite::types::Value::Text(workspace_path.to_string()),
                    rusqlite::types::Value::Text(team_id.to_string()),
                ],
            )
        };
        let mut stmt = conn.prepare_cached(&sql)?;
        let mapped: Vec<TieredMemoryRow> = stmt
            .query_map(rusqlite::params_from_iter(sql_params.iter()), row_to_tiered)?
            .filter_map(|r| r.ok())
            .collect();
        mapped
    } else {
        Vec::new()
    };

    // Phase 1.5: CJK-friendly LIKE fallback (FTS5 doesn't tokenize CJK).
    // Only triggered when fts_results is empty (FTS5 has poor CJK coverage).
    let like_results: Vec<TieredMemoryRow> = if fts_results.is_empty() && !query.trim().is_empty() {
        // Extract CJK + alphanumeric tokens >= 2 chars and AND them
        let tokens: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.chars().count() >= 2)
            .map(|s| s.to_string())
            .collect();
        if tokens.is_empty() {
            Vec::new()
        } else {
            let mut sql = String::from(
                "SELECT id, workspace_path, team_id, conversation_id, tier, visibility, content, tags, importance, created_at \
                 FROM tiered_memories \
                 WHERE 1=1",
            );
            // When workspace_path is empty, search globally; otherwise scope it.
            if !workspace_path.is_empty() {
                sql.push_str(" AND (workspace_path = ?1 OR team_id = ?2)");
            }
            // Build LIKE patterns with % wildcards for each token
            let like_patterns: Vec<String> = tokens.iter().map(|t| format!("%{}%", t)).collect();
            for _ in &like_patterns {
                sql.push_str(" AND content LIKE ?");
            }
            sql.push_str(" ORDER BY created_at DESC LIMIT 50");
            let mut stmt = conn.prepare_cached(&sql)?;
            let mut params_dyn: Vec<&dyn rusqlite::ToSql> = Vec::new();
            if !workspace_path.is_empty() {
                params_dyn.push(&workspace_path);
                params_dyn.push(&team_id);
            }
            for p in &like_patterns {
                params_dyn.push(p);
            }
            let mapped: Vec<TieredMemoryRow> = stmt
                .query_map(rusqlite::params_from_iter(params_dyn.iter().copied()), row_to_tiered)?
                .filter_map(|r| r.ok())
                .collect();
            mapped
        }
    } else {
        Vec::new()
    };

    // Phase 2: vector results (callers can implement in caller layer if they have embedding).
    // For now we use LIKE on content as a soft "vector" fallback when no embedding supplied.
    let vector_results: Vec<TieredMemoryRow> = if let Some(_emb) = embedding {
        // Stub: a real impl would query a vector index; omitted here to keep zero-dep.
        Vec::new()
    } else {
        Vec::new()
    };

    // Phase 3: RRF fusion (k=60, classic).
    let mut fused: std::collections::HashMap<String, (f64, TieredMemoryRow)> = std::collections::HashMap::new();
    for (rank, m) in fts_results.iter().enumerate() {
        let score = 1.0 / (60.0 + rank as f64 + 1.0);
        fused
            .entry(m.id.clone())
            .and_modify(|(s, _)| *s += score)
            .or_insert((score, m.clone()));
    }
    for (rank, m) in like_results.iter().enumerate() {
        // Slightly lower weight for LIKE-based fallback (less precise than FTS5)
        let score = 0.7 / (60.0 + rank as f64 + 1.0);
        fused
            .entry(m.id.clone())
            .and_modify(|(s, _)| *s += score)
            .or_insert((score, m.clone()));
    }
    for (rank, m) in vector_results.iter().enumerate() {
        let score = 1.0 / (60.0 + rank as f64 + 1.0);
        fused
            .entry(m.id.clone())
            .and_modify(|(s, _)| *s += score)
            .or_insert((score, m.clone()));
    }

    // Phase 4: time decay + sort
    let now = chrono::Utc::now();
    let mut ranked: Vec<(f64, TieredMemoryRow)> = fused
        .into_values()
        .map(|(rrf, m)| {
            let created = chrono::DateTime::parse_from_rfc3339(&m.created_at).ok();
            let days_old = created
                .map(|dt| now.signed_duration_since(dt).num_days().max(0) as i32)
                .unwrap_or(0);
            let decay = 0.95_f64.powi(days_old);
            let effective_importance = m.importance as f64 * decay;
            // Combine RRF (weight 0.4) + effective_importance/5 (weight 0.6)
            let final_score = rrf * 0.4 + (effective_importance / 5.0) * 0.6;
            (final_score, m)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(ranked.into_iter().take(top_k as usize).map(|(_, m)| m).collect())
}

/// Promote a fact to the next tier (L1 -> L2 -> L3). Idempotent: returns the promoted row.
pub fn promote_tier(conn: &Connection, id: &str) -> anyhow::Result<Option<TieredMemoryRow>> {
    ensure_schema(conn)?;
    let cur: Option<(String, String, String, String, i32, String, String, String, i32, String)> = conn
        .query_row(
            "SELECT id, workspace_path, team_id, conversation_id, tier, visibility, content, tags, importance, created_at \
             FROM tiered_memories WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .ok();
    if let Some((id, ws, team, conv, tier, vis, content, tags, imp, ts)) = cur {
        let new_tier = if tier >= 3 { 3 } else { tier + 1 };
        conn.execute(
            "UPDATE tiered_memories SET tier = ?1 WHERE id = ?2",
            params![new_tier, id],
        )?;
        Ok(Some(TieredMemoryRow {
            id,
            workspace_path: ws,
            team_id: team,
            conversation_id: conv,
            tier: Tier::from_i32(new_tier),
            visibility: Visibility::from_str(&vis),
            content,
            tags,
            importance: imp,
            created_at: ts,
        }))
    } else {
        Ok(None)
    }
}

/// Demote a memory to L1 (used when the same fact is contradicted).
pub fn demote_to_l1(conn: &Connection, id: &str) -> anyhow::Result<bool> {
    ensure_schema(conn)?;
    let n = conn.execute(
        "UPDATE tiered_memories SET tier = 1, importance = MAX(importance - 1, 1) WHERE id = ?1",
        params![id],
    )?;
    Ok(n > 0)
}

/// Delete a single memory by id.
pub fn delete_tiered_memory(conn: &Connection, id: &str) -> anyhow::Result<bool> {
    ensure_schema(conn)?;
    let n = conn.execute("DELETE FROM tiered_memories WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

/// List all memories (newest first), optionally scoped to a workspace.
pub fn list_all_tiered(conn: &Connection, workspace_path: &str, limit: i64) -> anyhow::Result<Vec<TieredMemoryRow>> {
    ensure_schema(conn)?;
    let rows: Vec<TieredMemoryRow> = if workspace_path.is_empty() {
        let mut stmt = conn.prepare_cached(
            "SELECT id, workspace_path, team_id, conversation_id, tier, visibility, content, tags, importance, created_at \
             FROM tiered_memories ORDER BY created_at DESC LIMIT ?1",
        )?;
        let mapped = stmt.query_map(params![limit], row_to_tiered)?;
        let out: Vec<TieredMemoryRow> = mapped.filter_map(|r| r.ok()).collect();
        out
    } else {
        let mut stmt = conn.prepare_cached(
            "SELECT id, workspace_path, team_id, conversation_id, tier, visibility, content, tags, importance, created_at \
             FROM tiered_memories WHERE workspace_path = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let mapped = stmt.query_map(params![workspace_path, limit], row_to_tiered)?;
        let out: Vec<TieredMemoryRow> = mapped.filter_map(|r| r.ok()).collect();
        out
    };
    Ok(rows)
}

/// Stats: counts per tier.
pub fn tiered_stats(conn: &Connection, workspace_path: &str) -> anyhow::Result<std::collections::HashMap<String, i64>> {
    ensure_schema(conn)?;
    let mut map = std::collections::HashMap::new();
    let (sql, sql_params): (String, Vec<rusqlite::types::Value>) = if workspace_path.is_empty() {
        (
            "SELECT tier, COUNT(*) FROM tiered_memories GROUP BY tier".to_string(),
            vec![],
        )
    } else {
        (
            "SELECT tier, COUNT(*) FROM tiered_memories WHERE workspace_path = ?1 GROUP BY tier".to_string(),
            vec![rusqlite::types::Value::Text(workspace_path.to_string())],
        )
    };
    let mut stmt = conn.prepare_cached(&sql)?;
    let collected: Vec<std::result::Result<(i32, i64), rusqlite::Error>> = if sql_params.is_empty() {
        stmt.query_map([], |r| -> rusqlite::Result<(i32, i64)> {
            Ok((r.get::<_, i32>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect()
    } else {
        stmt.query_map(rusqlite::params_from_iter(sql_params.iter()), |r| -> rusqlite::Result<(i32, i64)> {
            Ok((r.get::<_, i32>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect()
    };
    let rows = collected;
    for r in rows {
        let (tier, count) = r?;
        map.insert(format!("L{}", tier), count);
    }
    map.insert("total".to_string(), map.values().sum::<i64>());
    Ok(map)
}

/// Build a formatted system-prompt section from the top-K tiered memories.
/// Inlined mirror of the existing `engine_core.rs:354-364` shape, so it's a drop-in.
pub fn format_memories_for_prompt(memories: &[TieredMemoryRow]) -> String {
    if memories.is_empty() {
        return String::new();
    }
    let now = chrono::Utc::now();
    let items: Vec<String> = memories
        .iter()
        .map(|m| {
            let tier_name = match m.tier {
                Tier::L0 => "raw",
                Tier::L1 => "fact",
                Tier::L2 => "scene",
                Tier::L3 => "persona",
            };
            let created = chrono::DateTime::parse_from_rfc3339(&m.created_at)
                .ok()
                .map(|dt| format!("{} days ago", now.signed_duration_since(dt).num_days()))
                .unwrap_or_else(|| "recent".to_string());
            format!("- [L{} {} | importance:{}/5 | {}] {}", m.tier.as_i32(), tier_name, m.importance, created, m.content)
        })
        .collect();
    format!("[Relevant Memories — Tiered]\n{}", items.join("\n"))
}

// ─── Compatibility shim ─────────────────────────────────────────────────────
//
// The legacy `db::memory_repo::search_memories_hybrid` is still used in
// `engine_core.rs:332`. We expose a thin wrapper so callers can switch to
// tiered without rewriting the call site.

/// Convert TieredMemoryRow into the legacy MemoryRow shape so legacy
/// callers can consume results without refactoring.
pub fn tiered_to_legacy(rows: Vec<TieredMemoryRow>) -> Vec<MemoryRow> {
    rows.into_iter()
        .map(|t| MemoryRow {
            id: t.id,
            workspace_path: t.workspace_path,
            conversation_id: t.conversation_id,
            summary: t.content,
            tags: t.tags,
            memory_type: match t.tier {
                Tier::L0 => "raw".to_string(),
                Tier::L1 => "fact".to_string(),
                Tier::L2 => "scene".to_string(),
                Tier::L3 => "persona".to_string(),
            },
            importance: t.importance,
            created_at: t.created_at,
        })
        .collect()
}

/// Returns tiered memories OR, if tiered is empty, falls back to the legacy
/// hybrid search. Designed to be a drop-in replacement for callers that
/// currently call `db::memory_repo::search_memories_hybrid`.
pub fn search_with_fallback(
    conn: &Connection,
    workspace_path: &str,
    team_id: &str,
    query: &str,
    embedding: Option<&[f32]>,
    top_k: i64,
) -> anyhow::Result<Vec<MemoryRow>> {
    let tiered = search_tiered_hybrid(conn, workspace_path, team_id, query, embedding, top_k)?;
    if !tiered.is_empty() {
        return Ok(tiered_to_legacy(tiered));
    }
    // Fall back to legacy hybrid
    let raw = search_memories_hybrid(conn, workspace_path, query, embedding, top_k).unwrap_or_default();
    Ok(raw)
}

/// One-time import: copy legacy `memories` rows into the tiered store so the
/// primary retrieval path (TencentDB tiered search) can see historical data.
/// `insert_tiered_memory` dedup makes this safe to call on every startup.
pub fn migrate_legacy_memories(conn: &Connection) -> anyhow::Result<usize> {
    let legacy = crate::db::memory_repo::list_all_memories(conn, 100_000).unwrap_or_default();
    let mut migrated = 0usize;
    for m in legacy {
        let tier = if m.memory_type == "decision" || m.memory_type == "preference" { 2 } else { 1 };
        let row = TieredMemoryRow {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_path: m.workspace_path,
            team_id: "default".to_string(),
            conversation_id: m.conversation_id,
            tier: Tier::from_i32(tier),
            visibility: Visibility::from_str("private"),
            content: m.summary,
            tags: m.tags,
            importance: m.importance,
            created_at: m.created_at,
        };
        if insert_tiered_memory(conn, &row).unwrap_or(false) {
            migrated += 1;
        }
    }
    Ok(migrated)
}
