import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src-tauri\src\bridge\memory_handlers_v2.rs'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()

if 'memories_associations' in c:
    print('Already exists')
    sys.exit(0)

lines = []
lines.append('')
lines.append('pub async fn memories_associations(')
lines.append('    State(state): State<AppState>,')
lines.append('    Path(id): Path<String>,')
lines.append(') -> Json<serde_json::Value> {')
lines.append('    let db = state.db_manager.clone();')
lines.append('    let result = tokio::task::spawn_blocking(move || {')
lines.append('        db.with_conn(|conn| {')
lines.append('            crate::memory::vector_index::get_associated_memories(conn, &id, 5, 0.5)')
lines.append('        })')
lines.append('    }).await;')
lines.append('    match result {')
lines.append('        Ok(Ok(Ok(associations))) => {')
lines.append('            let mut items: Vec<serde_json::Value> = Vec::new();')
lines.append('            for (id, score) in associations {')
lines.append('                items.push(serde_json::json!({"memory_id": id, "score": score}));')
lines.append('            }')
lines.append('            Json(serde_json::json!({"associations": items}))')
lines.append('        }')
lines.append('        _ => Json(serde_json::json!({"associations": []})),')
lines.append('    }')
lines.append('}')
lines.append('')

new_fn = chr(10).join(lines)
c = c.rstrip() + new_fn
with open(path, 'w', encoding='utf-8') as f:
    f.write(c)
print('Added memories_associations')
