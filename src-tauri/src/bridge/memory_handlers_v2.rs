use crate::bridge::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct MemoryCreatePayload {
    pub summary: String,
    pub memory_type: Option<String>,
    pub importance: Option<i32>,
    pub tags: Option<String>,
    pub workspace_path: Option<String>,
    pub conversation_id: Option<String>,
}

pub async fn memories_create(
    State(state): State<AppState>,
    Json(payload): Json<MemoryCreatePayload>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Primary store: TencentDB tiered memory (legacy table retired)
            let memory_type = payload.memory_type.unwrap_or_else(|| "context".to_string());
            let tier = if memory_type == "decision" || memory_type == "preference" { 2 } else { 1 };
            let row = crate::memory::tiered::TieredMemoryRow {
                id: id.clone(),
                workspace_path: payload.workspace_path.unwrap_or_default(),
                team_id: "default".to_string(),
                conversation_id: payload.conversation_id.unwrap_or_default(),
                tier: crate::memory::tiered::Tier::from_i32(tier),
                visibility: crate::memory::tiered::Visibility::from_str("private"),
                content: payload.summary.clone(),
                tags: payload.tags.unwrap_or_else(|| "auto".to_string()),
                importance: payload.importance.unwrap_or(3),
                created_at: now.clone(),
            };
            crate::memory::tiered::insert_tiered_memory(conn, &row)?;
            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({
                "ok": true,
                "id": id,
                "created_at": now,
            }))
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"ok": false, "error": "Failed to create memory"})),
    }
}

#[derive(Deserialize)]
pub struct MemoryUpdatePayload {
    pub summary: Option<String>,
    pub memory_type: Option<String>,
    pub importance: Option<i32>,
    pub tags: Option<String>,
}

pub async fn memories_update(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<MemoryUpdatePayload>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Primary store: TencentDB tiered memory (legacy table retired)
            let mut sets = Vec::new();
            let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut param_idx = 1;

            if let Some(ref summary) = payload.summary {
                sets.push(format!("content = ?{}", param_idx));
                params_vec.push(Box::new(summary.clone()));
                param_idx += 1;
            }
            if let Some(ref memory_type) = payload.memory_type {
                let tier = if memory_type == "decision" || memory_type == "preference" { 2 } else { 1 };
                sets.push(format!("tier = ?{}", param_idx));
                params_vec.push(Box::new(tier));
                param_idx += 1;
            }
            if let Some(importance) = payload.importance {
                sets.push(format!("importance = ?{}", param_idx));
                params_vec.push(Box::new(importance));
                param_idx += 1;
            }
            if let Some(ref tags) = payload.tags {
                sets.push(format!("tags = ?{}", param_idx));
                params_vec.push(Box::new(tags.clone()));
                param_idx += 1;
            }

            if sets.is_empty() {
                return Ok::<serde_json::Value, anyhow::Error>(
                    serde_json::json!({"ok": false, "error": "No fields to update"}),
                );
            }

            let sql = format!(
                "UPDATE tiered_memories SET {} WHERE id = ?{}",
                sets.join(", "),
                param_idx
            );
            params_vec.push(Box::new(id.clone()));

            let mut stmt = conn.prepare_cached(&sql)?;
            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                params_vec.iter().map(|p| p.as_ref()).collect();
            stmt.execute(params_refs.as_slice())?;

            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({"ok": true}))
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"ok": false, "error": "Failed to update memory"})),
    }
}

pub async fn memories_tags(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Primary store: TencentDB tiered memory (legacy table retired)
            let all =
                crate::memory::tiered::list_all_tiered(conn, "", 10000).unwrap_or_default();
            let mut tag_map: std::collections::HashMap<String, i64> =
                std::collections::HashMap::new();
            for m in &all {
                for tag in m.tags.split(',').filter(|t| !t.trim().is_empty()) {
                    let t = tag.trim().to_string();
                    *tag_map.entry(t).or_insert(0) += 1;
                }
            }
            let mut tags: Vec<serde_json::Value> = tag_map
                .into_iter()
                .map(|(name, count)| serde_json::json!({"name": name, "count": count}))
                .collect();
            tags.sort_by(|a, b| {
                b["count"]
                    .as_i64()
                    .unwrap_or(0)
                    .cmp(&a["count"].as_i64().unwrap_or(0))
            });
            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({"tags": tags}))
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"tags": []})),
    }
}

#[derive(Deserialize)]
pub struct TagRenamePayload {
    pub old_name: String,
    pub new_name: String,
}

