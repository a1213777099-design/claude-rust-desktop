/// Vector index for semantic search over memories.
use anyhow::Result;
use rusqlite::{params, Connection};

pub const VECTOR_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS memory_vectors (
    memory_id TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL,
    dimension INTEGER NOT NULL DEFAULT 384,
    model TEXT NOT NULL DEFAULT 'tfidf',
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_vectors_created ON memory_vectors(created_at);
"#;

pub fn upsert_vector(conn: &Connection, memory_id: &str, embedding: &[f32], model: &str, created_at: &str) -> Result<()> {
    let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
    let dimension = embedding.len() as i32;
    let mut stmt = conn.prepare_cached(
        "INSERT OR REPLACE INTO memory_vectors (memory_id, embedding, dimension, model, created_at) VALUES (?1, ?2, ?3, ?4, ?5)"
    )?;
    stmt.execute(params![memory_id, bytes, dimension, model, created_at])?;
    Ok(())
}

pub fn delete_vector(conn: &Connection, memory_id: &str) -> Result<bool> {
    let changed = conn.execute("DELETE FROM memory_vectors WHERE memory_id = ?1", params![memory_id])?;
    Ok(changed > 0)
}

pub fn search_vectors(conn: &Connection, query_embedding: &[f32], workspace_path: Option<&str>, limit: usize, threshold: f32) -> Result<Vec<(String, f32)>> {
    let query_dim = query_embedding.len();
    let rows: Vec<(String, Vec<u8>, i32)> = if let Some(ws) = workspace_path {
        let mut stmt = conn.prepare(
            "SELECT mv.memory_id, mv.embedding, mv.dimension FROM memory_vectors mv \
             INNER JOIN memories m ON mv.memory_id = m.id WHERE m.workspace_path = ?1"
        )?;
        let mut iter = stmt.query(params![ws])?;
        let mut r = Vec::new();
        while let Some(row) = iter.next()? { r.push((row.get(0)?, row.get(1)?, row.get(2)?)); }
        r
    } else {
        let mut stmt = conn.prepare("SELECT memory_id, embedding, dimension FROM memory_vectors")?;
        let mut iter = stmt.query([])?;
        let mut r = Vec::new();
        while let Some(row) = iter.next()? { r.push((row.get(0)?, row.get(1)?, row.get(2)?)); }
        r
    };

    let mut scored: Vec<(String, f32)> = rows.iter()
        .filter(|(_, _, dim)| *dim as usize == query_dim)
        .filter_map(|(id, bytes, _)| {
            let emb = bytes_to_f32(bytes);
            let sim = super::embedding::cosine_similarity(query_embedding, &emb);
            if sim >= threshold { Some((id.clone(), sim)) } else { None }
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

pub fn count_vectors(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM memory_vectors", [], |row| row.get(0))?)
}

fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}


/// Get associated memories based on vector similarity.
pub fn get_associated_memories(
    conn: &Connection,
    memory_id: &str,
    limit: usize,
    threshold: f32,
) -> Result<Vec<(String, f32)>> {
    let embedding: Vec<u8> = conn.query_row(
        "SELECT embedding FROM memory_vectors WHERE memory_id = ?1",
        params![memory_id],
        |row| row.get(0),
    )?;
    let query_embedding = bytes_to_f32(&embedding);
    let mut stmt = conn.prepare(
        "SELECT memory_id, embedding, dimension FROM memory_vectors WHERE memory_id != ?1"
    )?;
    let mut iter = stmt.query(params![memory_id])?;
    let mut scored: Vec<(String, f32)> = Vec::new();
    while let Some(row) = iter.next()? {
        let id: String = row.get(0)?;
        let bytes: Vec<u8> = row.get(1)?;
        let dim: i32 = row.get(2)?;
        if dim as usize != query_embedding.len() { continue; }
        let emb = bytes_to_f32(&bytes);
        let sim = super::embedding::cosine_similarity(&query_embedding, &emb);
        if sim >= threshold { scored.push((id, sim)); }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_bytes_roundtrip() {
        let orig: Vec<f32> = vec![0.1, 0.2, -0.3, 1.0];
        let bytes: Vec<u8> = orig.iter().flat_map(|f| f.to_le_bytes()).collect();
        let restored = bytes_to_f32(&bytes);
        for (a, b) in orig.iter().zip(restored.iter()) { assert!((a - b).abs() < 0.0001); }
    }
}


/// Retrieve the stored embedding for a given memory id.
pub fn get_embedding(conn: &Connection, memory_id: &str) -> Result<Vec<f32>> {
    let bytes: Vec<u8> = conn.query_row(
        "SELECT embedding FROM memory_vectors WHERE memory_id = ?1",
        params![memory_id],
        |row| row.get(0),
    )?;
    Ok(bytes_to_f32(&bytes))
}