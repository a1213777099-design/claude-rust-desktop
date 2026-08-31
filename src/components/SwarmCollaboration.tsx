import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  Brain, Users, Zap, Loader2, CheckCircle2, XCircle, Clock,
  ArrowRight, GitBranch, Activity, Target, Sparkles, Send, FolderOpen,
  Bot, Square, RotateCcw, ChevronRight, Plus, Trash2, MessageSquare, Pencil, Terminal, AlertTriangle,
} from 'lucide-react';
import { workflowEventStream, WorkflowEvent, swarmCreateSession, swarmListSessions, swarmGetMessages, swarmAddMessage, swarmUpdateStatus, swarmDeleteSession, swarmRenameSession, getUserModels, getProviderModels, type SwarmSession, type SwarmMessage } from '../api';

// ─── 类型定义 ────────────────────────────────────────────────────────────────
interface AgentInfo {
  id: string;
  name: string;
  role: string;
  icon: string;
  color: string;
  status: 'idle' | 'working' | 'completed' | 'failed';
  output?: string;
}

interface ChatMessage {
  id: string;
  role: 'system' | 'agent';
  content: string;
  agentName?: string;
  agentIcon?: string;
  agentColor?: string;
  type?: string;
  meta?: {
    phase?: string;
    tool?: string;
    status?: 'running' | 'done';
    iteration?: number;
    elapsed_s?: number;
    chars?: number;
    agentRole?: string;
  };
  timestamp: number;
}

const META_AGENTS: AgentInfo[] = [
  { id: 'pm', name: '产品经理', role: 'ProductManager', icon: '\u{1F4CB}', color: '#3B82F6', status: 'idle' },
  { id: 'architect', name: '架构师', role: 'Architect', icon: '\u{1F3D7}', color: '#8B5CF6', status: 'idle' },
  { id: 'engineer', name: '工程师', role: 'Engineer', icon: '\u{1F4BB}', color: '#10B981', status: 'idle' },
  { id: 'reviewer', name: '审查员', role: 'Reviewer', icon: '\u{1F50D}', color: '#F59E0B', status: 'idle' },
  { id: 'qa', name: '测试工程师', role: 'QaEngineer', icon: '\u{1F9EA}', color: '#EF4444', status: 'idle' },
  { id: 'devops', name: '运维工程师', role: 'DevOps', icon: '\u{1F680}', color: '#06B6D4', status: 'idle' },
  { id: 'projmgr', name: '项目经理', role: 'ProjectManager', icon: '\u{1F4CA}', color: '#EC4899', status: 'idle' },
];

