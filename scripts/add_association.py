import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src-tauri\src\memory\vector_index.rs'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()

if 'get_associated_memories' in c:
    print('Already exists')
    sys.exit(0)

marker = '#[cfg(test)]'
if marker not in c:
    print('Marker not found')
    sys.exit(1)

lines = []
lines.append('')
lines.append('/// Get associated memories based on vector similarity.')
lines.append('pub fn get_associated_memories(')
lines.append('    conn: &Connection,')
lines.append('    memory_id: &str,')
lines.append('    limit: usize,')
lines.append('    threshold: f32,')
lines.append(') -> Result<Vec<(String, f32)>> {')
lines.append('    let embedding: Vec<u8> = conn.query_row(')
lines.append('        "SELECT embedding FROM memory_vectors WHERE memory_id = ?1",')
lines.append('        params![memory_id],')
lines.append('        |row| row.get(0),')
lines.append('    )?;')
lines.append('    let query_embedding = bytes_to_f32(&embedding);')
lines.append('    let mut stmt = conn.prepare(')
lines.append('        "SELECT memory_id, embedding, dimension FROM memory_vectors WHERE memory_id != ?1"')
lines.append('    )?;')
lines.append('    let mut iter = stmt.query(params![memory_id])?;')
lines.append('    let mut scored: Vec<(String, f32)> = Vec::new();')
lines.append('    while let Some(row) = iter.next()? {')
lines.append('        let id: String = row.get(0)?;')
lines.append('        let bytes: Vec<u8> = row.get(1)?;')
lines.append('        let dim: i32 = row.get(2)?;')
lines.append('        if dim as usize != query_embedding.len() { continue; }')
lines.append('        let emb = bytes_to_f32(&bytes);')
lines.append('        let sim = super::embedding::cosine_similarity(&query_embedding, &emb);')
lines.append('        if sim >= threshold { scored.push((id, sim)); }')
lines.append('    }')
lines.append('    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));')
lines.append('    scored.truncate(limit);')
lines.append('    Ok(scored)')
lines.append('}')
lines.append('')

new_fn = chr(10).join(lines)
c = c.replace(marker, new_fn + marker)

with open(path, 'w', encoding='utf-8') as f:
    f.write(c)
print('Added get_associated_memories')
