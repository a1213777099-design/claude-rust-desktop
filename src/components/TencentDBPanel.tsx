import React, { useState, useEffect, useCallback } from 'react';
import { CheckCircle2, XCircle, RefreshCw, Save, Database, Activity, Search, Plus, ArrowUp, BarChart3 } from 'lucide-react';
import {
  getTdaiConfig,
  setTdaiConfig,
  verifyTdaiAuth,
  getTdaiHealth,
  searchTdaiMemories,
  addTdaiMemory,
  promoteTdaiMemory,
  getTdaiStats,
  TencentDBConfig,
  TdaiHealthInfo,
  TdaiSearchResponse,
} from '../api';

type SaveState = '' | 'saving' | 'saved' | 'error';

const DEFAULT_CONFIG: TencentDBConfig = {
  base_url: 'http://localhost:8420',
  user_key: '',
  team_id: 'default-team',
  agent_id: 'claude-code-rust',
  user_id: 'local-user',
  space_id: 'default',
  enabled: false,
};

const TencentDBPanel: React.FC = () => {
  const [cfg, setCfg] = useState<TencentDBConfig>(DEFAULT_CONFIG);
  const [loaded, setLoaded] = useState(false);
  const [saveState, setSaveState] = useState<SaveState>('');
  const [saveMsg, setSaveMsg] = useState('');
  const [health, setHealth] = useState<TdaiHealthInfo | null>(null);
  const [healthLoading, setHealthLoading] = useState(false);
  const [auth, setAuth] = useState<{ valid: boolean; user_id?: string; team_id?: string; agent_id?: string; error?: string } | null>(null);
  const [authLoading, setAuthLoading] = useState(false);
  const [searchQ, setSearchQ] = useState('');
  const [searchRes, setSearchRes] = useState<TdaiSearchResponse | null>(null);
  const [searching, setSearching] = useState(false);
  const [addContent, setAddContent] = useState('');
  const [adding, setAdding] = useState(false);
  const [stats, setStats] = useState<Record<string, number> | null>(null);

  const load = useCallback(async () => {
    try {
      const c = await getTdaiConfig();
      setCfg({ ...DEFAULT_CONFIG, ...c });
    } catch (e) {
      console.error('[TencentDB] load config failed', e);
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const save = async () => {
    setSaveState('saving');
    setSaveMsg('');
    try {
      const r = await setTdaiConfig(cfg);
      setCfg({ ...DEFAULT_CONFIG, ...r.config });
      setSaveState('saved');
      setSaveMsg('已保存');
      setTimeout(() => setSaveState(''), 1500);
    } catch (e: any) {
      setSaveState('error');
      setSaveMsg(String(e?.message || e));
    }
  };

  const refreshHealth = async () => {
    setHealthLoading(true);
    try {
      const h = await getTdaiHealth();
      setHealth(h);
    } catch (e: any) {
      setHealth({ reachable: false, base_url: cfg.base_url, error: String(e?.message || e) });
    } finally {
      setHealthLoading(false);
    }
  };

  const verifyAuth = async () => {
    setAuthLoading(true);
    try {
      const a = await verifyTdaiAuth();
      setAuth(a);
    } catch (e: any) {
      setAuth({ valid: false, error: String(e?.message || e) });
    } finally {
      setAuthLoading(false);
    }
  };

  const doSearch = async () => {
    if (!searchQ.trim()) return;
    setSearching(true);
    try {
      const r = await searchTdaiMemories(searchQ, '', 10);
      setSearchRes(r);
    } catch (e: any) {
      console.error('[TencentDB] search failed', e);
      setSearchRes({ hits: [], source: 'error' });
    } finally {
      setSearching(false);
    }
  };

  const doAdd = async () => {
    if (!addContent.trim()) return;
    setAdding(true);
    try {
      const r = await addTdaiMemory('', addContent.trim(), 3, '');
      if (r.ok) {
        setAddContent('');
        await refreshStats();
      } else {
        alert('添加失败: ' + (r.error || 'unknown'));
      }
    } finally {
      setAdding(false);
    }
  };

  const doPromote = async (id: string) => {
    const r = await promoteTdaiMemory(id);
    if (r.ok) {
      await doSearch();
    } else {
      alert('提升失败: ' + (r.error || 'unknown'));
    }
  };

  const refreshStats = async () => {
    try {
      const r = await getTdaiStats();
      if (r.ok) setStats(r.stats);
    } catch (e) {
      console.error('[TencentDB] stats failed', e);
    }
  };

  useEffect(() => { if (loaded) refreshStats(); }, [loaded]);

  if (!loaded) {
    return <div className="text-claude-textSecondary">Loading TencentDB config…</div>;
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-claude-text flex items-center gap-2">
          <Database size={20} /> TencentDB Agent Memory
        </h2>
        <p className="text-sm text-claude-textSecondary mt-1">
          这是当前项目的主记忆系统。每次对话都会自动从 TencentDB 检索相关记忆并注入 prompt。配置错误时自动降级到本地 tiered store。
        </p>
      </div>

      {/* Config Form */}
      <div className="bg-claude-surface rounded-xl border border-claude-border p-5 space-y-4">
        <h3 className="text-base font-semibold text-claude-text">连接配置</h3>
        <div className="grid grid-cols-2 gap-4">
          <Field label="Base URL" hint="TencentDB Memory Core 地址，例如 http://localhost:8420">
            <input
              className="w-full bg-claude-inputBg border border-claude-border rounded px-3 py-2 text-sm"
              value={cfg.base_url}
              onChange={(e) => setCfg({ ...cfg, base_url: e.target.value })}
              placeholder="http://localhost:8420"
            />
          </Field>
          <Field label="User Key (x-tdai-user-key)" hint="从 Memory Hub 申请的密钥">
            <input
              className="w-full bg-claude-inputBg border border-claude-border rounded px-3 py-2 text-sm"
              value={cfg.user_key}
              onChange={(e) => setCfg({ ...cfg, user_key: e.target.value })}
              type="password"
              placeholder="tdai-xxx…"
            />
          </Field>
          <Field label="Team ID">
            <input className="w-full bg-claude-inputBg border border-claude-border rounded px-3 py-2 text-sm" value={cfg.team_id} onChange={(e) => setCfg({ ...cfg, team_id: e.target.value })} />
          </Field>
          <Field label="Agent ID">
            <input className="w-full bg-claude-inputBg border border-claude-border rounded px-3 py-2 text-sm" value={cfg.agent_id} onChange={(e) => setCfg({ ...cfg, agent_id: e.target.value })} />
          </Field>
          <Field label="User ID">
            <input className="w-full bg-claude-inputBg border border-claude-border rounded px-3 py-2 text-sm" value={cfg.user_id} onChange={(e) => setCfg({ ...cfg, user_id: e.target.value })} />
          </Field>
          <Field label="Space ID">
            <input className="w-full bg-claude-inputBg border border-claude-border rounded px-3 py-2 text-sm" value={cfg.space_id} onChange={(e) => setCfg({ ...cfg, space_id: e.target.value })} />
          </Field>
        </div>

        <label className="flex items-center gap-2 text-sm text-claude-text">
          <input
            type="checkbox"
            checked={cfg.enabled}
            onChange={(e) => setCfg({ ...cfg, enabled: e.target.checked })}
            className="rounded"
          />
          启用 TencentDB 远程记忆（取消勾选则只使用本地 tiered store）
        </label>

        <div className="flex items-center gap-3 pt-2">
          <button
            onClick={save}
            disabled={saveState === 'saving'}
            className="bg-blue-600 hover:bg-blue-700 text-white px-4 py-2 rounded-lg text-sm font-medium flex items-center gap-2 disabled:opacity-50"
          >
            <Save size={14} />
            {saveState === 'saving' ? '保存中…' : '保存配置'}
          </button>
          {saveState === 'saved' && <span className="text-green-500 text-sm flex items-center gap-1"><CheckCircle2 size={14} />{saveMsg}</span>}
          {saveState === 'error' && <span className="text-red-500 text-sm flex items-center gap-1"><XCircle size={14} />{saveMsg}</span>}
        </div>
      </div>

      {/* Health & Auth */}
      <div className="grid grid-cols-2 gap-4">
        <Card title="健康状态" icon={<Activity size={16} />}>
          <div className="space-y-2">
            {health ? (
              <>
                <Row label="可达" value={health.reachable ? '是' : '否'} ok={health.reachable} />
                <Row label="Base URL" value={health.base_url} />
                {health.latency_ms != null && <Row label="延迟" value={`${health.latency_ms} ms`} />}
                {health.error && <div className="text-xs text-red-400 font-mono break-all">{health.error}</div>}
              </>
            ) : (
              <div className="text-claude-textSecondary text-sm">点击下方按钮探测</div>
            )}
            <button onClick={refreshHealth} disabled={healthLoading} className="bg-claude-btn-hover hover:bg-claude-hover text-claude-text px-3 py-1.5 rounded text-sm flex items-center gap-2">
              <RefreshCw size={14} className={healthLoading ? 'animate-spin' : ''} /> 探测
            </button>
          </div>
        </Card>

        <Card title="身份认证" icon={<CheckCircle2 size={16} />}>
          <div className="space-y-2">
            {auth ? (
              <>
                <Row label="有效" value={auth.valid ? '是' : '否'} ok={auth.valid} />
                {auth.user_id && <Row label="User" value={auth.user_id} />}
                {auth.team_id && <Row label="Team" value={auth.team_id} />}
                {auth.agent_id && <Row label="Agent" value={auth.agent_id} />}
                {auth.error && <div className="text-xs text-red-400 font-mono break-all">{auth.error}</div>}
              </>
            ) : (
              <div className="text-claude-textSecondary text-sm">先保存配置再验证</div>
            )}
            <button onClick={verifyAuth} disabled={authLoading} className="bg-claude-btn-hover hover:bg-claude-hover text-claude-text px-3 py-1.5 rounded text-sm flex items-center gap-2">
              <RefreshCw size={14} className={authLoading ? 'animate-spin' : ''} /> 验证
            </button>
          </div>
        </Card>
      </div>

      {/* Search & Add */}
      <div className="grid grid-cols-2 gap-4">
        <Card title="检索记忆" icon={<Search size={16} />}>
          <div className="space-y-2">
            <div className="flex gap-2">
              <input
                className="flex-1 bg-claude-inputBg border border-claude-border rounded px-3 py-1.5 text-sm"
                value={searchQ}
                onChange={(e) => setSearchQ(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && doSearch()}
                placeholder="输入检索词…"
              />
              <button onClick={doSearch} disabled={searching} className="bg-blue-600 hover:bg-blue-700 text-white px-3 py-1.5 rounded text-sm flex items-center gap-1">
                <Search size={12} /> 检索
              </button>
            </div>
            {searchRes && (
              <div className="text-xs text-claude-textSecondary">
                来源: {searchRes.source} · {searchRes.hits.length} 条结果
              </div>
            )}
            <div className="space-y-2 max-h-72 overflow-y-auto">
              {searchRes?.hits.map((h) => (
                <div key={h.id} className="border border-claude-border rounded p-2 text-sm bg-claude-inputBg">
                  <div className="flex items-center justify-between gap-2 mb-1">
                    <span className="text-xs text-claude-textSecondary">L{h.tier} · imp {h.importance} · score {h.score.toFixed(3)}</span>
                    <button onClick={() => doPromote(h.id)} className="text-xs text-blue-400 hover:text-blue-300 flex items-center gap-1">
                      <ArrowUp size={10} /> 提升
                    </button>
                  </div>
                  <div className="text-claude-text text-xs whitespace-pre-wrap break-words">{h.content}</div>
                </div>
              ))}
            </div>
          </div>
        </Card>

        <Card title="添加记忆" icon={<Plus size={16} />}>
          <div className="space-y-2">
            <textarea
              className="w-full bg-claude-inputBg border border-claude-border rounded px-3 py-2 text-sm h-32 resize-none"
              value={addContent}
              onChange={(e) => setAddContent(e.target.value)}
              placeholder="输入记忆内容…"
            />
            <button onClick={doAdd} disabled={adding || !addContent.trim()} className="bg-blue-600 hover:bg-blue-700 text-white px-3 py-1.5 rounded text-sm flex items-center gap-1 disabled:opacity-50">
              <Plus size={12} /> {adding ? '添加中…' : '添加'}
            </button>
          </div>
        </Card>
      </div>

      {/* Stats */}
      <Card title="统计" icon={<BarChart3 size={16} />}>
        {stats ? (
          <div className="grid grid-cols-4 gap-3">
            {Object.entries(stats).map(([k, v]) => (
              <div key={k} className="bg-claude-inputBg rounded p-3 text-center">
                <div className="text-2xl font-semibold text-claude-text">{v}</div>
                <div className="text-xs text-claude-textSecondary mt-1">{k}</div>
              </div>
            ))}
          </div>
        ) : (
          <div className="text-claude-textSecondary text-sm">暂无统计数据</div>
        )}
        <button onClick={refreshStats} className="mt-3 bg-claude-btn-hover hover:bg-claude-hover text-claude-text px-3 py-1.5 rounded text-sm flex items-center gap-2">
          <RefreshCw size={14} /> 刷新
        </button>
      </Card>
    </div>
  );
};

const Field: React.FC<{ label: string; hint?: string; children: React.ReactNode }> = ({ label, hint, children }) => (
  <div>
    <label className="block text-sm font-medium text-claude-text mb-1">{label}</label>
    {children}
    {hint && <div className="text-xs text-claude-textSecondary mt-1">{hint}</div>}
  </div>
);

const Card: React.FC<{ title: string; icon: React.ReactNode; children: React.ReactNode }> = ({ title, icon, children }) => (
  <div className="bg-claude-surface rounded-xl border border-claude-border p-5 space-y-3">
    <h3 className="text-base font-semibold text-claude-text flex items-center gap-2">{icon}{title}</h3>
    {children}
  </div>
);

const Row: React.FC<{ label: string; value: string; ok?: boolean }> = ({ label, value, ok }) => (
  <div className="flex justify-between text-sm">
    <span className="text-claude-textSecondary">{label}</span>
    <span className={ok === true ? 'text-green-400' : ok === false ? 'text-red-400' : 'text-claude-text'}>{value}</span>
  </div>
);

export default TencentDBPanel;
