import React, { useState, useEffect } from 'react';
import { Search, Trash2, Brain, Filter, RefreshCw } from 'lucide-react';
import { getMemories, searchMemories, deleteMemory, getMemoryStats } from '../api';

const TYPE_COLORS: Record<string, string> = {
  fact: 'bg-blue-500/20 text-blue-400',
  preference: 'bg-purple-500/20 text-purple-400',
  decision: 'bg-green-500/20 text-green-400',
  context: 'bg-gray-500/20 text-gray-400',
};

const TYPE_LABELS: Record<string, string> = {
  fact: 'Fact',
  preference: 'Preference',
  decision: 'Decision',
  context: 'Context',
};

const IMP_STARS = (n: number) => '\u2605'.repeat(n) + '\u2606'.repeat(5 - n);

export default function MemoryPanel() {
  const [memories, setMemories] = useState<any[]>([]);
  const [stats, setStats] = useState<any>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [filterType, setFilterType] = useState<string>('all');
  const [loading, setLoading] = useState(true);

  const loadMemories = async () => {
    setLoading(true);
    try {
      const [mems, st] = await Promise.all([getMemories(), getMemoryStats()]);
      setMemories(mems);
      setStats(st);
    } catch (e) {
      console.error('Failed to load memories', e);
    }
    setLoading(false);
  };

  useEffect(() => { loadMemories(); }, []);

  const handleSearch = async () => {
    if (!searchQuery.trim()) {
      loadMemories();
      return;
    }
    setLoading(true);
    try {
      const results = await searchMemories(searchQuery);
      setMemories(results);
    } catch (e) {
      console.error('Search failed', e);
    }
    setLoading(false);
  };

  const handleDelete = async (id: string) => {
    if (!confirm('Delete this memory?')) return;
    const ok = await deleteMemory(id);
    if (ok) setMemories(prev => prev.filter(m => m.id !== id));
  };

  const filtered = filterType === 'all'
    ? memories
    : memories.filter(m => m.memory_type === filterType);

  return (
    <div className="space-y-6">
      {/* Stats bar */}
      {stats && (
        <div className="flex gap-4 text-[13px] text-claude-textSecondary">
          <span className="font-medium text-claude-text">{stats.total} memories</span>
          {stats.by_type?.map(([t, c]: [string, number]) => (
            <span key={t} className="flex items-center gap-1">
              <span className={`inline-block w-2 h-2 rounded-full ${TYPE_COLORS[t]?.split(' ')[0] || 'bg-gray-500'}`} />
              {TYPE_LABELS[t] || t}: {c}
            </span>
          ))}
        </div>
      )}

      {/* Search + filter bar */}
      <div className="flex gap-3 items-center">
        <div className="flex-1 relative">
          <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-claude-textSecondary" />
          <input
            type="text"
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleSearch()}
            placeholder="Search memories..."
            className="w-full pl-10 pr-4 py-2 bg-claude-input border border-claude-border rounded-lg text-[14px] text-claude-text placeholder:text-claude-textSecondary focus:outline-none focus:border-claude-accent"
          />
        </div>
        <select
          value={filterType}
          onChange={e => setFilterType(e.target.value)}
          className="px-3 py-2 bg-claude-input border border-claude-border rounded-lg text-[14px] text-claude-text"
        >
          <option value="all">All types</option>
          <option value="fact">Fact</option>
          <option value="preference">Preference</option>
          <option value="decision">Decision</option>
          <option value="context">Context</option>
        </select>
        <button onClick={loadMemories} className="p-2 rounded-lg hover:bg-claude-hover text-claude-textSecondary" title="Refresh">
          <RefreshCw size={16} />
        </button>
      </div>

      {/* Memory list */}
      {loading ? (
        <div className="text-center text-claude-textSecondary py-12">Loading...</div>
      ) : filtered.length === 0 ? (
        <div className="text-center text-claude-textSecondary py-12">
          <Brain size={32} className="mx-auto mb-3 opacity-40" />
          <p>No memories found</p>
        </div>
      ) : (
        <div className="space-y-2">
          {filtered.map(mem => (
            <div
              key={mem.id}
              className="group p-4 bg-claude-hover/50 rounded-lg border border-claude-border/50 hover:border-claude-border transition-colors"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1.5">
                    <span className={`text-[11px] px-1.5 py-0.5 rounded ${TYPE_COLORS[mem.memory_type] || TYPE_COLORS.context}`}>
                      {TYPE_LABELS[mem.memory_type] || mem.memory_type}
                    </span>
                    <span className="text-[11px] text-yellow-500/80 tracking-wider">{IMP_STARS(mem.importance)}</span>
                    <span className="text-[11px] text-claude-textSecondary">{new Date(mem.created_at).toLocaleDateString()}</span>
                  </div>
                  <p className="text-[13px] text-claude-text leading-relaxed whitespace-pre-wrap break-words">
                    {mem.content.length > 300 ? mem.content.slice(0, 300) + '...' : mem.content}
                  </p>
                  {mem.tags && mem.tags !== 'auto' && (
                    <div className="mt-1.5 flex gap-1 flex-wrap">
                      {mem.tags.split(',').filter(Boolean).map((tag: string) => (
                        <span key={tag} className="text-[10px] px-1.5 py-0.5 bg-claude-hover rounded text-claude-textSecondary">
                          {tag.trim()}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
                <button
                  onClick={() => handleDelete(mem.id)}
                  className="opacity-0 group-hover:opacity-100 p-1.5 rounded hover:bg-red-500/20 text-claude-textSecondary hover:text-red-400 transition-all"
                  title="Delete memory"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