pub async fn memories_tag_rename(
    State(state): State<AppState>,
    Json(payload): Json<TagRenamePayload>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Primary store: TencentDB tiered memory (legacy table retired)
            let all =
                crate::memory::tiered::list_all_tiered(conn, "", 10000).unwrap_or_default();
            let mut updated = 0i64;
            for m in &all {
                let tags: Vec<&str> = m.tags.split(',').map(|t| t.trim()).collect();
                if tags.contains(&payload.old_name.as_str()) {
                    let new_tags: Vec<String> = tags
                        .iter()
                        .map(|t| {
                            if *t == payload.old_name.as_str() {
                                payload.new_name.clone()
                            } else {
                                (*t).to_string()
                            }
                        })
                        .collect();
                    let tag_str = new_tags.join(",");
                    let _ = conn.execute(
                        "UPDATE tiered_memories SET tags = ?1 WHERE id = ?2",
                        rusqlite::params![tag_str, m.id],
                    );
                    updated += 1;
                }
            }
            Ok::<serde_json::Value, anyhow::Error>(
                serde_json::json!({"ok": true, "updated": updated}),
            )
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"ok": false, "error": "Failed to rename tag"})),
    }
}

#[derive(Deserialize)]
pub struct TagsMergePayload {
    pub source_tags: Vec<String>,
    pub target_tag: String,
}

pub async fn memories_tags_merge(
    State(state): State<AppState>,
    Json(payload): Json<TagsMergePayload>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Primary store: TencentDB tiered memory (legacy table retired)
            let all =
                crate::memory::tiered::list_all_tiered(conn, "", 10000).unwrap_or_default();
            let mut updated = 0i64;
            for m in &all {
                let tags: Vec<&str> = m.tags.split(',').map(|t| t.trim()).collect();
                let has_source = payload
                    .source_tags
                    .iter()
                    .any(|st| tags.contains(&st.as_str()));
                if has_source {
                    let mut new_tags: Vec<String> = tags
                        .iter()
                        .filter(|t| !payload.source_tags.contains(&(*t).to_string()))
                        .map(|t| (*t).to_string())
                        .collect();
                    if !new_tags.contains(&payload.target_tag) {
                        new_tags.push(payload.target_tag.clone());
                    }
                    let tag_str = new_tags.join(",");
                    let _ = conn.execute(
                        "UPDATE tiered_memories SET tags = ?1 WHERE id = ?2",
                        rusqlite::params![tag_str, m.id],
                    );
                    updated += 1;
                }
            }
            Ok::<serde_json::Value, anyhow::Error>(
                serde_json::json!({"ok": true, "updated": updated}),
            )
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"ok": false, "error": "Failed to merge tags"})),
    }
}

#[derive(Deserialize)]
pub struct TagDeletePayload {
    pub tag_name: String,
    pub remove_from_memories: Option<bool>,
}

pub async fn memories_tag_delete(
    State(state): State<AppState>,
    Json(payload): Json<TagDeletePayload>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            if payload.remove_from_memories.unwrap_or(true) {
                // Primary store: TencentDB tiered memory (legacy table retired)
                let all =
                    crate::memory::tiered::list_all_tiered(conn, "", 10000).unwrap_or_default();
                let mut updated = 0i64;
                for m in &all {
                    let tags: Vec<&str> = m.tags.split(',').map(|t| t.trim()).collect();
                    if tags.contains(&payload.tag_name.as_str()) {
                        let new_tags: Vec<String> = tags
                            .iter()
                            .filter(|t| *t != &payload.tag_name.as_str())
                            .map(|t| (*t).to_string())
                            .collect();
                        let tag_str = new_tags.join(",");
                        let _ = conn.execute(
                            "UPDATE tiered_memories SET tags = ?1 WHERE id = ?2",
                            rusqlite::params![tag_str, m.id],
                        );
                        updated += 1;
                    }
                }
                Ok::<serde_json::Value, anyhow::Error>(
                    serde_json::json!({"ok": true, "updated": updated}),
                )
            } else {
                Ok::<serde_json::Value, anyhow::Error>(
                    serde_json::json!({"ok": true, "updated": 0}),
                )
            }
        })
    }).await;
    match result {
        Ok(Ok(Ok(data))) => Json(data),
        _ => Json(serde_json::json!({"ok": false, "error": "Failed to delete tag"})),
    }
}


