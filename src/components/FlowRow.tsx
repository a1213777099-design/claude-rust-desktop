/**
 * DSH 风格流程行组件库（参照 deepseek-harness ui-chat / ui-tool / ui-primitives）
 *
 * - FlowRow: 24px 单行折叠条（图标 + 标题 + 分隔点 + 摘要 + 后缀 + chevron）
 * - StateDot / ChaseDot: 四态状态点（done/warning/error）+ 运行像素追逐环
 * - ReasoningRow: 思考折叠条，流式时摘要跟随最新一行，结束定格为第一行
 * - ToolRow: 工具调用行（终端/读取/编辑/搜索等差异化变体 + IN/OUT 展开卡）
 */
import React, { useMemo, useState } from 'react';
import {
  FileText, Pencil, FilePlus, Terminal, Search, FolderOpen,
  Globe, HelpCircle, Monitor, ChevronDown, Sparkles, Code2,
} from 'lucide-react';

/* ============ 状态点 ============ */

export type DotState = 'done' | 'warning' | 'error';

/** 静态状态点：外圈 10% 光晕 + 内核实心 */
export function StateDot({ state, size = 10 }: { state: DotState; size?: number }) {
  return <span className="dsh-statedot" data-state={state} style={{ width: size, height: size }} aria-hidden />;
}

const CHASE_CELLS: readonly (readonly [number, number])[] = [
  [0, 0], [4, 0], [8, 0], [8, 4], [8, 8], [4, 8], [0, 8], [0, 4],
];

/** 运行中像素追逐环：3x3 矩阵格逐格点亮（StateDot ongoing 等价物） */
export function ChaseDot({ size = 10 }: { size?: number }) {
  return (
    <svg className="dsh-chase" width={size} height={size} viewBox="0 0 10 10" shapeRendering="crispEdges" aria-hidden>
      {CHASE_CELLS.map(([x, y], i) => (
        <rect key={`${x}-${y}`} x={x} y={y} width="2" height="2"
          style={{ animationDelay: `${(i - CHASE_CELLS.length) * 125}ms` }} />
      ))}
    </svg>
  );
}

/* ============ 24px 折叠条 ============ */

export interface FlowRowProps {
  icon: React.ReactNode;
  title: string;
  /** 折叠时显示的摘要（标题右侧，带分隔点） */
  summary?: React.ReactNode;
  /** 摘要是否跟随最新行（流式 thinking 用，右对齐不截断） */
  followEnd?: boolean;
  summaryError?: boolean;
  /** 摘要后的等宽小字后缀（如 diff 统计） */
  suffix?: React.ReactNode;
  open: boolean;
  expandable: boolean;
  onToggle: () => void;
  /** 折叠行状态：running 时套扫光动画 + 图标换追逐环 */
  running?: boolean;
  error?: boolean;
  stopped?: boolean;
  children?: React.ReactNode;
}

export function FlowRow({
  icon, title, summary, followEnd, summaryError, suffix,
  open, expandable, onToggle, running, error, stopped, children,
}: FlowRowProps) {
  const state = running ? 'running' : error ? 'error' : stopped ? 'stopped' : 'ok';
  const leading = error
    ? <StateDot state="error" />
    : stopped
      ? <StateDot state="warning" />
      : running
        ? <ChaseDot />
        : <span className="dsh-icon-idle">{icon}</span>;
  return (
    <div className="dsh-flowrow-root" data-variant={title} data-state={state}>
      <div
        className="dsh-flowrow"
        data-expandable={expandable || undefined}
        data-state={state}
        role={expandable ? 'button' : undefined}
        tabIndex={expandable ? 0 : undefined}
        aria-expanded={expandable ? open : undefined}
        onClick={expandable ? onToggle : undefined}
        onKeyDown={(e) => {
          if (expandable && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); onToggle(); }
        }}
      >
        <span className="dsh-flowrow-leading">
          {leading}
          {/* hover 时静态图标淡出、chevron 淡入（非运行态才显示 idle 图标） */}
          {!running && !error && !stopped && (
            <ChevronDown size={14} className="dsh-chevron-hover" />
          )}
        </span>
        <span className="dsh-flowrow-title">{title}</span>
        {summary != null && (
          <>
            <span className="dsh-flowrow-sep" aria-hidden />
            <span className="dsh-flowrow-summary" data-follow-end={followEnd || undefined} data-error={summaryError || undefined}>
              <span className="dsh-summary-text">{summary}</span>
            </span>
          </>
        )}
        {suffix != null && <span className="dsh-flowrow-suffix">{suffix}</span>}
      </div>
      {open && children}
    </div>
  );
}

