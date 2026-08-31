import React, { useState } from 'react';
import {
  FileText, Pencil, FilePlus, Terminal, Search, FolderOpen,
  Globe, HelpCircle, Monitor, Check, X, ChevronDown,
} from 'lucide-react';
import { useI18n } from '../hooks/useI18n';

interface ToolCallEntry {
  id?: string;
  name: string;
  input?: any;
  result?: any;
  status?: 'running' | 'done' | 'error' | 'canceled';
  textBefore?: string;
  isExpanded?: boolean;
}

interface ToolCallCardProps {
  toolCall: ToolCallEntry;
  isStreaming?: boolean;
  onToggle?: () => void;
}

const TOOL_META: Record<string, { icon: React.ReactNode; color: string }> = {
  Read:       { icon: <FileText size={13} />, color: 'text-blue-500' },
  Write:      { icon: <FilePlus size={13} />, color: 'text-green-500' },
  Edit:       { icon: <Pencil size={13} />, color: 'text-amber-500' },
  MultiEdit:  { icon: <Pencil size={13} />, color: 'text-amber-500' },
  Bash:       { icon: <Terminal size={13} />, color: 'text-purple-500' },
  Glob:       { icon: <FolderOpen size={13} />, color: 'text-cyan-500' },
  Grep:       { icon: <Search size={13} />, color: 'text-cyan-500' },
  ListDir:    { icon: <FolderOpen size={13} />, color: 'text-amber-500' },
  WebFetch:   { icon: <Globe size={13} />, color: 'text-blue-500' },
  WebSearch:  { icon: <Globe size={13} />, color: 'text-blue-500' },
  AskUserQuestion: { icon: <HelpCircle size={13} />, color: 'text-pink-500' },
  computer_use:    { icon: <Monitor size={13} />, color: 'text-indigo-500' },
};

function basename(p: string): string {
  return p.split(/[/\\]/).filter(Boolean).pop() || p;
}

/** 主参数摘要：文件路径 / 命令 / 查询词等，显示在标题右侧 */
function getPrimaryArg(name: string, input: any): string {
  if (!input) return '';
  switch (name) {
    case 'Read':
    case 'Write':
    case 'Edit':
    case 'MultiEdit':
      return input.file_path || '';
    case 'Bash':
      return input.command || '';
    case 'Glob':
      return input.pattern || '';
    case 'Grep':
      return [input.pattern, input.path ? `(${basename(input.path)})` : ''].filter(Boolean).join(' ');
    case 'ListDir':
      return input.path || '.';
    case 'WebFetch':
      return input.url || '';
    case 'WebSearch':
      return input.query || '';
    case 'computer_use':
      return input.action || input.tool || '';
    default:
      return '';
  }
}

/** 从结果文本里统计 diff 行数 */
function getDiffStats(result: string | null | undefined): { added: number; removed: number } | null {
  if (!result || typeof result !== 'string') return null;
  let added = 0, removed = 0;
  for (const line of result.split('\n')) {
    if (line.startsWith('+') && !line.startsWith('+++')) added++;
    if (line.startsWith('-') && !line.startsWith('---')) removed++;
  }
  return added > 0 || removed > 0 ? { added, removed } : null;
}

function truncateOutput(text: string, maxLen = 600): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen) + '\n...';
}

