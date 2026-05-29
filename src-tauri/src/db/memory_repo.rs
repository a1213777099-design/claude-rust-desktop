use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRow {
    pub id: String,
    pub workspace_path: String,
    pub conversation_id: String,
    pub summary: String,
    pub tags: String,
    pub memory_type: String,
    pub importance: i32,
    pub created_at: String,
}

fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRow> {
    Ok(MemoryRow {
        id: row.get(0)?,
        workspace_path: row.get(1)?,
        conversation_id: row.get(2)?,
        summary: row.get(3)?,
        tags: row.get(4)?,
        memory_type: row.get(5).unwrap_or_else(|_| "context".to_string()),
        importance: row.get(6).unwrap_or(3),
        created_at: row.get(7)?,
    })
}

/// Insert a memory with V2 fields (type + importance), with dedup
pub fn insert_memory(
    conn: &Connection,
    id: &str,
    workspace_path: &str,
    conversation_id: &str,
    summary: &str,
    tags: &str,
    memory_type: &str,
    importance: i32,
    created_at: &str,
) -> Result<()> {
    // Dedup: skip if similar summary prefix already exists
    let prefix_chars: String = summary.chars().take(80).collect();
    if !prefix_chars.is_empty() {
        let like_check = format!("%{}%", prefix_chars);
        let existing: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE workspace_path = ?1 AND summary LIKE ?2",
            params![workspace_path, like_check],
            |row| row.get(0),
        ).unwrap_or(0);
        if existing > 0 {
            return Ok(());
        }
    }

    let mut stmt = conn.prepare_cached(
        "INSERT INTO memories (id, workspace_path, conversation_id, summary, tags, memory_type, importance, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    )?;
    stmt.execute(params![id, workspace_path, conversation_id, summary, tags, memory_type, importance, created_at])?;
    Ok(())
}

/// V1-compatible insert (backward compat)
pub fn insert_memory_v1(
    conn: &Connection,
    id: &str,
    workspace_path: &str,
    conversation_id: &str,
    summary: &str,
    tags: &str,
    created_at: &str,
) -> Result<()> {
    insert_memory(conn, id, workspace_path, conversation_id, summary, tags, "context", 3, created_at)
}

/// FTS5 full-text search with LIKE fallback
pub fn search_memories(conn: &Connection, workspace_path: &str, query: &str, limit: i64) -> Result<Vec<MemoryRow>> {
    let fts_result = conn.prepare_cached(
        "SELECT m.id, m.workspace_path, m.conversation_id, m.summary, m.tags, m.memory_type, m.importance, m.created_at \
         FROM memories m \
         INNER JOIN memories_fts fts ON m.rowid = fts.rowid \
         WHERE m.workspace_path = ?1 AND memories_fts MATCH ?2 \
         ORDER BY m.importance DESC, m.created_at DESC LIMIT ?3"
    ).and_then(|mut stmt| {
        let rows = stmt.query_map(params![workspace_path, query, limit], |row| row_to_memory(row))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    });

    match fts_result {
        Ok(rows) if !rows.is_empty() => Ok(rows),
        _ => {
            let like_query = format!("%{}%", query);
            let mut stmt = conn.prepare_cached(
                "SELECT id, workspace_path, conversation_id, summary, tags, memory_type, importance, created_at \
                 FROM memories WHERE workspace_path = ?1 AND (summary LIKE ?2 OR tags LIKE ?2) \
                 ORDER BY importance DESC, created_at DESC LIMIT ?3"
            )?;
            let rows = stmt.query_map(params![workspace_path, like_query, limit], |row| row_to_memory(row))?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        }
    }
}

