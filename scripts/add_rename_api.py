import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src\api.ts'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()
new_fn = '''
export async function swarmRenameSession(id: string, title: string): Promise<void> {
  const res = await request('/swarm/sessions/' + id + '/title', {
    method: 'POST',
    body: JSON.stringify({ title }),
  });
  await res.json().catch(() => {});
}
'''
marker = '// === Swarm Session Persistence API ==='
if marker in c and 'swarmRenameSession' not in c:
    # Find the end of the swarm section
    idx = c.find(marker)
    # Insert after the last swarm function
    last_swarm = c.rfind('swarmUpdateStatus', idx)
    if last_swarm >= 0:
        # Find the end of that function (next closing brace + newline)
        end = c.find('\n}', last_swarm)
        if end >= 0:
            insert_at = end + 2
            c = c[:insert_at] + new_fn + c[insert_at:]
            print('swarmRenameSession added')
with open(path, 'w', encoding='utf-8') as f:
    f.write(c)
print('Frontend API done')