/* ============ 思考行（ReasoningRow） ============ */

function firstLine(text: string): string {
  const nl = text.indexOf('\n');
  return nl === -1 ? text : text.slice(0, nl);
}

function latestLine(text: string): string {
  const visible = text.trimEnd();
  const nl = visible.lastIndexOf('\n');
  return nl === -1 ? visible : visible.slice(nl + 1);
}

/**
 * 思考折叠条：运行中显示"思考中"+ 摘要跟随最新一行思考（右对齐推进）；
 * 结束后标题变"思考"、摘要定格为第一行（"思考完输出一段小总结"的感知来源）。
 */
export function ReasoningRow({
  text, running, durationMs, defaultExpanded, onToggleExpanded,
}: {
  text: string;
  running: boolean;
  /** 完成后显示的思考时长（毫秒） */
  durationMs?: number;
  defaultExpanded?: boolean;
  onToggleExpanded?: (next: boolean) => void;
}) {
  const [expanded, setExpanded] = useState(defaultExpanded ?? false);
  const summary = running ? latestLine(text) : firstLine(text);
  const toggle = () => {
    const next = !expanded;
    setExpanded(next);
    onToggleExpanded?.(next);
  };
  return (
    <FlowRow
      icon={<Sparkles size={14} />}
      title={running ? '思考中…' : '思考'}
      summary={summary}
      followEnd={running}
      open={expanded}
      expandable
      onToggle={toggle}
      running={running}
    >
      <div className="dsh-thinkbody">{text}</div>
      {!running && durationMs ? (
        <div className="dsh-thinkbody" style={{ paddingTop: 0, opacity: 0.7 }}>
          持续了 {Math.max(1, Math.round(durationMs / 1000))} 秒
        </div>
      ) : null}
    </FlowRow>
  );
}

/* ============ 工具行（ToolRow） ============ */

export interface ToolCallEntryLike {
  id?: string;
  name: string;
  input?: any;
  result?: any;
  status?: 'running' | 'done' | 'error' | 'canceled';
  /** 该工具被调用时模型已输出的完整正文（用于推算中间叙述文本的分界） */
  textBefore?: string;
}

type Variant = 'read' | 'write' | 'edit' | 'bash' | 'search' | 'code' | 'web' | 'ask' | 'computer' | 'others';

const TOOL_VARIANTS: Record<string, Variant> = {
  Read: 'read', Write: 'write', Edit: 'edit', MultiEdit: 'edit',
  Bash: 'bash', Grep: 'search', Glob: 'search', ListDir: 'search',
  WebFetch: 'web', WebSearch: 'web',
  AskUserQuestion: 'ask', computer_use: 'computer', browser_use: 'computer',
};

const TOOL_TITLES: Record<string, string> = {
  Read: '读取', Write: '写入', Edit: '编辑', MultiEdit: '编辑',
  Bash: '终端', Grep: '搜索', Glob: '查找文件', ListDir: '列出目录',
  WebFetch: '抓取网页', WebSearch: '搜索网页',
  AskUserQuestion: '询问用户', computer_use: '电脑操作', browser_use: '浏览器',
};

const VARIANT_ICONS: Record<Variant, React.ReactNode> = {
  search: <Search size={14} />,
  read: <FileText size={14} />,
  bash: <Terminal size={14} />,
  write: <FilePlus size={14} />,
  edit: <Pencil size={14} />,
  code: <Code2 size={14} />,
  web: <Globe size={14} />,
  ask: <HelpCircle size={14} />,
  computer: <Monitor size={14} />,
  others: <Sparkles size={14} />,
};

function basename(p: string): string {
  return p.split(/[/\\]/).filter(Boolean).pop() || p;
}