export function ToolCallCard({ toolCall: tc }: ToolCallCardProps) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(tc.isExpanded ?? false);
  const meta = TOOL_META[tc.name] || { icon: <Search size={13} />, color: 'text-gray-400' };
  const status = tc.status || 'done';
  const isRunning = status === 'running';
  const isError = status === 'error';

  // 文件类工具：拆成 文件名 + 目录 两段展示
  const FILE_TOOLS = new Set(['Read', 'Write', 'Edit', 'MultiEdit']);
  const filePath = FILE_TOOLS.has(tc.name) && tc.input?.file_path ? String(tc.input.file_path) : '';
  const fileName = filePath ? basename(filePath) : '';
  const fileDir = filePath && fileName !== filePath ? filePath.slice(0, filePath.length - fileName.length) : '';
  const primaryArg = filePath ? '' : getPrimaryArg(tc.name, tc.input);

  const hasResult = tc.result != null && tc.result !== '';
  const resultText = typeof tc.result === 'string' ? tc.result : JSON.stringify(tc.result, null, 2);
  const diffStats = getDiffStats(resultText);

  // 标题动词
  const verbKey: Record<string, string> = {
    Read: 'chat.toolRead', Write: 'chat.toolWrite', Edit: 'chat.toolEdit', MultiEdit: 'chat.toolEdit',
    Bash: 'chat.toolBash', Grep: 'chat.toolGrep', Glob: 'chat.toolGlob', ListDir: 'chat.toolListDir',
    WebFetch: 'chat.toolWebFetch', WebSearch: 'chat.toolWebSearch',
    AskUserQuestion: 'chat.toolAsk', computer_use: 'chat.toolComputer',
  };
  const verb = verbKey[tc.name] ? t(verbKey[tc.name]) : tc.name;

  const canExpand = hasResult && !isRunning;

  return (
    <div className="my-1 rounded-lg border border-black/[0.06] dark:border-white/[0.07] bg-white/60 dark:bg-white/[0.02] overflow-hidden">
      {/* 卡片头：状态 + 工具图标 + 动词 + 主参数 + diff 统计 + 展开箭头 */}
      <div
        className={`flex items-center gap-2 px-2.5 py-1.5 text-[12.5px] transition-colors
          ${canExpand ? 'cursor-pointer select-none hover:bg-black/[0.03] dark:hover:bg-white/[0.04]' : ''}`}
        onClick={canExpand ? () => setExpanded(!expanded) : undefined}
      >
        <span className="flex-shrink-0 w-3.5 flex justify-center">
          {isRunning ? (
            <span className="inline-block w-2.5 h-2.5 border-[1.5px] border-blue-400 border-t-transparent rounded-full animate-spin" />
          ) : isError ? (
            <X size={13} className="text-red-400" />
          ) : (
            <Check size={13} className="text-green-500" />
          )}
        </span>

        <span className={`flex-shrink-0 ${meta.color}`}>{meta.icon}</span>

        <span className="flex-shrink-0 font-medium text-claude-textSecondary">
          {verb}
        </span>

        {filePath ? (
          <>
            <span className="flex-shrink-0 text-[12px] font-medium text-claude-text">{fileName}</span>
            {fileDir && (
              <span className="text-[11px] text-claude-textSecondary/70 font-mono truncate flex-1 min-w-0">{fileDir}</span>
            )}
          </>
        ) : primaryArg ? (
          <span className="text-claude-text font-mono text-[11.5px] truncate flex-1 min-w-0 opacity-90">
            {primaryArg}
          </span>
        ) : (
          <span className="flex-1" />
        )}

        {diffStats && !isRunning && (
          <span className="flex-shrink-0 text-[10.5px] font-mono flex gap-1.5">
            {diffStats.added > 0 && <span className="text-green-500">+{diffStats.added}</span>}
            {diffStats.removed > 0 && <span className="text-red-500">-{diffStats.removed}</span>}
          </span>
        )}

        {canExpand && (
          <ChevronDown
            size={13}
            className={`flex-shrink-0 text-claude-textSecondary transition-transform duration-200 ${expanded ? 'rotate-180' : ''}`}
          />
        )}
      </div>

      {/* 展开体：按工具类型渲染 */}
      {expanded && hasResult && (
        <div className="border-t border-black/[0.05] dark:border-white/[0.06]">
          {isError ? (
            <pre className="text-[12px] font-mono text-red-400 bg-red-500/5 p-2.5 overflow-x-auto max-h-[300px] overflow-y-auto whitespace-pre-wrap">
              {resultText}
            </pre>
          ) : tc.name === 'Bash' ? (
            <BashOutput output={resultText} isError={isError} />
          ) : tc.name === 'Read' ? (
            <ReadOutput output={resultText} filePath={tc.input?.file_path || ''} />
          ) : (tc.name === 'Edit' || tc.name === 'MultiEdit' || tc.name === 'Write') && diffStats ? (
            <DiffOutput output={resultText} />
          ) : (
            <pre className="text-[12px] font-mono text-claude-textSecondary p-2.5 overflow-x-auto max-h-[300px] overflow-y-auto whitespace-pre-wrap">
              {truncateOutput(resultText)}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

function BashOutput({ output, isError }: { output: string; isError: boolean }) {
  try {
    const data = JSON.parse(output);
    const stdout = data.stdout || '';
    const stderr = data.stderr || '';
    const exitCode = data.exit_code;
    const truncated = data.stdout_truncated || data.stderr_truncated;

    return (
      <div className="bg-[#FAF9F5] dark:bg-black/30">
        {stdout && (
          <pre className="text-[12px] font-mono text-claude-text p-3 overflow-x-auto max-h-[300px] overflow-y-auto whitespace-pre-wrap leading-relaxed">
            {stdout}{truncated && '\n... (truncated)'}
          </pre>
        )}
        {stderr && (
          <pre className={`text-[12px] font-mono p-3 overflow-x-auto max-h-[200px] overflow-y-auto whitespace-pre-wrap border-t border-black/[0.05] dark:border-white/[0.06] ${isError ? 'text-red-400' : 'text-yellow-600 dark:text-yellow-400'}`}>
            {stderr}
          </pre>
        )}
        {exitCode !== undefined && exitCode !== 0 && (
          <div className="px-3 py-1.5 text-[11px] font-mono text-red-400/80 border-t border-black/[0.05] dark:border-white/[0.06]">
            exit code: {exitCode}
          </div>
        )}
        {!stdout && !stderr && exitCode === undefined && (
          <pre className="text-[12px] font-mono text-claude-textSecondary p-3 whitespace-pre-wrap">(no output)</pre>
        )}
      </div>
    );
  } catch {
    return (
      <pre className="text-[12px] font-mono text-claude-textSecondary p-3 overflow-x-auto max-h-[300px] overflow-y-auto whitespace-pre-wrap">
        {output}
      </pre>
    );
  }
}

function ReadOutput({ output, filePath }: { output: string; filePath: string }) {
  return (
    <div className="bg-[#FAF9F5] dark:bg-black/30">
      <div className="flex items-center gap-2 px-3 py-1.5 text-[11px] text-claude-textSecondary border-b border-black/[0.05] dark:border-white/[0.06]">
        <FileText size={11} className="text-blue-500" />
        <span className="font-mono truncate">{filePath}</span>
      </div>
      <pre className="text-[12px] font-mono text-claude-textSecondary p-3 overflow-x-auto max-h-[400px] overflow-y-auto whitespace-pre-wrap leading-relaxed">
        {output}
      </pre>
    </div>
  );
}

function DiffOutput({ output }: { output: string }) {
  const lines = output.split('\n');
  return (
    <div className="bg-[#FAF9F5] dark:bg-black/30">
      <pre className="text-[12px] font-mono p-0 overflow-x-auto max-h-[400px] overflow-y-auto">
        {lines.map((line, i) => {
          let cls = 'text-gray-500 dark:text-gray-400';
          if (line.startsWith('+') && !line.startsWith('+++')) cls = 'text-green-600 dark:text-green-400 bg-green-500/10';
          if (line.startsWith('-') && !line.startsWith('---')) cls = 'text-red-500 dark:text-red-400 bg-red-500/10';
          if (line.startsWith('@@')) cls = 'text-cyan-600 dark:text-cyan-400';
          return <div key={i} className={`px-3 ${cls}`}>{line || ' '}</div>;
        })}
      </pre>
    </div>
  );
}

/** 聚合编辑/写入类工具调用，输出「修改了哪些文件 +增 -删」列表（供消息底部汇总卡使用） */
export function computeChangedFiles(toolCalls: ToolCallEntry[]): { path: string; added: number; removed: number }[] {
  const FILE_EDIT_TOOLS = ['Edit', 'MultiEdit', 'Write'];
  const fileMap = new Map<string, { added: number; removed: number }>();
  for (const tc of toolCalls || []) {
    if (FILE_EDIT_TOOLS.includes(tc.name) && tc.input?.file_path && tc.status !== 'running') {
      const stats = getDiffStats(typeof tc.result === 'string' ? tc.result : '') || { added: 0, removed: 0 };
      const cur = fileMap.get(tc.input.file_path) || { added: 0, removed: 0 };
      fileMap.set(tc.input.file_path, { added: cur.added + stats.added, removed: cur.removed + stats.removed });
    }
  }
  return [...fileMap.entries()].map(([path, s]) => ({ path, ...s }));
}

/** 汇总卡：「N个文件已更改 +X -Y」，单文件直接显示行，多文件可展开 */
export function FileChangesSummary({ files }: { files: { path: string; added: number; removed: number }[] }) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(files.length === 1);
  const totalAdded = files.reduce((s, f) => s + f.added, 0);
  const totalRemoved = files.reduce((s, f) => s + f.removed, 0);

  return (
    <div className="my-1 rounded-lg border border-black/[0.06] dark:border-white/[0.07] bg-white/60 dark:bg-white/[0.02] overflow-hidden">
      <div
        className={`flex items-center gap-2 px-2.5 py-1.5 text-[12.5px] transition-colors
          ${files.length > 1 ? 'cursor-pointer select-none hover:bg-black/[0.03] dark:hover:bg-white/[0.04]' : ''}`}
        onClick={files.length > 1 ? () => setExpanded(!expanded) : undefined}
      >
        <span className="flex-shrink-0 w-3.5 flex justify-center">
          <Check size={13} className="text-green-500" />
        </span>
        <span className="flex-shrink-0 text-amber-500"><Pencil size={13} /></span>
        <span className="flex-shrink-0 font-medium text-claude-textSecondary">
          {t('chat.filesChanged', { count: files.length })}
        </span>
        <span className="flex-1" />
        <span className="flex-shrink-0 text-[10.5px] font-mono flex gap-1.5">
          {totalAdded > 0 && <span className="text-green-500">+{totalAdded}</span>}
          {totalRemoved > 0 && <span className="text-red-500">-{totalRemoved}</span>}
        </span>
        {files.length > 1 && (
          <ChevronDown
            size={13}
            className={`flex-shrink-0 text-claude-textSecondary transition-transform duration-200 ${expanded ? 'rotate-180' : ''}`}
          />
        )}
      </div>
      {expanded && files.map((f) => {
        const name = basename(f.path);
        const dir = name !== f.path ? f.path.slice(0, f.path.length - name.length) : '';
        return (
          <div key={f.path} className="flex items-center gap-2 px-2.5 py-1.5 text-[12px] border-t border-black/[0.05] dark:border-white/[0.06]">
            <span className="flex-shrink-0 w-3.5" />
            <span className="flex-shrink-0 text-blue-500"><Pencil size={12} /></span>
            <span className="flex-shrink-0 font-medium text-claude-text">{name}</span>
            {dir && (
              <span className="text-[11px] text-claude-textSecondary/70 font-mono truncate flex-1 min-w-0">{dir}</span>
            )}
            <span className="flex-shrink-0 text-[10.5px] font-mono flex gap-1.5">
              {f.added > 0 && <span className="text-green-500">+{f.added}</span>}
              {f.removed > 0 && <span className="text-red-500">-{f.removed}</span>}
            </span>
          </div>
        );
      })}
    </div>
  );
}

/**
 * 渲染一条消息的全部工具调用 —— ZCode 风格：每个调用一张独立折叠卡片，
 * 末尾附「N个文件已更改」汇总卡（存在编辑/写入时）
 */
export function ToolCallList({
  toolCalls,
  isStreaming,
  hideSummary,
}: {
  toolCalls: ToolCallEntry[];
  isStreaming?: boolean;
  /** 汇总卡改由消息底部渲染时置 true，避免重复 */
  hideSummary?: boolean;
}) {
  if (!toolCalls || toolCalls.length === 0) return null;
  void isStreaming;

  const changedFiles = computeChangedFiles(toolCalls);

  return (
    <div className="my-1.5 space-y-0.5">
      {toolCalls.map((tc, i) => (
        <ToolCallCard key={tc.id || i} toolCall={tc} isStreaming={isStreaming} />
      ))}
      {!hideSummary && changedFiles.length > 0 && <FileChangesSummary files={changedFiles} />}
    </div>
  );
}