// ─── 主组件 ──────────────────────────────────────────────────────────────────
const SwarmCollaboration: React.FC = () => {
  const [inputTask, setInputTask] = useState('');
  const [agents, setAgents] = useState<AgentInfo[]>(META_AGENTS.map(a => ({ ...a })));
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isRunning, setIsRunning] = useState(false);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const abortRef = useRef(false);
  const chatEndRef = useRef<HTMLDivElement>(null);
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);
  const [workspace, setWorkspace] = useState<string>(() => localStorage.getItem('metagpt_workspace') || '');
  // 协作工作流使用的模型
  const [modelOptions, setModelOptions] = useState<{ id: string; name: string }[]>([]);
  const [selectedModel, setSelectedModel] = useState<string>(() => localStorage.getItem('swarm_model') || '');

  useEffect(() => {
    (async () => {
      try {
        const data = await getUserModels();
        const list = [...(data?.common || []), ...(data?.all || [])];
        const seen = new Set<string>();
        const opts = list
          .filter((m: any) => m?.id && !seen.has(m.id) && (seen.add(m.id), true))
          .map((m: any) => ({ id: m.id, name: m.name || m.id }));
        if (opts.length > 0) { setModelOptions(opts); return; }
        // 兜底：本地 provider 模型
        try {
          const pModels = await getProviderModels();
          if (Array.isArray(pModels)) {
            setModelOptions(pModels.filter((m: any) => m?.id).map((m: any) => ({ id: m.id, name: m.name || m.id })));
          }
        } catch {}
      } catch (e) {
        console.warn('[Swarm] Failed to load models:', e);
      }
    })();
  }, []);

  // 工作流暂停（429 等可重试错误）与续跑
  const [paused, setPaused] = useState<{ failedRole: string; error: string } | null>(null);
  const completedOutputsRef = useRef<Array<{ name: string; cause_by: string; output: string }>>([]);
  const lastGoalRef = useRef('');

  // Session persistence state
  const [sessions, setSessions] = useState<SwarmSession[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);
  const [loadingSessions, setLoadingSessions] = useState(false);
  const sessionIdRef = useRef<string | null>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState('');

  const handleSelectWorkspace = useCallback(async () => {
    try {
      const { tauriAPI } = await import('../utils/tauriAPI');
      const dir = await tauriAPI.selectDirectory();
      if (dir) {
        setWorkspace(dir);
        localStorage.setItem('metagpt_workspace', dir);
      }
    } catch (e) {
      console.warn('[Swarm] Failed to select directory:', e);
    }
  }, []);

  // Load sessions on mount

  const handleSelectSession = useCallback(async (sessionId: string, sessionData?: SwarmSession) => {
    try {
      const session = sessionData || sessions.find(s => s.id === sessionId);
      if (session && session.workspace) {
        setWorkspace(session.workspace);
        localStorage.setItem('metagpt_workspace', session.workspace);
      }
      const msgs = await swarmGetMessages(sessionId);
      console.log('[Swarm] Loaded messages:', msgs.length, 'for session', sessionId);
      setMessages(msgs.map((m: SwarmMessage) => ({
        id: m.id,
        role: m.role as 'system' | 'agent',
        content: m.content,
        agentName: m.agent_name || undefined,
        agentIcon: m.agent_icon || undefined,
        agentColor: m.agent_color || undefined,
        type: m.type || undefined,
        timestamp: m.created_at * 1000,
      })));
      setCurrentSessionId(sessionId); localStorage.setItem("swarm_current_session", sessionId);
      sessionIdRef.current = sessionId;

      if (session && session.agent_status) {
        try {
          const statusMap = JSON.parse(session.agent_status);
          setAgents(META_AGENTS.map(a => ({
            ...a,
            status: statusMap[a.id] || 'idle',
          })));
        } catch(e){console.warn("[Swarm] save failed:",e);}
      }
      setIsRunning(false);
      setInputTask('');
    } catch (e) {
      console.warn('[Swarm] Failed to load session:', e);
    }
  }, [sessions]);

  const loadSessions = useCallback(async () => {
    setLoadingSessions(true);
    try {
      const list = await swarmListSessions();
      setSessions(list);
      if (list.length > 0) {
        const lastId = localStorage.getItem('swarm_current_session');
        const toLoad = lastId && list.find(s => s.id === lastId) ? lastId : list[0].id;
        const sessionObj = list.find(s => s.id === toLoad);
        handleSelectSession(toLoad, sessionObj);
      }
    } catch (e) {
      console.warn('[Swarm] Failed to load sessions:', e);
    }
    setLoadingSessions(false);
  }, [handleSelectSession]);

  useEffect(() => {
    loadSessions();
  }, []);

  // Delete a session
  const handleDeleteSession = useCallback(async (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await swarmDeleteSession(sessionId);
      setSessions(prev => prev.filter(s => s.id !== sessionId));
      if (currentSessionId === sessionId) {
        setCurrentSessionId(null);
        sessionIdRef.current = null;
        setMessages([]);
      }
    } catch (e) {
      console.warn('[Swarm] Failed to delete session:', e);
    }
  }, [currentSessionId]);

  // Auto scroll
  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const addMessage = useCallback((msg: Omit<ChatMessage, 'id' | 'timestamp'>) => {
    setMessages(prev => [...prev, { ...msg, id: 'msg_' + Date.now() + '_' + Math.random(), timestamp: Date.now() }]);
  }, []);

  const roleToAgent = useCallback((role: string): AgentInfo | undefined => {
    // 返工轮次（EngineerRework）复用工程师卡片展示
    const normalized = role === 'EngineerRework' ? 'Engineer' : role;
    return META_AGENTS.find(a => a.role === normalized || a.name === normalized);
  }, []);

  // 工作流执行核心：首次启动与暂停后重试续跑共用
  const runWorkflow = useCallback(async (goal: string, resumeRoles: Array<{ name: string; cause_by: string; output: string }>) => {
    if (!goal || isRunning) return;
    setIsRunning(true);
    setIsAnalyzing(true);
    abortRef.current = false;
    setMessages([]);
    setPaused(null);
    completedOutputsRef.current = [...resumeRoles];
    lastGoalRef.current = goal;
    setAgents(META_AGENTS.map(a => {
      const resumed = resumeRoles.find(r => r.name === a.role || (a.role === 'Engineer' && r.name === 'EngineerRework'));
      return resumed
        ? { ...a, status: 'completed' as const, output: resumed.output }
        : { ...a, status: 'idle' as const, output: undefined };
    }));

    // 复用当前会话：已选中会话时直接往里写，不再每次发送都新建会话卡片；
    // 仅在没有任何选中会话时才自动创建
    let sessionId: string | null = sessionIdRef.current;
    if (!sessionId) {
      try {
        const sessionResp = await swarmCreateSession(goal.trim().slice(0, 100), workspace || undefined);
        sessionId = typeof sessionResp === 'object' ? sessionResp.id : sessionResp;
        console.log('[Swarm] Created session:', sessionId, 'type:', typeof sessionId);
        setCurrentSessionId(sessionId);
        localStorage.setItem('swarm_current_session', sessionId);
        sessionIdRef.current = sessionId;
      } catch (e) {
        console.warn('[Swarm] Failed to create session:', e);
      }
    }

    // 回放已完成角色的产出卡片，保留上下文
    for (const r of resumeRoles) {
      const ag = roleToAgent(r.name);
      if (ag && r.output) {
        addMessage({ role: 'agent', agentName: ag.name, agentIcon: ag.icon, agentColor: ag.color, content: r.output, type: 'report' });
      }
    }

    addMessage({ role: 'system', content: (resumeRoles.length > 0 ? '\u{1F501} 重试续跑：已完成 ' + resumeRoles.length + ' 个角色，从断点继续...' : '\u{1F680} 正在启动 MetaGPT 智能体团队...'), type: 'start' });
    if (sessionId) {
      try {
        await swarmAddMessage(sessionId, 'system', '\u{1F680} 正在启动 MetaGPT 智能体团队...', undefined, undefined, undefined, 'start');
      } catch(e){console.warn("[Swarm] save failed:",e);}
    }

    const agentStatusMap: Record<string, string> = {};
    for (const r of resumeRoles) {
      const ag = roleToAgent(r.name);
      if (ag) agentStatusMap[ag.id] = 'completed';
    }
    let wasPaused = false;

    try {
      for await (const event of workflowEventStream(goal, undefined, selectedModel || undefined, workspace || undefined, resumeRoles.length > 0 ? resumeRoles : undefined)) {
        if (abortRef.current) break;
        const et = event.event_type;
        const role = event.data?.agent_role || '';
        const agent = roleToAgent(role);

        if (et === 'workflow_start') {
          setIsAnalyzing(false);
          const infoMsg = '\u{1F9E0} 主脑已调度 ' + (event.data?.roles || 7) + ' 个智能体开始协作';
          addMessage({ role: 'system', content: infoMsg, type: 'info' });
          if (sessionId) {
            try { await swarmAddMessage(sessionId, 'system', infoMsg, undefined, undefined, undefined, 'info'); } catch(e){console.warn("[Swarm] save failed:",e);}
          }
        } else if (et === 'task_started') {
          if (agent) {
            setAgents(prev => prev.map(a => a.id === agent.id ? { ...a, status: 'working' } : a));
            agentStatusMap[agent.id] = 'working';
            const startMsg = agent.icon + ' ' + agent.name + ' 开始工作...';
            addMessage({ role: 'system', content: startMsg, type: 'progress' });
            if (sessionId) {
              try { await swarmAddMessage(sessionId, 'system', startMsg, agent.name, agent.icon, agent.color, 'progress'); } catch(e){console.warn("[Swarm] save failed:",e);}
            }
          }
        } else if (et === 'task_progress') {
          const phase = event.data?.phase as string | undefined;

          if (phase === 'tool') {
            // ZCode 风格工具卡：新工具事件把之前的 running 卡标记为完成
            const toolName = String(event.data?.tool || event.message || 'tool');
            const activityRole = event.task_id || role || 'workflow';
            setMessages(prev => [
              ...prev.map(m => m.meta?.status === 'running' ? { ...m, meta: { ...m.meta, status: 'done' as const } } : m),
              { id: 'msg_' + Date.now() + '_' + Math.random(), timestamp: Date.now(), role: 'system' as const, content: toolName, type: 'tool', meta: { phase: 'tool', tool: toolName, status: 'running' as const, agentRole: activityRole } },
            ]);
            if (sessionId) {
              try { await swarmAddMessage(sessionId, 'system', toolName, activityRole, agent?.icon, agent?.color, 'tool'); } catch(e){console.warn('[Swarm] save failed:',e);}
            }
          } else if (phase === 'thinking' || phase === 'waiting' || phase === 'output' || phase === 'output_done') {
            // 活动卡：按角色 upsert（心跳 10s 一条，追加会刷屏）
            const activityRole = event.task_id || role || 'workflow';
            setMessages(prev => {
              const marked = prev.map(m => m.meta?.status === 'running' ? { ...m, meta: { ...m.meta, status: 'done' as const } } : m);
              const filtered = marked.filter(m => !(m.type === 'activity' && m.meta?.agentRole === activityRole));
              return [...filtered, { id: 'act_' + activityRole, timestamp: Date.now(), role: 'system' as const, content: '', type: 'activity', meta: { phase, agentRole: activityRole, iteration: event.data?.iteration, elapsed_s: event.data?.elapsed_s, chars: event.data?.chars } }];
            });
          } else {
            // 兜底：无结构化 data 的历史/未知事件走原文本样式
            const progMsg = agent
              ? '\u2022 ' + agent.icon + ' ' + event.message
              : '\u2022 ' + event.message;
            addMessage({ role: 'system', content: progMsg, type: 'progress' });
            if (sessionId) {
              try { await swarmAddMessage(sessionId, 'system', progMsg, agent?.name, agent?.icon, agent?.color, 'progress'); } catch(e){console.warn("[Swarm] save failed:",e);}
            }
          }
        } else if (et === 'task_completed') {
          setMessages(prev => prev.map(m => m.meta?.status === 'running' ? { ...m, meta: { ...m.meta, status: 'done' as const } } : m));
          let output = '';
          if (event.data?.output) {
            output = typeof event.data.output === 'object' ? (event.data.output.output || JSON.stringify(event.data.output, null, 2)) : String(event.data.output);
          }
          if (!output) output = event.message || '任务完成';
          if (event.data?.agent_role && output) {
            completedOutputsRef.current.push({ name: event.data.agent_role, cause_by: event.data?.cause_by || 'General', output });
          }
          if (agent) {
            setAgents(prev => prev.map(a => a.id === agent.id ? { ...a, status: 'completed', output } : a));
            agentStatusMap[agent.id] = 'completed';
            addMessage({ role: 'agent', agentName: agent.name, agentIcon: agent.icon, agentColor: agent.color, content: output, type: 'report' });
            if (sessionId) {
              try { await swarmAddMessage(sessionId, 'agent', output, agent.name, agent.icon, agent.color, 'report'); } catch(e){console.warn("[Swarm] save failed:",e);}
            }
          }
        } else if (et === 'task_failed') {
          if (agent) {
            setAgents(prev => prev.map(a => a.id === agent.id ? { ...a, status: 'failed' } : a));
            agentStatusMap[agent.id] = 'failed';
            const failMsg = '\u274C 失败: ' + (event.message || '未知错误');
            addMessage({ role: 'agent', agentName: agent.name, agentIcon: agent.icon, agentColor: '#EF4444', content: failMsg, type: 'error' });
            if (sessionId) {
              try { await swarmAddMessage(sessionId, 'agent', failMsg, agent.name, agent.icon, '#EF4444', 'error'); } catch(e){console.warn("[Swarm] save failed:",e);}
            }
          }
        } else if (et === 'workflow_resumed') {
          addMessage({ role: 'system', content: '\u{1F501} 已回放 ' + (event.data?.replayed || 0) + ' 个已完成角色，从断点继续', type: 'info' });
        } else if (et === 'workflow_paused') {
          // 可重试错误（429/限流/过载等）：暂停工作流，等待用户点击重试续跑
          wasPaused = true;
          const failedRole = String(event.data?.failed_role || event.task_id || '');
          const ag = roleToAgent(failedRole);
          if (ag) {
            setAgents(prev => prev.map(a => a.id === ag.id ? { ...a, status: 'failed' } : a));
            agentStatusMap[ag.id] = 'failed';
          }
          const pauseMsg = '\u23F8\uFE0F 工作流已暂停：' + (event.data?.error || event.message || 'API 繁忙');
          addMessage({ role: 'system', content: pauseMsg, type: 'error' });
          if (sessionId) {
            try { await swarmAddMessage(sessionId, 'system', pauseMsg, undefined, undefined, undefined, 'error'); } catch(e){console.warn('[Swarm] save failed:',e);}
          }
          setPaused({ failedRole, error: String(event.data?.error || event.message || 'API 繁忙') });
        } else if (et === 'workflow_completed') {
          const dur = event.data?.duration_ms ? (event.data.duration_ms / 1000).toFixed(1) + '秒' : '';
          const doneMsg = '\u{1F389} 工作流已完成！' + (dur ? ' 耗时: ' + dur : '');
          addMessage({ role: 'system', content: doneMsg, type: 'done' });
          if (sessionId) {
            try {
              await swarmAddMessage(sessionId, 'system', doneMsg, undefined, undefined, undefined, 'done');
              await swarmUpdateStatus(sessionId, 'completed', agentStatusMap);
            } catch(e){console.warn("[Swarm] save failed:",e);}
          }
        } else if (et === 'workflow_failed') {
          const errMsg = '\u274C 工作流失败: ' + (event.message || '');
          addMessage({ role: 'system', content: errMsg, type: 'error' });
          if (sessionId) {
            try {
              await swarmAddMessage(sessionId, 'system', errMsg, undefined, undefined, undefined, 'error');
              await swarmUpdateStatus(sessionId, 'failed', agentStatusMap);
            } catch(e){console.warn("[Swarm] save failed:",e);}
          }
        }
      }
    } catch (err) {
      const errMsg = '\u274C 出错: ' + String(err);
      addMessage({ role: 'system', content: errMsg, type: 'error' });
      if (sessionId) {
        try { await swarmAddMessage(sessionId, 'system', errMsg, undefined, undefined, undefined, 'error'); } catch(e){console.warn("[Swarm] save failed:",e);}
      }
    }

    // 暂停→paused；正常/失败→completed
    if (sessionId) {
      try { await swarmUpdateStatus(sessionId, wasPaused ? 'paused' : 'completed', agentStatusMap); } catch(e){console.warn("[Swarm] save failed:",e);}
    }

    setIsRunning(false);
    setIsAnalyzing(false);
    loadSessions();
  }, [isRunning, addMessage, roleToAgent, workspace, loadSessions, selectedModel]);

  const handleStart = useCallback(async () => {
    if (!inputTask.trim() || isRunning) return;
    await runWorkflow(inputTask.trim(), []);
  }, [inputTask, isRunning, runWorkflow]);

  // 429 暂停后的重试：带已完成角色产出续跑
  const handleRetry = useCallback(async () => {
    if (isRunning || !paused) return;
    await runWorkflow(lastGoalRef.current, completedOutputsRef.current);
  }, [isRunning, paused, runWorkflow]);

  const handleStop = useCallback(() => { abortRef.current = true; setIsRunning(false); }, []);
  const handleReset = useCallback(() => {
    setPaused(null);
    setMessages([]);
    setAgents(META_AGENTS.map(a => ({ ...a, status: 'idle' as const, output: undefined })));
    setInputTask('');
    setIsRunning(false);
    setCurrentSessionId(null);
    sessionIdRef.current = null;
  }, []);


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

  const handleNewTask = useCallback(async () => {
    let selectedDir = '';
    try {
      const { tauriAPI } = await import('../utils/tauriAPI');
      selectedDir = await tauriAPI.selectDirectory() || '';
    } catch (e) {
      console.warn('[Swarm] select dir failed:', e);
    }
    try {
      const title = '\u65B0\u4EFB\u52A1 ' + new Date().toLocaleString('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' });
      const sessionResp2 = await swarmCreateSession(title, selectedDir || undefined);
      const sessionId = typeof sessionResp2 === 'object' ? sessionResp2.id : sessionResp2;
      await loadSessions();
      setCurrentSessionId(sessionId);
      sessionIdRef.current = sessionId;
      setWorkspace(selectedDir);
      if (selectedDir) localStorage.setItem('metagpt_workspace', selectedDir);
      setMessages([]);
      setAgents(META_AGENTS.map(a => ({ ...a, status: 'idle' as const, output: undefined })));
      setInputTask('');
      setIsRunning(false);
      setTimeout(() => inputRef.current?.focus(), 100);
    } catch (e) {
      console.warn('[Swarm] create session failed:', e);
    }
  }, [loadSessions]);

  const completedCount = agents.filter(a => a.status === 'completed').length;
  const failedCount = agents.filter(a => a.status === 'failed').length;
  const workingCount = agents.filter(a => a.status === 'working').length;

  const formatTime = (ts: number) => {
    const d = new Date(ts * 1000);
    return d.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' }) + ' ' + d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'completed': return '\u2705';
      case 'failed': return '\u274C';
      case 'running': return '\u23F3';
      case 'paused': return '\u23F8\uFE0F';
      default: return '\u{1F4CB}';
    }
  };

  return (
    <div className="flex h-full bg-claude-bg">
      {/* ─── 左侧 Agent 列表 ─── */}
      <div className="w-56 flex-shrink-0 border-r border-claude-border flex flex-col bg-claude-bg/50">
        <div className="p-3 border-b border-claude-border">
          <div className="flex items-center gap-2 mb-2">
            <Users size={16} className="text-[#3B82F6]" />
            <span className="text-[13px] font-semibold text-claude-text">智能体团队</span>
          </div>
          <div className="flex items-center gap-3 text-[11px] text-claude-textSecondary">
            <span>✅ {completedCount}</span>
            <span>⚡ {workingCount}</span>
            <span>❌ {failedCount}</span>
            <span>⏳ {agents.length - completedCount - workingCount - failedCount}</span>
          </div>
          {/* 任务进度 */}
          {isRunning && (
            <div className="mt-2">
              <div className="flex items-center justify-between text-[10px] text-claude-textSecondary mb-1">
                <span>任务进度</span>
                <span>{completedCount + failedCount}/{agents.length}</span>
              </div>
              <div className="w-full h-1.5 bg-claude-border/30 rounded-full overflow-hidden">
                <div
                  className={"h-full rounded-full transition-all duration-500" + (workingCount > 0 ? " animate-pulse " : " ") + (completedCount + failedCount > 0 ? "bg-emerald-500" : "bg-claude-border/20")}
                  style={{ width: Math.round((completedCount + failedCount) / Math.max(1, agents.length) * 100) + "%" }}
                />
              </div>
            </div>
          )}
        </div>
        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {/* 工作区选择 */}
          <div className="mb-2 px-2">
            <div className="text-[10px] text-claude-textSecondary mb-1">工作区</div>
            <div
              className="w-full flex items-center gap-1.5 px-2 py-1.5 text-[11px] rounded-md border border-claude-border text-claude-textSecondary truncate"
              title={workspace || '未设置'}
            >
              <FolderOpen size={12} className="flex-shrink-0" />
              <span className="truncate">{workspace ? workspace.split(/[\\/]/).pop() || workspace : '未设置'}</span>
            </div>
            {workspace && (
              <div className="text-[9px] text-claude-textSecondary/60 mt-0.5 truncate" title={workspace}>{workspace}</div>
            )}
          </div>
          {agents.map(agent => (
            <div
              key={agent.id}
              onClick={() => setSelectedAgent(selectedAgent === agent.id ? null : agent.id)}
              className={`flex items-center gap-2 px-2.5 py-2 rounded-lg cursor-pointer transition-all text-[12px]${
                agent.status === 'working' ? ' bg-blue-500/10 border border-blue-500/20' :
                agent.status === 'completed' ? ' bg-emerald-500/10 border border-emerald-500/20' :
                agent.status === 'failed' ? ' bg-red-500/10 border border-red-500/20' :
                ' hover:bg-claude-hover border border-transparent'
              }`}
            >
              <span className="text-[16px]">{agent.icon}</span>
              <div className="flex-1 min-w-0">
                <div className="font-medium text-claude-text truncate">{agent.name}</div>
                <div className="text-[10px] text-claude-textSecondary truncate">{agent.role}</div>
              </div>
              <div className="flex-shrink-0">
                {agent.status === 'idle' && <Clock size={12} className="text-gray-400" />}
                {agent.status === 'working' && <Loader2 size={12} className="text-blue-400 animate-spin" />}
                {agent.status === 'completed' && <CheckCircle2 size={12} className="text-emerald-400" />}
                {agent.status === 'failed' && <XCircle size={12} className="text-red-400" />}
              </div>
            </div>
          ))}
        </div>
        {/* 选中 Agent 的输出 */}
        {selectedAgent && (() => {
          const ag = agents.find(a => a.id === selectedAgent);
          if (!ag?.output) return null;
          return (
            <div className="border-t border-claude-border p-3 max-h-[200px] overflow-y-auto">
              <div className="text-[11px] font-medium text-claude-textSecondary mb-1">{ag.icon} {ag.name} 输出</div>
              <div className="text-[11px] text-claude-text whitespace-pre-wrap leading-relaxed">{ag.output.slice(0, 1000)}{ag.output.length > 1000 ? '...' : ''}</div>
            </div>
          );
        })()}
      </div>

      {/* ─── 中间聊天室 ─── */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* 顶部标题栏 */}
        <div className="flex-shrink-0 px-4 py-3 border-b border-claude-border flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Brain size={16} className="text-[#8B5CF6]" />
            <span className="text-[14px] font-semibold text-claude-text">MetaGPT 智能协作</span>
            {isRunning && (
              <span className="ml-2 px-2 py-0.5 text-[10px] rounded-full bg-blue-500/10 text-blue-400 border border-blue-500/20">
                {workingCount > 0 ? workingCount + ' 个智能体工作中' : '运行中'}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            {isRunning && (
              <button onClick={handleStop} className="flex items-center gap-1 px-2 py-1 text-[11px] text-red-400 hover:bg-red-500/10 rounded-md transition-colors">
                <Square size={12} /> 停止
              </button>
            )}
            {!isRunning && messages.length > 0 && (
              <button onClick={handleReset} className="flex items-center gap-1 px-2 py-1 text-[11px] text-claude-textSecondary hover:bg-claude-hover rounded-md transition-colors">
                <RotateCcw size={12} /> 重置
              </button>
            )}
          </div>
        </div>

        {/* 消息区域 */}
        <div className="flex-1 overflow-y-auto px-4 py-4 space-y-4">
          {messages.length === 0 && !isRunning && (
            <div className="flex flex-col items-center justify-center h-full text-center">
              <div className="w-16 h-16 rounded-full bg-claude-hover flex items-center justify-center mb-4">
                <Bot size={28} className="text-[#8B5CF6]" />
              </div>
              <h3 className="text-[15px] font-medium text-claude-text mb-2">MetaGPT 智能体协作</h3>
              <p className="text-[12px] text-claude-textSecondary max-w-sm leading-relaxed">
                输入任务，7个专业智能体将按照 MetaGPT 原生架构自动协作：产品经理→架构师→工程师→审查员→测试→运维→项目经理
              </p>
              <div className="mt-4 flex flex-wrap justify-center gap-2">
                {META_AGENTS.map(a => (
                  <span key={a.id} className="px-2 py-1 text-[10px] rounded-full border border-claude-border text-claude-textSecondary">
                    {a.icon} {a.name}
                  </span>
                ))}
              </div>
            </div>
          )}

          {messages.map(msg => (
            <div key={msg.id}>
              {msg.role === 'system' ? (
                msg.type === 'tool' ? (
                  // ZCode 风格工具调用卡
                  <div className="flex justify-start">
                    <div className="w-full max-w-[560px] rounded-lg border border-black/[0.06] dark:border-white/[0.07] bg-white/60 dark:bg-white/[0.02] px-2.5 py-1.5 flex items-center gap-2">
                      {msg.meta?.status === 'running' ? (
                        <span className="flex-shrink-0 inline-block w-2.5 h-2.5 border-[1.5px] border-blue-400 border-t-transparent rounded-full animate-spin" />
                      ) : (
                        <CheckCircle2 size={13} className="flex-shrink-0 text-green-500" />
                      )}
                      <Terminal size={13} className="flex-shrink-0 text-purple-500" />
                      <span className="flex-shrink-0 text-[12px] font-medium text-claude-textSecondary">调用工具</span>
                      <span className="flex-1 min-w-0 text-[11.5px] font-mono text-claude-text truncate opacity-90">{msg.content}</span>
                    </div>
                  </div>
                ) : msg.type === 'activity' ? (
                  // 智能体活动卡（思考/等待/输出完成），心跳更新不刷屏
                  <div className="flex justify-start">
                    <div className="w-full max-w-[560px] rounded-lg border border-amber-500/20 bg-amber-500/5 dark:bg-amber-500/5 px-2.5 py-1.5 flex items-center gap-2">
                      <Brain size={13} className="flex-shrink-0 text-amber-500 animate-pulse" />
                      <span className="flex-shrink-0 text-[12px] font-medium text-claude-textSecondary">
                        {msg.meta?.phase === 'waiting'
                          ? `等待LLM响应 · ${msg.meta?.elapsed_s || 0}s`
                          : msg.meta?.phase === 'output_done'
                            ? `思考完成 · ${msg.meta?.chars ?? 0} 字符`
                            : `思考中 · 迭代 ${msg.meta?.iteration ?? '-'}`
                        }
                      </span>
                      <span className="flex-1 min-w-0 text-[11px] font-mono text-claude-textSecondary/60 truncate text-right">{msg.meta?.agentRole}</span>
                    </div>
                  </div>
                ) : msg.type === 'progress' ? (
                  // Progress messages: left-aligned, detailed style
                  <div className="flex gap-2 items-start">
                    <div className="flex-shrink-0 w-6 h-6 rounded-full bg-claude-hover flex items-center justify-center text-[10px] mt-0.5">
                      🤖
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="text-[11px] text-claude-textSecondary mb-0.5">🔧 工作流进度</div>
                      <div className="text-[12px] text-claude-text bg-claude-hover/50 border border-claude-border/50 rounded px-2 py-1.5 whitespace-pre-wrap leading-relaxed">
                        {msg.content}
                      </div>
                    </div>
                  </div>
                ) : (
                  // Other system messages: centered pill style
                  <div className="flex justify-center">
                    <div className={`px-3 py-1.5 rounded-full text-[11px]${
                      msg.type === 'error' ? ' bg-red-500/10 text-red-400 border border-red-500/20' :
                      msg.type === 'done' ? ' bg-emerald-500/10 text-emerald-400 border border-emerald-500/20' :
                      ' bg-claude-hover text-claude-textSecondary border border-claude-border'
                    }`}>{msg.content}</div>
                  </div>
                )
              ) : (
                <div className="flex gap-3">
                  <div className="flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center text-[14px]" style={{ backgroundColor: (msg.agentColor || '#3B82F6') + '20' }}>
                    {msg.agentIcon || '\u{1F916}'}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="text-[11px] font-medium mb-1" style={{ color: msg.agentColor || '#3B82F6' }}>
                      {msg.agentName || '智能体'}
                    </div>
                    <div className="bg-claude-hover border border-claude-border rounded-lg px-3 py-2 text-[12px] text-claude-text whitespace-pre-wrap leading-relaxed max-h-[400px] overflow-y-auto">
                      {msg.content}
                    </div>
                  </div>
                </div>
              )}
            </div>
          ))}
          {paused && !isRunning && (
            <div className="flex justify-center">
              <div className="flex items-center gap-3 px-4 py-2.5 rounded-xl border border-amber-500/30 bg-amber-500/10">
                <AlertTriangle size={16} className="flex-shrink-0 text-amber-400" />
                <div className="text-left min-w-0">
                  <div className="text-[12px] font-medium text-claude-text">
                    工作流已暂停：{paused.failedRole || '智能体'} 因 API 繁忙中断
                  </div>
                  <div className="text-[10.5px] text-claude-textSecondary max-w-[420px] truncate" title={paused.error}>
                    {paused.error}
                  </div>
                </div>
                <button
                  onClick={handleRetry}
                  className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium text-white bg-amber-500 hover:bg-amber-600 rounded-lg flex-shrink-0 transition-colors"
                >
                  <RotateCcw size={12} /> 重试续跑
                </button>
              </div>
            </div>
          )}
          <div ref={chatEndRef} />
        </div>

        {/* 底部输入栏 */}
        <div className="flex-shrink-0 border-t border-claude-border p-3">
          <div className="flex gap-2">
            <textarea
              ref={inputRef}
              value={inputTask}
              onChange={e => setInputTask(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleStart(); } }}
              placeholder="输入任务，让智能体团队协作完成... (Shift+Enter 换行)"
              disabled={isRunning}
              rows={2}
              className="flex-1 px-3 py-2 text-[13px] bg-claude-input border border-claude-border rounded-lg text-claude-text placeholder:text-claude-textSecondary/50 focus:outline-none focus:border-[#8B5CF6] disabled:opacity-50 resize-none"
            />
            <button
              onClick={isRunning ? handleStop : handleStart}
              disabled={!isRunning && !inputTask.trim()}
              className={`px-4 py-2 rounded-lg text-[12px] font-medium flex items-center gap-1.5 transition-colors disabled:opacity-50 disabled:cursor-not-allowed${
                isRunning
                  ? ' bg-red-500/10 text-red-400 border border-red-500/20 hover:bg-red-500/20'
                  : ' bg-[#8B5CF6] text-white hover:bg-[#7C3AED]'
              }`}
            >
              {isRunning ? <><Square size={14} /> 停止</> : <><Send size={14} /> 发送</>}
            </button>
          </div>
          <div className="flex items-center gap-4 mt-2 text-[10px] text-claude-textSecondary">
            <span>🏗️ MetaGPT 原生架构</span>
            <span>📡 消息驱动</span>
            <span>🤖 7个专业智能体</span>
            <label className="flex items-center gap-1" title="协作工作流使用的模型">
              <Brain size={10} className="text-[#8B5CF6]" />
              <select
                value={selectedModel}
                onChange={e => {
                  setSelectedModel(e.target.value);
                  try { localStorage.setItem('swarm_model', e.target.value); } catch {}
                }}
                disabled={isRunning}
                className="max-w-[150px] truncate bg-claude-input border border-claude-border rounded px-1 py-0.5 text-[10px] text-claude-textSecondary hover:text-claude-text focus:outline-none focus:border-[#8B5CF6] disabled:opacity-50 cursor-pointer"
              >
                <option value="">默认模型</option>
                {modelOptions.map(m => (
                  <option key={m.id} value={m.id}>{m.name}</option>
                ))}
              </select>
            </label>
            {isAnalyzing && <span className="text-blue-400 animate-pulse">⏳ 正在分析...</span>}
          </div>
        </div>
      </div>

      {/* ─── 右侧会话列表面板 ─── */}
      <div className="w-64 flex-shrink-0 border-l border-claude-border flex flex-col bg-claude-bg/50">
        <div className="p-3 border-b border-claude-border flex items-center justify-between">
          <div className="flex items-center gap-2">
            <MessageSquare size={14} className="text-[#8B5CF6]" />
            <span className="text-[12px] font-semibold text-claude-text">会话记录</span>
          </div>
          <button
            onClick={handleNewTask}
            className="p-1.5 rounded-md hover:bg-claude-hover transition-colors"
            title="新建任务"
          >
            <Plus size={16} className="text-claude-textSecondary" />
          </button>
        </div>
        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {loadingSessions && (
            <div className="flex items-center justify-center py-4">
              <Loader2 size={16} className="animate-spin text-claude-textSecondary" />
            </div>
          )}
          {!loadingSessions && sessions.length === 0 && (
            <div className="text-center py-8">
              <MessageSquare size={24} className="mx-auto text-claude-textSecondary/30 mb-2" />
              <p className="text-[11px] text-claude-textSecondary/50">暂无会话记录</p>
            </div>
          )}
           {sessions.map(session => (
            <div
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
                  <div className="font-medium text-claude-text truncate leading-tight">{session.title || '\u65B0\u4EFB\u52A1'}</div>
                )}
                {session.workspace && (
                  <div className="text-[9px] text-claude-textSecondary/70 truncate mt-0.5 flex items-center gap-1">
                    <FolderOpen size={9} className="flex-shrink-0" />
                    {session.workspace.split(/[\\/]/).pop() || session.workspace}
                  </div>
                )}
                <div className="text-[10px] text-claude-textSecondary mt-0.5">{formatTime(session.created_at)}</div>
              </div>
              <div className="flex-shrink-0 flex items-center gap-0.5">
                <button
                  onClick={(e) => { e.stopPropagation(); setEditingSessionId(session.id); setEditingTitle(session.title || ''); }}
                  className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-claude-hover transition-all"
                  title="\u91CD\u547D\u540D"
                >
                  <Pencil size={11} className="text-claude-textSecondary" />
                </button>
                <button
                  onClick={(e) => handleDeleteSession(session.id, e)}
                  className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-red-500/10 transition-all"
                  title="\u5220\u9664\u4F1A\u8BDD"
                >
                  <Trash2 size={11} className="text-red-400" />
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};

export default SwarmCollaboration;