function getSummary(tc: ToolCallEntryLike): { text: string; filePath?: string } {
  const input = tc.input || {};
  switch (tc.name) {
    case 'Read': case 'Write': case 'Edit': case 'MultiEdit':
      return { text: String(input.file_path || ''), filePath: input.file_path ? String(input.file_path) : undefined };
    case 'Bash':
      return { text: String(input.command || input.description || '') };
    case 'Grep':
      return { text: [input.pattern, input.path ? `(${basename(String(input.path))})` : ''].filter(Boolean).join(' ') };
    case 'Glob':
      return { text: String(input.pattern || '') };
    case 'ListDir':
      return { text: String(input.path || '.') };
    case 'WebFetch':
      return { text: String(input.url || '') };
    case 'WebSearch':
      return { text: String(input.query || '') };
    case 'AskUserQuestion':
      return { text: (Array.isArray(input.questions) ? input.questions.map((q: any) => q?.question).filter(Boolean).join(' / ') : String(input.question || '')) };
    case 'computer_use': case 'browser_use':
      return { text: String(input.action || input.url || input.target || '') };
    default: {
      // 未知工具：取第一个字符串参数作摘要
      const first = Object.values(input).find((v) => typeof v === 'string' && v) as string | undefined;
      return { text: first ? String(first).slice(0, 120) : '' };
    }
  }
}

function resultText(result: any): string | null {
  if (result == null || result === '') return null;
  if (typeof result === 'string') return result;
  try { return JSON.stringify(result, null, 2); } catch { return String(result); }
}

function firstLineOf(text: string): string {
  return firstLine(text.trim());
}

function diffTotals(text: string): { added: number; removed: number } | null {
  let added = 0, removed = 0;
  for (const line of text.split('\n')) {
    if (line.startsWith('+') && !line.startsWith('+++')) added++;
    if (line.startsWith('-') && !line.startsWith('---')) removed++;
  }
  return added > 0 || removed > 0 ? { added, removed } : null;
}

/** 头尾截断：首尾保留、中段折叠（head-tail-cap 等价物） */
function HeadTailText({ text, maxLines = 8, mono = true }: { text: string; maxLines?: number; mono?: boolean }) {
  const [showAll, setShowAll] = useState(false);
  const lines = useMemo(() => text.split('\n'), [text]);
  if (lines.length <= maxLines || showAll) {
    return <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }} className={mono ? '' : 'font-sans'}>{text}</pre>;
  }
  const head = Math.ceil(maxLines / 2);
  const tail = maxLines - head;
  const hidden = lines.length - head - tail;
  return (
    <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }} className={mono ? '' : 'font-sans'}>
      {lines.slice(0, head).join('\n')}
      <span className="dsh-cap-skip" onClick={(e) => { e.stopPropagation(); setShowAll(true); }}>
        … 展开其余 {hidden} 行 …
      </span>
      {lines.slice(lines.length - tail).join('\n')}
    </pre>
  );
}

