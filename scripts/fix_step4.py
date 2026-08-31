import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src\components\SwarmCollaboration.tsx'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()

# Replace the workspace button with a read-only display
old_btn = '''            <button
              onClick={handleSelectWorkspace}
              className="w-full flex items-center gap-1.5 px-2 py-1.5 text-[11px] rounded-md border border-claude-border hover:bg-claude-hover transition-colors text-claude-textSecondary truncate"
              title={workspace || '\u672a\u8bbe\u7f6e\uff08\u5f53\u524d\u76ee\u5f55\uff09'}
            >
              <FolderOpen size={12} className="flex-shrink-0" />
              <span className="truncate">{workspace ? workspace.split(/[\\/]/).pop() || workspace : '\u9009\u62e9\u5de5\u4f5c\u533a...'}</span>
            </button>'''

new_div = '''            <div
              className="w-full flex items-center gap-1.5 px-2 py-1.5 text-[11px] rounded-md border border-claude-border text-claude-textSecondary truncate"
              title={workspace || '\u672a\u8bbe\u7f6e'}
            >
              <FolderOpen size={12} className="flex-shrink-0" />
              <span className="truncate">{workspace ? workspace.split(/[\\\\/]/).pop() || workspace : '\u672a\u8bbe\u7f6e'}</span>
            </div>'''

if old_btn in c:
    c = c.replace(old_btn, new_div)
    print('Step 4: left panel workspace made read-only')
else:
    print('Step 4: pattern not found, trying alt')
    # Try simpler match
    idx = c.find('onClick={handleSelectWorkspace}')
    if idx >= 0:
        print('  Found onClick at', idx)
        print('  Context:', repr(c[idx-30:idx+100]))

with open(path, 'w', encoding='utf-8') as f:
    f.write(c)
