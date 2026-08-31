/// MetaGPT memory persistence: saves workflow outputs to persistent storage.
/// Enables cross-workflow knowledge retention.
use anyhow::Result;
use crate::db::memory_repo;
use crate::memory::embedding::EmbeddingEngine;
use rusqlite::Connection;

/// Save a workflow role's output to persistent memory.
pub async fn save_role_output(
    conn: &Connection,
    workspace: &str,
    role_name: &str,
    cause_by: &str,
    output: &str,
    embedding_engine: Option<&EmbeddingEngine>,
) -> Result<()> {
    if output.is_empty() || output.len() < 50 {
        return Ok(()); // Skip trivial outputs
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Determine memory type and importance based on role
    let (memory_type, importance) = match role_name {
        "ProductManager" => ("prd", 5),
        "Architect" => ("design", 5),
        "Engineer" => ("code", 4),
        "Reviewer" => ("review", 4),
        "QaEngineer" => ("test", 3),
        "DevOps" => ("deployment", 3),
        "ProjectManager" => ("summary", 4),
        _ => ("workflow", 3),
    };

    // Truncate summary for storage (keep first 2000 chars)
    let summary: String = output.chars().take(2000).collect();
    let tags = format!("metagpt,{},{}", role_name.to_lowercase(), cause_by);

    // Generate embedding if engine is available
    let embedding: Option<Vec<f32>> = if let Some(engine) = embedding_engine {
        match engine.embed(&summary).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                tracing::warn!(target: "metagpt::persistence", "Failed to generate embedding: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Save to database
    if let Some(ref emb) = embedding {
        memory_repo::insert_memory_with_vector(
            conn, &id, workspace, "metagpt_workflow",
            &summary, &tags, memory_type, importance, &now, emb.as_slice(),
        )?;
    } else {
        memory_repo::insert_memory(
            conn, &id, workspace, "metagpt_workflow",
            &summary, &tags, memory_type, importance, &now,
        )?;
    }

    tracing::info!(target: "metagpt::persistence", "Saved {} output ({} chars) as memory {}", role_name, output.len(), id);
    Ok(())
}

/// Save the entire workflow result (all role outputs) to persistent memory.
pub async fn save_workflow_result(
    conn: &Connection,
    workspace: &str,
    role_outputs: &[(String, String, String)], // (role_name, cause_by, output)
    embedding_engine: Option<&EmbeddingEngine>,
) -> Result<()> {
    for (role_name, cause_by, output) in role_outputs {
        save_role_output(conn, workspace, role_name, cause_by, output, embedding_engine).await?;
    }
    Ok(())
}

/// Load relevant past workflow memories for context injection.
pub fn load_relevant_memories(
    conn: &Connection,
    workspace: &str,
    query: &str,
    query_embedding: Option<&[f32]>,
    limit: i64,
) -> Result<Vec<memory_repo::MemoryRow>> {
    memory_repo::search_memories_hybrid(conn, workspace, query, query_embedding, limit)
}

/// Load recent workflow memories for a workspace.
pub fn load_recent_workflow_memories(
    conn: &Connection,
    workspace: &str,
    limit: i64,
) -> Result<Vec<memory_repo::MemoryRow>> {
    memory_repo::list_recent_memories(conn, workspace, limit)
}