export function ToolRow({ toolCall: tc }: { toolCall: ToolCallEntryLike }) {
  const [expanded, setExpanded] = useState(false);
  const variant = TOOL_VARIANTS[tc.name] || 'others';
  const title = TOOL_TITLES[tc.name] || tc.name;
  const state = tc.status === 'running' ? 'running'
    : tc.status === 'error' ? 'error'
      : tc.status === 'canceled' ? 'stopped' : 'ok';
  const isRunning = state === 'running';
  const isError = state === 'error';

  const { text: rawSummary, filePath } = getSummary(tc);
  const output = resultText(tc.result);
  const expandable = !isRunning && output !== null;
  const errorSummary = isError && output ? firstLineOf(output) : null;
  // 错误行摘要被失败首行替换；终端行摘要用描述；其余用主参数
  const summaryText = errorSummary ?? rawSummary;
  const diffStat = useMemo(
    () => (variant === 'edit' || variant === 'write') && output && !isRunning ? diffTotals(output) : null,
    [variant, output, isRunning],
  );

  // 展开体渲染：按变体分派
  const renderBody = () => {
    if (output === null) return null;
    if (variant === 'bash') {
      // 终端卡：解析 JSON stdout/stderr/exit_code
      let stdout = output, stderr = '', exitCode: number | undefined;
      try {
        const data = JSON.parse(output);
        stdout = data.stdout || '';
        stderr = data.stderr || '';
        exitCode = data.exit_code;
      } catch { /* 纯文本输出 */ }
      return (
        <div className="dsh-terminalbody">
          {stdout && <pre>{stdout}</pre>}
          {stderr && <pre style={{ color: isError ? 'var(--dsh-state-error)' : 'var(--dsh-state-warn)' }}>{stderr}</pre>}
          {exitCode !== undefined && exitCode !== 0 && (
            <div style={{ padding: '4px 14px', fontSize: 11, fontFamily: 'ui-monospace, monospace', color: 'var(--dsh-state-error)', borderTop: '0.5px solid var(--dsh-border-l1)' }}>
              退出码 {exitCode}
            </div>
          )}
          {!stdout && !stderr && exitCode === undefined && <pre>(no output)</pre>}
        </div>
      );
    }
    if ((variant === 'edit' || variant === 'write') && diffStat) {
      const lines = output.split('\n');
      return (
        <div className="dsh-diffbody">
          <HeadTailText text={lines.map((l) => l).join('\n')} maxLines={8} />
          <div style={{ padding: '4px 14px', fontSize: 11, fontFamily: 'ui-monospace, monospace', color: 'var(--dsh-label-caption)', borderTop: '0.5px solid var(--dsh-border-l1)' }}>
            +{diffStat.added} -{diffStat.removed}
          </div>
        </div>
      );
    }
    if (variant === 'read') {
      return (
        <div className="dsh-terminalbody">
          <div style={{ padding: '4px 14px', fontSize: 11, color: 'var(--dsh-label-secondary)', borderBottom: '0.5px solid var(--dsh-border-l1)', display: 'flex', gap: 6, alignItems: 'center' }}>
            <FileText size={11} />
            <span style={{ fontFamily: 'ui-monospace, monospace', overflow: 'hidden', textOverflow: 'ellipsis' }}>{filePath || ''}</span>
          </div>
          <pre>{output}</pre>
        </div>
      );
    }
    // 通用 IN/OUT 卡
    return (
      <div className="dsh-iocard">
        {rawSummary && (
          <div className="dsh-iosection">
            <span className="dsh-iolabel">输入</span>
            <span className="dsh-iotext">{rawSummary}</span>
          </div>
        )}
        {rawSummary && <span className="dsh-iodivider" aria-hidden />}
        <div className="dsh-iosection">
          <span className="dsh-iolabel">输出</span>
          <span className="dsh-iotext" data-error={isError || undefined}>
            <HeadTailText text={output} maxLines={8} />
          </span>
        </div>
      </div>
    );
  };

  return (
    <FlowRow
      icon={VARIANT_ICONS[variant]}
      title={title}
      summary={summaryText === '' ? undefined : summaryText}
      summaryError={errorSummary !== null}
      suffix={diffStat ? `+${diffStat.added} -${diffStat.removed}` : undefined}
      open={expanded && expandable}
      expandable={expandable}
      onToggle={() => setExpanded((v) => !v)}
      running={isRunning}
      error={isError}
      stopped={state === 'stopped'}
    >
      {renderBody()}
    </FlowRow>
  );
}

/** 工具行之间交错的"中间叙述文本"节点（仿 dsh 的 text node 流程区渲染） */
function ProcessText({ text }: { text: string }) {
  return (
    <div className="dsh-process-text">
      {text}
    </div>
  );
}

/** 消息内全部工具行列表：按 textBefore 推算并在工具行之间插入叙述文本 */
export function ToolRowList({ toolCalls, fullText }: { toolCalls: ToolCallEntryLike[]; fullText?: string }) {
  if (!toolCalls || toolCalls.length === 0) return null;

  // 计算每个工具行之前的叙述文本段（该工具 textBefore 减去上一个工具 textBefore 的增量）
  const items: React.ReactNode[] = [];
  let prevOffset = 0;
  const text = fullText || '';

  toolCalls.forEach((tc, i) => {
    const tbLen = (tc.textBefore || '').length;
    if (text && tbLen > prevOffset) {
      const seg = text.slice(prevOffset, tbLen).trim();
      if (seg) items.push(<ProcessText key={`proc-text-${i}`} text={seg} />);
    }
    items.push(<ToolRow key={tc.id || i} toolCall={tc} />);
    prevOffset = tbLen;
  });

  return (
    <div className="my-1.5 space-y-0.5">
      {items}
    </div>
  );
}