/// Vector semantic search endpoint.
/// POST /api/memories/vector-search
/// Body: { "query": "...", "workspace": "...", "limit": 10 }
pub async fn memories_vector_search(
    State(state): State<crate::bridge::AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let query = payload["query"].as_str().unwrap_or("");
    let workspace = payload["workspace"].as_str().unwrap_or("");
    let limit = payload["limit"].as_i64().unwrap_or(10);

    if query.is_empty() {
        return Json(serde_json::json!({"results": [], "error": "query is required"}));
    }

    let db = state.db_manager.clone();
    let engine = state.embedding_engine.clone();
    let query_owned = query.to_string();
    let workspace_owned = workspace.to_string();

    let result = tokio::task::spawn_blocking(move || {
        // Generate embedding for the query
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
        let embedding = match rt {
            Ok(rt) => rt.block_on(engine.embed(&query_owned)).ok(),
            Err(_) => None,
        };

        db.with_conn(|conn| {
            // Primary store: TencentDB tiered memory (legacy table retired)
            crate::memory::tiered::search_with_fallback(
                conn,
                &workspace_owned,
                "default",
                &query_owned,
                embedding.as_deref(),
                limit,
            )
        })
    }).await;

    match result {
        Ok(Ok(Ok(memories))) => {
            let items: Vec<serde_json::Value> = memories.iter().map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "summary": m.summary,
                    "tags": m.tags,
                    "memory_type": m.memory_type,
                    "importance": m.importance,
                    "created_at": m.created_at,
                    "workspace_path": m.workspace_path,
                })
            }).collect();
            Json(serde_json::json!({"results": items, "count": items.len()}))
        }
        _ => Json(serde_json::json!({"results": [], "error": "search failed"}))
    }
}

/// Memory stats with vector index info.
/// GET /api/memories/vector-stats
pub async fn memories_vector_stats(
    State(state): State<crate::bridge::AppState>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Primary store: TencentDB tiered memory (legacy table retired)
            let vector_count = crate::memory::vector_index::count_vectors(conn).unwrap_or(0);
            let total_memories: i64 = conn
                .query_row("SELECT COUNT(*) FROM tiered_memories", [], |r| r.get(0))
                .unwrap_or(0);
            serde_json::json!({
                "vector_count": vector_count,
                "total_memories": total_memories,
                "embedding_model": "tfidf",
                "dimension": 384,
            })
        })
    }).await;
    match result {
        Ok(Ok(stats)) => Json(stats),
        _ => Json(serde_json::json!({"error": "failed to get stats"})),
    }
}
pub async fn memories_associations(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::memory::vector_index::get_associated_memories(conn, &id, 5, 0.5)
        })
    }).await;
    match result {
        Ok(Ok(Ok(associations))) => {
            let mut items: Vec<serde_json::Value> = Vec::new();
            for (id, score) in associations {
                items.push(serde_json::json!({"memory_id": id, "score": score}));
            }
            Json(serde_json::json!({"associations": items}))
        }
        _ => Json(serde_json::json!({"associations": []})),
    }
}
pub async fn memories_cluster(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let num_clusters = body.get("num_clusters").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let memories = crate::db::memory_repo::list_recent_memories(conn, "default", 1000)?;
            let mut embeddings: Vec<(String, Vec<f32>)> = Vec::new();
            for mem in &memories {
                if let Ok(emb) = crate::memory::vector_index::get_embedding(conn, &mem.id) {
                    embeddings.push((mem.id.clone(), emb));
                }
            }
            crate::memory::clustering::cluster_memories(&embeddings, num_clusters)
        })
    }).await;
    match result {
        Ok(Ok(Ok(clusters))) => {
            let items: Vec<serde_json::Value> = clusters.iter().map(|(id, cluster_id)| {
                serde_json::json!({"memory_id": id, "cluster_id": cluster_id})
            }).collect();
            Json(serde_json::json!({"clusters": items}))
        }
        _ => Json(serde_json::json!({"clusters": []})),
    }
}
pub async fn memories_compress(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let max_tokens = body.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(4000) as usize;
    let compressor = crate::memory::compression::ContextCompressor::new(max_tokens, None);
    Json(serde_json::json!({"ok": true, "max_tokens": max_tokens}))
}

// === Knowledge Base API ===

/// List knowledge base entries (memory_type = 'knowledge')
pub async fn knowledge_list(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let db = state.db_manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, summary, tags, importance, created_at, workspace_path, conversation_id 
                 FROM memories WHERE memory_type = 'knowledge' ORDER BY created_at DESC LIMIT 100"
            ).map_err(|e| anyhow::anyhow!(e))?;
            let rows = stmt.query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "summary": row.get::<_, String>(1)?,
                    "tags": row.get::<_, String>(2)?,
                    "importance": row.get::<_, i32>(3)?,
                    "created_at": row.get::<_, String>(4)?,
                    "workspace_path": row.get::<_, String>(5)?,
                    "conversation_id": row.get::<_, String>(6)?,
                    "memory_type": "knowledge"
                }))
            }).map_err(|e| anyhow::anyhow!(e))?;
            let mut items = Vec::new();
            for row in rows {
                items.push(row.map_err(|e| anyhow::anyhow!(e))?);
            }
            Ok::<Vec<serde_json::Value>, anyhow::Error>(items)
        })
    }).await;
    match result {
        Ok(Ok(Ok(items))) => Json(serde_json::json!({"knowledge": items, "count": items.len()})),
        _ => Json(serde_json::json!({"knowledge": [], "count": 0, "error": "failed to list knowledge"})),
    }
}