/// List recent memories ordered by importance then recency
pub fn list_recent_memories(conn: &Connection, workspace_path: &str, limit: i64) -> Result<Vec<MemoryRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, workspace_path, conversation_id, summary, tags, memory_type, importance, created_at \
         FROM memories WHERE workspace_path = ?1 ORDER BY importance DESC, created_at DESC LIMIT ?2"
    )?;
    let rows = stmt.query_map(params![workspace_path, limit], |row| row_to_memory(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get high-importance memories for priority injection
pub fn get_important_memories(conn: &Connection, workspace_path: &str, limit: i64) -> Result<Vec<MemoryRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, workspace_path, conversation_id, summary, tags, memory_type, importance, created_at \
         FROM memories WHERE workspace_path = ?1 AND importance >= 4 \
         ORDER BY importance DESC, created_at DESC LIMIT ?2"
    )?;
    let rows = stmt.query_map(params![workspace_path, limit], |row| row_to_memory(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Count memories for a workspace
pub fn count_memories(conn: &Connection, workspace_path: &str) -> Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE workspace_path = ?1",
        params![workspace_path],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Phase 2: Smart summary extraction from messages
/// Focuses on USER messages; assistant only contributes conclusions
pub fn build_smart_summary(messages: &[crate::db::message_repo::MessageRow]) -> (String, String, i32) {
    let mut decisions = Vec::new();
    let mut preferences = Vec::new();
    let mut facts = Vec::new();
    let mut user_topics = Vec::new();

    for msg in messages.iter().rev().take(30) {
        let text: String = msg.content.chars().take(500).collect();
        let lower = text.to_lowercase();

        if msg.role == "user" {
            // Detect preferences
            if lower.contains("prefer") || lower.contains("i like") || lower.contains("i want")
                || lower.contains("don't") || lower.contains("do not")
                || text.contains("喜欢") || text.contains("偏好") || text.contains("不要") || text.contains("希望") {
                preferences.push(text.chars().take(200).collect::<String>());
            }
            // Detect decisions
            if lower.contains("let's use") || lower.contains("decide") || lower.contains("we'll go with")
                || text.contains("决定") || text.contains("改成") || text.contains("用这个") || text.contains("就这样") {
                decisions.push(text.chars().take(200).collect::<String>());
            }
            // Extract topic (first meaningful user message)
            if user_topics.len() < 3 && text.chars().count() > 5 {
                user_topics.push(text.chars().take(100).collect::<String>());
            }
        } else if msg.role == "assistant" {
            // Only extract short conclusion sentences from assistant
            for line in text.lines() {
                let line_trim = line.trim();
                if line_trim.chars().count() > 10 && line_trim.chars().count() < 120 {
                    if line_trim.starts_with("答案") || line_trim.starts_with("结论")
                        || line_trim.starts_with("所以") || line_trim.starts_with("建议")
                        || line_trim.starts_with("Answer:") || line_trim.starts_with("Conclusion:")
                        || line_trim.starts_with("So ") || line_trim.starts_with("Therefore") {
                        facts.push(line_trim.chars().take(150).collect::<String>());
                    }
                }
            }
        }
    }

    // Build summary - decisions first, then preferences, then topics
    let mut summary_parts = Vec::new();
    if !decisions.is_empty() {
        summary_parts.push(format!("Decisions: {}", decisions.join("; ")));
    }
    if !preferences.is_empty() {
        summary_parts.push(format!("Preferences: {}", preferences.join("; ")));
    }
    if !facts.is_empty() {
        summary_parts.push(format!("Key facts: {}", facts.join("; ")));
    }
    if !user_topics.is_empty() && summary_parts.is_empty() {
        summary_parts.push(format!("Topics: {}", user_topics.join("; ")));
    }

    // Determine memory_type: scan ALL content, not just first line
    let full_text = summary_parts.join(" ");
    let has_decisions = full_text.contains("Decisions:") || full_text.contains("决定") || full_text.contains("改成");
    let has_preferences = full_text.contains("Preferences:") || full_text.contains("喜欢") || full_text.contains("偏好") || full_text.contains("不要");

    let _memory_type = if has_decisions && has_preferences {
        "decision"
    } else if has_decisions {
        "decision"
    } else if has_preferences {
        "preference"
    } else if full_text.contains("Key facts:") {
        "fact"
    } else {
        "context"
    };

    let importance = if has_decisions && has_preferences {
        5
    } else if has_decisions || has_preferences {
        4
    } else if !facts.is_empty() {
        3
    } else {
        2
    };

    let summary = if summary_parts.is_empty() {
        messages.iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.chars().take(200).collect::<String>())
            .unwrap_or_else(|| "conversation".to_string())
    } else {
        summary_parts.join("\n")
    };

    // Clean tags: just "auto" + topic keywords (max 5 words)
    let tags = if user_topics.is_empty() {
        "auto".to_string()
    } else {
        let keywords: Vec<String> = user_topics.iter().take(1)
            .flat_map(|t| t.split_whitespace().take(5).map(|w| w.to_string()).collect::<Vec<_>>())
            .collect();
        if keywords.is_empty() { "auto".to_string() } else { format!("auto,{}", keywords.join(",")) }
    };

    (summary, tags, importance)
}/// Phase 5: Consolidate duplicate memories for a workspace
/// Merges memories with similar content, keeping the higher-importance one
pub fn consolidate_memories(conn: &Connection, workspace_path: &str) -> Result<i64> {
    let memories = list_recent_memories(conn, workspace_path, 200)?;
    let mut removed = 0i64;

    for i in 0..memories.len() {
        for j in (i + 1)..memories.len() {
            let a = &memories[i];
            let b = &memories[j];

            // Check if summaries overlap significantly (first 60 chars match)
            let a_prefix: String = a.summary.chars().take(60).collect();
            let b_prefix: String = b.summary.chars().take(60).collect();

            if a_prefix == b_prefix && !a_prefix.is_empty() {
                // Keep the one with higher importance, or the newer one
                let to_delete = if a.importance >= b.importance { &b.id } else { &a.id };
                let _ = conn.execute(
                    "DELETE FROM memories WHERE id = ?1",
                    params![to_delete],
                );
                removed += 1;
            }
        }
    }

    // Also prune: keep max 50 memories per workspace, delete oldest low-importance ones
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE workspace_path = ?1",
        params![workspace_path],
        |row| row.get(0),
    ).unwrap_or(0);

    if count > 50 {
        let to_remove = count - 50;
        conn.execute(
            "DELETE FROM memories WHERE id IN (SELECT id FROM memories WHERE workspace_path = ?1 AND importance <= 2 ORDER BY created_at ASC LIMIT ?2)",
            params![workspace_path, to_remove],
        )?;
    }

    Ok(removed)
}

/// List all memories across all workspaces
pub fn list_all_memories(conn: &Connection, limit: i64) -> Result<Vec<MemoryRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, workspace_path, conversation_id, summary, tags, memory_type, importance, created_at \n         FROM memories ORDER BY importance DESC, created_at DESC LIMIT ?1"
    )?;
    let rows = stmt.query_map(params![limit], |row| row_to_memory(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Search all memories across all workspaces
pub fn search_all_memories(conn: &Connection, query: &str, limit: i64) -> Result<Vec<MemoryRow>> {
    let like_query = format!("%{}%", query);
    let mut stmt = conn.prepare_cached(
        "SELECT id, workspace_path, conversation_id, summary, tags, memory_type, importance, created_at \n         FROM memories WHERE summary LIKE ?1 OR tags LIKE ?1 \n         ORDER BY importance DESC, created_at DESC LIMIT ?2"
    )?;
    let rows = stmt.query_map(params![like_query, limit], |row| row_to_memory(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}