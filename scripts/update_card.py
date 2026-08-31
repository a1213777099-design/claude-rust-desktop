import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src\components\SwarmCollaboration.tsx'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Card starts at line 569 (0-based: 568), ends at line 597 (0-based: 596)
card_start = 568  # sessions.map line
card_end = 596    # closing div line

# Verify
print(f'Line {card_start+1}: {lines[card_start].rstrip()}')
print(f'Line {card_end+1}: {lines[card_end].rstrip()}')

# Build new card lines
new_card_lines = [
    '           {sessions.map(session => (\n',
    '            <div\n',
    '              key={session.id}\n',
    '              onClick={() => editingSessionId !== session.id && handleSelectSession(session.id)}\n',
    '              className={`group flex items-start gap-2 px-2.5 py-2.5 rounded-lg cursor-pointer transition-all text-[11px] border${\n',
    '                currentSessionId === session.id\n',
    "                  ? ' bg-[#8B5CF6]/10 border-[#8B5CF6]/30'\n",
    "                  : ' hover:bg-claude-hover border-transparent'\n",
    '              }`}\n',
    '            >\n',
    '              <span className="flex-shrink-0 mt-0.5">{getStatusIcon(session.status)}</span>\n',
    '              <div className="flex-1 min-w-0">\n',
    '                {editingSessionId === session.id ? (\n',
    '                  <input\n',
    '                    autoFocus\n',
    '                    value={editingTitle}\n',
    '                    onChange={e => setEditingTitle(e.target.value)}\n',
    '                    onBlur={() => handleRenameSession(session.id, editingTitle)}\n',
    "                    onKeyDown={e => { if (e.key === 'Enter') handleRenameSession(session.id, editingTitle); if (e.key === 'Escape') { setEditingSessionId(null); setEditingTitle(''); } }}\n",
    '                    className="w-full bg-transparent border-b border-[#8B5CF6] text-claude-text text-[11px] font-medium leading-tight focus:outline-none"\n',
    '                    onClick={e => e.stopPropagation()}\n',
    '                  />\n',
    '                ) : (\n',
    "                  <div className=\"font-medium text-claude-text truncate leading-tight\">{session.title || '\\u65B0\\u4EFB\\u52A1'}</div>\n",
    '                )}\n',
    '                {session.workspace && (\n',
    '                  <div className="text-[9px] text-claude-textSecondary/70 truncate mt-0.5 flex items-center gap-1">\n',
    '                    <FolderOpen size={9} className="flex-shrink-0" />\n',
    '                    {session.workspace.split(/[\\\\/]/).pop() || session.workspace}\n',
    '                  </div>\n',
    '                )}\n',
    '                <div className="text-[10px] text-claude-textSecondary mt-0.5">{formatTime(session.created_at)}</div>\n',
    '              </div>\n',
    '              <div className="flex-shrink-0 flex items-center gap-0.5">\n',
    '                <button\n',
    "                  onClick={(e) => { e.stopPropagation(); setEditingSessionId(session.id); setEditingTitle(session.title || ''); }}\n",
    '                  className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-claude-hover transition-all"\n',
    '                  title="\\u91CD\\u547D\\u540D"\n',
    '                >\n',
    '                  <Pencil size={11} className="text-claude-textSecondary" />\n',
    '                </button>\n',
    '                <button\n',
    '                  onClick={(e) => handleDeleteSession(session.id, e)}\n',
    '                  className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-red-500/10 transition-all"\n',
    '                  title="\\u5220\\u9664\\u4F1A\\u8BDD"\n',
    '                >\n',
    '                  <Trash2 size={11} className="text-red-400" />\n',
    '                </button>\n',
    '              </div>\n',
    '            </div>\n',
    '          ))}\n',
]

# Replace
new_lines = lines[:card_start] + new_card_lines + lines[card_end+1:]
with open(path, 'w', encoding='utf-8') as f:
    f.writelines(new_lines)
print('Card replaced with rename support')
