import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src-tauri\src\bridge\mod.rs'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()
if '/api/memories/{id}/associations' in c:
    print('Route already exists')
else:
    old = '.route("/api/memories/vector-stats", get(memory_handlers_v2::memories_vector_stats))'
    if old in c:
        new = old + chr(10) + '            .route("/api/memories/{id}/associations", get(memory_handlers_v2::memories_associations))'
        c = c.replace(old, new)
        with open(path, 'w', encoding='utf-8') as f:
            f.write(c)
        print('Route added')
    else:
        print('Marker not found')
