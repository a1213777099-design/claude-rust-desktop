import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src-tauri\src\bridge\memory_handlers_v2.rs'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()
if 'memories_cluster' in c:
    print('Already exists')
else:
    fn_code = '''
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
'''
    c = c.rstrip() + fn_code
    with open(path, 'w', encoding='utf-8') as f:
        f.write(c)
    print('Added clustering endpoint')
