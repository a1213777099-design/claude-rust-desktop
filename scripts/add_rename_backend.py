import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src-tauri\src\bridge\mod.rs'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()
lines = []
lines.append('async fn swarm_session_rename(')
lines.append('    State(state): State<AppState>,')
lines.append('    Path(id): Path<String>,')
lines.append('    Json(body): Json<serde_json::Value>,')
lines.append(') -> Json<serde_json::Value> {')
lines.append('    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();')
lines.append('    let db = state.db_manager.clone();')
lines.append('    let result = tokio::task::spawn_blocking(move || {')
lines.append('        db.with_conn(|conn| crate::db::swarm_repo::update_session_title(conn, &id, &title))')
lines.append('    }).await;')
lines.append('    match result {')
lines.append('        Ok(Ok(Ok(_))) => Json(serde_json::json!({ "ok": true })),')
lines.append('        _ => Json(serde_json::json!({ "error": "Failed to rename" })),')
lines.append('    }')
lines.append('}')
lines.append('')
handler = '\n'.join(lines)
marker = '/// MetaGPT workflow endpoint'
if marker in c and 'swarm_session_rename' not in c:
    idx = c.find(marker)
    c = c[:idx] + handler + c[idx:]
    print('Handler added')
old_r = '.route("/api/swarm/sessions/{id}/status", post(swarm_status_update))'
if old_r in c and 'sessions/{id}/title' not in c:
    new_r = old_r + '\n            .route("/api/swarm/sessions/{id}/title", post(swarm_session_rename))'
    c = c.replace(old_r, new_r)
    print('Route added')
with open(path, 'w', encoding='utf-8') as f:
    f.write(c)
print('Done')
