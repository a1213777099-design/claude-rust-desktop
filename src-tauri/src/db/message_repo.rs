use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub thinking: Option<String>,
    pub created_at: String,
    pub is_compact_boundary: bool,
    pub sort_order: i64,
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        thinking: row.get(4)?,
        created_at: row.get(5)?,
        is_compact_boundary: row.get::<_, i64>(6)? != 0,
        sort_order: row.get(7)?,
    })
}

pub fn insert_message(
    conn: &Connection,
    id: &str,
    conversation_id: &str,
    role: &str,
    content: &str,
    thinking: Option<&str>,
    created_at: &str,
    is_compact_boundary: bool,
    sort_order: i64,
) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO messages (id, conversation_id, role, content, thinking, created_at, is_compact_boundary, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    )?;
    stmt.execute(params![
        id,
        conversation_id,
        role,
        content,
        thinking,
        created_at,
        is_compact_boundary as i64,
        sort_order,
    ])?;
    Ok(())
}

pub fn get_messages_by_conversation(conn: &Connection, conversation_id: &str) -> Result<Vec<MessageRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, conversation_id, role, content, thinking, created_at, is_compact_boundary, sort_order FROM messages WHERE conversation_id = ?1 ORDER BY sort_order ASC"
    )?;
    let rows = stmt.query_map(params![conversation_id], |row| row_to_message(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_message(conn: &Connection, id: &str) -> Result<Option<MessageRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, conversation_id, role, content, thinking, created_at, is_compact_boundary, sort_order FROM messages WHERE id = ?1"
    )?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(Some(row_to_message(row)?)),
        None => Ok(None),
    }
}

// 追加消息必须用 MAX(sort_order)+1 而不是消息条数：compact 保留尾部消息的原序号，
// 条数与序号脱节（如 0,24..28 共 6 条），按条数分配会从 6 起追加并在 24 处撞号，
// 造成角色序列错乱（连续双 user/双 assistant），弱工具模型随之退化。
pub fn next_sort_order(conn: &Connection, conversation_id: &str) -> i64 {
    conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM messages WHERE conversation_id = ?1",
        params![conversation_id],
        |row| row.get(0),
    ).unwrap_or(0)
}

pub fn delete_message(conn: &Connection, id: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached("DELETE FROM messages WHERE id = ?1")?;
    stmt.execute(params![id])?;
    Ok(())
}

pub fn delete_messages_from(conn: &Connection, conversation_id: &str, sort_order: i64) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "DELETE FROM messages WHERE conversation_id = ?1 AND sort_order >= ?2"
    )?;
    stmt.execute(params![conversation_id, sort_order])?;
    Ok(())
}

pub fn delete_messages_tail(conn: &Connection, conversation_id: &str, count: i64) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "DELETE FROM messages WHERE conversation_id = ?1 AND sort_order >= (SELECT MIN(sort_order) FROM (SELECT sort_order FROM messages WHERE conversation_id = ?1 ORDER BY sort_order DESC LIMIT ?2))"
    )?;
    stmt.execute(params![conversation_id, count])?;
    Ok(())
}

pub fn update_message_content(conn: &Connection, id: &str, content: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE messages SET content = ?1 WHERE id = ?2"
    )?;
    stmt.execute(params![content, id])?;
    Ok(())
}

pub fn delete_messages_before(
    conn: &Connection,
    conversation_id: &str,
    before_sort_order: i64,
) -> Result<()> {
    conn.execute(
        "DELETE FROM messages WHERE conversation_id = ?1 AND sort_order < ?2",
        params![conversation_id, before_sort_order],
    )?;
    Ok(())
}

pub fn count_messages(conn: &Connection, conversation_id: &str) -> Result<i64> {
    let mut stmt = conn.prepare_cached(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1"
    )?;
    let count: i64 = stmt.query_row(params![conversation_id], |row| row.get(0))?;
    Ok(count)
}

// ─── Tool call persistence ──────────────────────────────────────────────────
// 聊天引擎每轮的工具调用记录：持久化后前端重载会话时仍能渲染工具卡片

#[derive(Debug, Clone)]
pub struct ToolCallRow {
    pub id: String,
    pub message_id: String,
    pub name: String,
    pub input: String,
    pub output: String,
    pub is_error: bool,
    pub sort_order: i64,
}

pub fn insert_tool_call(
    conn: &Connection,
    id: &str,
    message_id: &str,
    name: &str,
    input: &str,
    output: &str,
    is_error: bool,
    sort_order: i64,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO tool_calls (id, message_id, name, input, output, is_error, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, message_id, name, input, output, is_error as i64, sort_order],
    )?;
    Ok(())
}

pub fn list_tool_calls_for_conversation(conn: &Connection, conversation_id: &str) -> Result<Vec<ToolCallRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT tc.id, tc.message_id, tc.name, COALESCE(tc.input, ''), COALESCE(tc.output, ''), tc.is_error, tc.sort_order \
         FROM tool_calls tc JOIN messages m ON m.id = tc.message_id \
         WHERE m.conversation_id = ?1 ORDER BY tc.sort_order ASC",
    )?;
    let rows = stmt.query_map(params![conversation_id], |row| {
        Ok(ToolCallRow {
            id: row.get(0)?,
            message_id: row.get(1)?,
            name: row.get(2)?,
            input: row.get(3)?,
            output: row.get(4)?,
            is_error: row.get::<_, i64>(5)? != 0,
            sort_order: row.get(6)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                thinking TEXT,
                created_at TEXT NOT NULL,
                is_compact_boundary INTEGER DEFAULT 0,
                sort_order INTEGER NOT NULL
            );"
        ).unwrap();
        conn
    }

    // TDD-S: compact 后序号与条数脱节（0,24..28 共 6 条），
    // next_sort_order 必须返回 MAX+1=29 而不是条数 6，否则追加到 24 时撞号。
    #[test]
    fn test_next_sort_order_after_compact_gap() {
        let conn = test_conn();
        let cid = "c1";
        for (i, so) in [0i64, 24, 25, 26, 27, 28].iter().enumerate() {
            insert_message(&conn, &format!("m{i}"), cid, "user", "x", None, "t", false, *so).unwrap();
        }
        assert_eq!(next_sort_order(&conn, cid), 29, "必须基于 MAX(sort_order)+1，而非条数");
    }

    #[test]
    fn test_next_sort_order_empty_conversation() {
        let conn = test_conn();
        assert_eq!(next_sort_order(&conn, "nobody"), 0);
    }

    #[test]
    fn test_next_sort_order_sequential() {
        let conn = test_conn();
        let cid = "c2";
        for i in 0..5i64 {
            insert_message(&conn, &format!("m{i}"), cid, "user", "x", None, "t", false, i).unwrap();
        }
        assert_eq!(next_sort_order(&conn, cid), 5);
    }
}