/// Create a knowledge base entry
pub async fn knowledge_create(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let summary = payload["summary"].as_str().unwrap_or("").to_string();
    let tags = payload["tags"].as_str().unwrap_or("knowledge").to_string();
    let importance = payload["importance"].as_i64().unwrap_or(4) as i32;
    let workspace_path = payload["workspace_path"].as_str().unwrap_or("default").to_string();
    
    if summary.is_empty() {
        return Json(serde_json::json!({"ok": false, "error": "summary is required"}));
    }
    
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let db = state.db_manager.clone();
    let id_clone = id.clone();
    let created_clone = created_at.clone();
    
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO memories (id, workspace_path, conversation_id, summary, tags, created_at, memory_type, importance) 
                 VALUES (?1, ?2, 'knowledge', ?3, ?4, ?5, 'knowledge', ?6)",
                rusqlite::params![id_clone, workspace_path, summary, tags, created_clone, importance],
            ).map_err(|e| anyhow::anyhow!(e))?;
            Ok::<(), anyhow::Error>(())
        })
    }).await;
    
    match result {
        Ok(Ok(Ok(()))) => Json(serde_json::json!({"ok": true, "id": id, "created_at": created_at})),
        _ => Json(serde_json::json!({"ok": false, "error": "failed to create knowledge entry"})),
    }
}

/// Search knowledge base entries
pub async fn knowledge_search(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let query = payload["query"].as_str().unwrap_or("").to_string();
    let limit = payload["limit"].as_i64().unwrap_or(20) as usize;
    
    if query.is_empty() {
        return Json(serde_json::json!({"results": [], "error": "query is required"}));
    }
    
    let db = state.db_manager.clone();
    let engine = state.embedding_engine.clone();
    
    let result = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
        let embedding = match rt {
            Ok(rt) => rt.block_on(engine.embed(&query)).ok(),
            Err(_) => None,
        };
        
        db.with_conn(|conn| {
            // Try FTS5 search first
            let fts_results: Vec<String> = if let Ok(mut stmt) = conn.prepare(
                "SELECT id FROM memories_fts WHERE memories_fts MATCH ?1 AND rowid IN (
                    SELECT rowid FROM memories WHERE memory_type = 'knowledge'
                ) LIMIT ?2"
            ) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
                    Ok(row.get::<_, String>(0)?)
                }) {
                    rows.filter_map(|r| r.ok()).collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            
            if !fts_results.is_empty() {
                let mut items = Vec::new();
                for id in fts_results {
                    if let Ok(mut stmt) = conn.prepare(
                        "SELECT id, summary, tags, importance, created_at FROM memories WHERE id = ?1"
                    ) {
                        if let Ok(mut rows) = stmt.query_map(rusqlite::params![id], |row| {
                            Ok(serde_json::json!({
                                "id": row.get::<_, String>(0)?,
                                "summary": row.get::<_, String>(1)?,
                                "tags": row.get::<_, String>(2)?,
                                "importance": row.get::<_, i32>(3)?,
                                "created_at": row.get::<_, String>(4)?,
                                "memory_type": "knowledge"
                            }))
                        }) {
                            if let Some(Ok(item)) = rows.next() {
                                items.push(item);
                            }
                        }
                    }
                }
                return Ok(items);
            }
            
            // Fallback to LIKE search
            let mut stmt = conn.prepare(
                "SELECT id, summary, tags, importance, created_at FROM memories 
                 WHERE memory_type = 'knowledge' AND (summary LIKE ?1 OR tags LIKE ?1) 
                 ORDER BY importance DESC, created_at DESC LIMIT ?2"
            ).map_err(|e| anyhow::anyhow!(e))?;
            
            let pattern = format!("%{}%", query);
            let rows = stmt.query_map(rusqlite::params![pattern, limit as i64], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "summary": row.get::<_, String>(1)?,
                    "tags": row.get::<_, String>(2)?,
                    "importance": row.get::<_, i32>(3)?,
                    "created_at": row.get::<_, String>(4)?,
                    "memory_type": "knowledge"
                }))
            }).map_err(|e| anyhow::anyhow!(e))?;
            
            let mut items = Vec::new();
            for row in rows {
                items.push(row.map_err(|e| anyhow::anyhow!(e))?);
            }
            Ok::<Vec<serde_json::Value>, anyhow::Error>(items)
        })
    }).await;
    
    match result {
        Ok(Ok(Ok(items))) => Json(serde_json::json!({"results": items, "count": items.len()})),
        _ => Json(serde_json::json!({"results": [], "error": "search failed"})),
    }
}