use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmSessionRow {
    pub id: String,
    pub title: String,
    pub workspace: Option<String>,
    pub status: String,
    pub agent_status: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmMessageRow {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub agent_name: Option<String>,
    pub agent_icon: Option<String>,
    pub agent_color: Option<String>,
    #[serde(rename = "type")]
    pub msg_type: Option<String>,
    pub created_at: i64,
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SwarmSessionRow> {
    Ok(SwarmSessionRow {
        id: row.get(0)?,
        title: row.get(1)?,
        workspace: row.get(2)?,
        status: row.get(3)?,
        agent_status: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<SwarmMessageRow> {
    Ok(SwarmMessageRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        agent_name: row.get(4)?,
        agent_icon: row.get(5)?,
        agent_color: row.get(6)?,
        msg_type: row.get(7)?,
        created_at: row.get(8)?,
    })
}

pub fn create_session(
    conn: &Connection,
    id: &str,
    title: &str,
    workspace: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare_cached(
        "INSERT INTO swarm_sessions (id, title, workspace, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'running', ?4, ?4)"
    )?;
    stmt.execute(params![id, title, workspace, now])?;
    Ok(())
}

pub fn list_sessions(conn: &Connection) -> Result<Vec<SwarmSessionRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, title, workspace, status, agent_status, created_at, updated_at FROM swarm_sessions ORDER BY updated_at DESC LIMIT 50"
    )?;
    let rows = stmt.query_map([], |row| row_to_session(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_session(conn: &Connection, id: &str) -> Result<Option<SwarmSessionRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, title, workspace, status, agent_status, created_at, updated_at FROM swarm_sessions WHERE id = ?1"
    )?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(Some(row_to_session(row)?)),
        None => Ok(None),
    }
}

pub fn update_session_status(
    conn: &Connection,
    id: &str,
    status: &str,
    agent_status: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare_cached(
        "UPDATE swarm_sessions SET status = ?1, agent_status = ?2, updated_at = ?3 WHERE id = ?4"
    )?;
    stmt.execute(params![status, agent_status, now, id])?;
    Ok(())
}

pub fn update_session_title(conn: &Connection, id: &str, title: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare_cached(
        "UPDATE swarm_sessions SET title = ?1, updated_at = ?2 WHERE id = ?3"
    )?;
    stmt.execute(params![title, now, id])?;
    Ok(())
}

pub fn delete_session(conn: &Connection, id: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached("DELETE FROM swarm_sessions WHERE id = ?1")?;
    stmt.execute(params![id])?;
    Ok(())
}

pub fn insert_message(
    conn: &Connection,
    id: &str,
    session_id: &str,
    role: &str,
    content: &str,
    agent_name: Option<&str>,
    agent_icon: Option<&str>,
    agent_color: Option<&str>,
    msg_type: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare_cached(
        "INSERT INTO swarm_messages (id, session_id, role, content, agent_name, agent_icon, agent_color, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )?;
    stmt.execute(params![id, session_id, role, content, agent_name, agent_icon, agent_color, msg_type, now])?;
    Ok(())
}

pub fn get_messages(conn: &Connection, session_id: &str) -> Result<Vec<SwarmMessageRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, session_id, role, content, agent_name, agent_icon, agent_color, type, created_at FROM swarm_messages WHERE session_id = ?1 ORDER BY created_at ASC"
    )?;
    let rows = stmt.query_map(params![session_id], |row| row_to_message(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn count_messages(conn: &Connection, session_id: &str) -> Result<i64> {
    let mut stmt = conn.prepare_cached(
        "SELECT COUNT(*) FROM swarm_messages WHERE session_id = ?1"
    )?;
    let count: i64 = stmt.query_row(params![session_id], |row| row.get(0))?;
    Ok(count)
}
