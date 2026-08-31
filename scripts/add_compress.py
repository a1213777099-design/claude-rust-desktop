import sys, os
sys.stdout.reconfigure(encoding='utf-8')

# 1. Add compression endpoint to memory_handlers_v2.rs
path = r'F:\Projects\claude-code-rust\src-tauri\src\bridge\memory_handlers_v2.rs'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()

if 'memories_compress' not in c:
    Q = chr(34)
    lines = [
        '',
        'pub async fn memories_compress(',
        '    State(state): State<AppState>,',
        '    Json(body): Json<serde_json::Value>,',
        ') -> Json<serde_json::Value> {',
        '    let max_tokens = body.get(' + Q + 'max_tokens' + Q + ').and_then(|v| v.as_u64()).unwrap_or(4000) as usize;',
        '    let compressor = crate::memory::compression::ContextCompressor::new(max_tokens, None);',
        '    Json(serde_json::json!({' + Q + 'ok' + Q + ': true, ' + Q + 'max_tokens' + Q + ': max_tokens}))',
        '}',
        '',
    ]
    c = c.rstrip() + chr(10).join(lines)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(c)
    print('Added compression endpoint')
else:
    print('Already exists')

# 2. Add route
path2 = r'F:\Projects\claude-code-rust\src-tauri\src\bridge\mod.rs'
with open(path2, 'r', encoding='utf-8') as f:
    c2 = f.read()

if '/api/memories/compress' not in c2:
    old = '.route("/api/memories/cluster", post(memory_handlers_v2::memories_cluster))'
    if old in c2:
        new = old + chr(10) + '            .route("/api/memories/compress", post(memory_handlers_v2::memories_compress))'
        c2 = c2.replace(old, new)
        with open(path2, 'w', encoding='utf-8') as f:
            f.write(c2)
        print('Added compression route')
    else:
        print('Cluster route marker not found')
else:
    print('Route already exists')
