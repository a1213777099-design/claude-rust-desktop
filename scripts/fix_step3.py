import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src\components\SwarmCollaboration.tsx'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Insert after line 566 (index 565)
ws = []
ws.append('                {session.workspace && (\n')
ws.append('                  <div className="text-[9px] text-claude-textSecondary/70 truncate mt-0.5 flex items-center gap-1">\n')
ws.append('                    <FolderOpen size={9} className="flex-shrink-0" />\n')
ws.append('                    {session.workspace.split(/[\\\\/]/).pop() || session.workspace}\n')
ws.append('                  </div>\n')
ws.append('                )}\n')

for j in range(len(ws)):
    lines.insert(566 + j, ws[j])

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)
print('done', len(lines))
