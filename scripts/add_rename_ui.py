import sys
sys.stdout.reconfigure(encoding='utf-8')
path = r'F:\Projects\claude-code-rust\src\components\SwarmCollaboration.tsx'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()

# 1. Add import for swarmRenameSession
old_import = "import { workflowEventStream, WorkflowEvent, swarmCreateSession, swarmListSessions, swarmGetMessages, swarmAddMessage, swarmUpdateStatus, swarmDeleteSession, type SwarmSession, type SwarmMessage } from '../api';"
new_import = "import { workflowEventStream, WorkflowEvent, swarmCreateSession, swarmListSessions, swarmGetMessages, swarmAddMessage, swarmUpdateStatus, swarmDeleteSession, swarmRenameSession, type SwarmSession, type SwarmMessage } from '../api';"
if old_import in c:
    c = c.replace(old_import, new_import)
    print('Import added')

# 2. Add editing state after sessionIdRef
old_ref = "  const sessionIdRef = useRef<string | null>(null);\n  const inputRef = useRef<HTMLInputElement>(null);"
new_ref = "  const sessionIdRef = useRef<string | null>(null);\n  const inputRef = useRef<HTMLInputElement>(null);\n  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);\n  const [editingTitle, setEditingTitle] = useState('');"
if old_ref in c:
    c = c.replace(old_ref, new_ref)
    print('State added')

# 3. Add rename handler after handleDeleteSession
marker = "  const handleNewTask = useCallback(async () => {"
rename_handler = """
  // Rename a session
  const handleRenameSession = useCallback(async (sessionId: string, newTitle: string) => {
    if (!newTitle.trim()) return;
    try {
      await swarmRenameSession(sessionId, newTitle.trim());
      setSessions(prev => prev.map(s => s.id === sessionId ? { ...s, title: newTitle.trim() } : s));
    } catch (e) {
      console.warn('[Swarm] Failed to rename session:', e);
    }
    setEditingSessionId(null);
    setEditingTitle('');
  }, []);

"""
if marker in c and 'handleRenameSession' not in c:
    c = c.replace(marker, rename_handler + marker)
    print('Rename handler added')

# 4. Update session card to show edit button and inline input
# Find the card structure and update it
old_card = """            <div
              key={session.id}
              onClick={() => handleSelectSession(session.id)}
              className={`group flex items-start gap-2 px-2.5 py-2.5 rounded-lg cursor-pointer transition-all text-[11px] border${
                currentSessionId === session.id
                  ? ' bg-[#8B5CF6]/10 border-[#8B5CF6]/30'
                  : ' hover:bg-claude-hover border-transparent'
              }`}
            >
              <span className="flex-shrink-0 mt-0.5">{getStatusIcon(session.status)}</span>
              <div className="flex-1 min-w-0">
                <div className="font-medium text-claude-text truncate leading-tight">{session.title || '新任务'}</div>
                <div className="text-[10px] text-claude-textSecondary mt-0.5">{formatTime(session.created_at)}</div>
              </div>
              <button
                onClick={(e) => handleDeleteSession(session.id, e)}
                className="flex-shrink-0 opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-red-500/10 transition-all"
                title="删除会话"
              >
                <Trash2 size={11} className="text-red-400" />
              </button>
            </div>"""

new_card = """            <div
              key={session.id}
              onClick={() => editingSessionId !== session.id && handleSelectSession(session.id)}
              className={`group flex items-start gap-2 px-2.5 py-2.5 rounded-lg cursor-pointer transition-all text-[11px] border${
                currentSessionId === session.id
                  ? ' bg-[#8B5CF6]/10 border-[#8B5CF6]/30'
                  : ' hover:bg-claude-hover border-transparent'
              }`}
            >
              <span className="flex-shrink-0 mt-0.5">{getStatusIcon(session.status)}</span>
              <div className="flex-1 min-w-0">
                {editingSessionId === session.id ? (
                  <input
                    autoFocus
                    value={editingTitle}
                    onChange={e => setEditingTitle(e.target.value)}
                    onBlur={() => handleRenameSession(session.id, editingTitle)}
                    onKeyDown={e => { if (e.key === 'Enter') handleRenameSession(session.id, editingTitle); if (e.key === 'Escape') { setEditingSessionId(null); setEditingTitle(''); } }}
                    className="w-full bg-transparent border-b border-[#8B5CF6] text-claude-text text-[11px] font-medium leading-tight focus:outline-none"
                    onClick={e => e.stopPropagation()}
                  />
                ) : (
                  <div className="font-medium text-claude-text truncate leading-tight">{session.title || '新任务'}</div>
                )}
                <div className="text-[10px] text-claude-textSecondary mt-0.5">{formatTime(session.created_at)}</div>
              </div>
              <div className="flex-shrink-0 flex items-center gap-0.5">
                <button
                  onClick={(e) => { e.stopPropagation(); setEditingSessionId(session.id); setEditingTitle(session.title || ''); }}
                  className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-claude-hover transition-all"
                  title="重命名"
                >
                  <Pencil size={11} className="text-claude-textSecondary" />
                </button>
                <button
                  onClick={(e) => handleDeleteSession(session.id, e)}
                  className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-red-500/10 transition-all"
                  title="删除会话"
                >
                  <Trash2 size={11} className="text-red-400" />
                </button>
              </div>
            </div>"""

if old_card in c:
    c = c.replace(old_card, new_card)
    print('Card UI updated')
else:
    print('Card pattern not found, trying to find it...')
    if 'handleDeleteSession(session.id, e)' in c:
        print('  Found delete button')

# 5. Add Pencil to lucide imports
old_lucide = "import {\n  Brain, Users, Zap, Loader2, CheckCircle2, XCircle, Clock,\n  ArrowRight, GitBranch, Activity, Target, Sparkles, Send, FolderOpen,\n  Bot, Square, RotateCcw, ChevronRight, Plus, Trash2, MessageSquare,\n} from 'lucide-react';"
new_lucide = "import {\n  Brain, Users, Zap, Loader2, CheckCircle2, XCircle, Clock,\n  ArrowRight, GitBranch, Activity, Target, Sparkles, Send, FolderOpen,\n  Bot, Square, RotateCcw, ChevronRight, Plus, Trash2, MessageSquare, Pencil,\n} from 'lucide-react';"
if old_lucide in c:
    c = c.replace(old_lucide, new_lucide)
    print('Pencil icon added')

with open(path, 'w', encoding='utf-8') as f:
    f.write(c)
print('Frontend done')
