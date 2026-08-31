import React, { useMemo } from 'react';
import { X, CheckCircle2, XCircle, Clock, FileText, Wrench, Terminal, Copy, Check } from 'lucide-react';
import { SkillExecuteResult } from '../api';

interface SkillExecutionResultModalProps {
  skillName: string;
  result: SkillExecuteResult;
  onClose: () => void;
  onApplyToChat?: (content: string) => void;
  onReExecute?: () => void;
}

const OutputIcon: React.FC<{ type: string }> = ({ type }) => {
  switch (type) {
    case 'text':
      return <FileText size={14} className="text-blue-400" />;
    case 'tool_call':
      return <Wrench size={14} className="text-amber-400" />;
    case 'mcp_call':
      return <Terminal size={14} className="text-purple-400" />;
    default:
      return <FileText size={14} className="text-gray-400" />;
  }
};

function formatDuration(ms?: number): string {
  if (ms == null) return '';
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function truncateContent(content: string, maxLen = 200): string {
  if (content.length <= maxLen) return content;
  return content.slice(0, maxLen) + '...';
}

const SkillExecutionResultModal: React.FC<SkillExecutionResultModalProps> = ({
  skillName,
  result,
  onClose,
  onApplyToChat,
  onReExecute,
}) => {
  const [copied, setCopied] = React.useState(false);

  // 后端 SkillExecutionResult 只有 summary + results(tool 调用记录)，映射为展示结构
  const summaryText = useMemo(() => {
    return result.result?.summary || '';
  }, [result]);

  const outputs = useMemo(() => {
    return (result.result?.results || []).map((r) => ({
      type: 'tool_call' as const,
      tool_name: r.tool_name,
      content: typeof r.output === 'string' ? r.output : JSON.stringify(r.output, null, 2),
      status: (r.error ? 'error' : 'done') as 'error' | 'done',
      duration_ms: undefined as number | undefined,
    }));
  }, [result]);

  const handleCopySummary = async () => {
    try {
      await navigator.clipboard.writeText(summaryText || JSON.stringify(result, null, 2));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {}
  };

  const hasError = !result.success || !!result.error;
  const toolCallOutputs = outputs;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40 backdrop-blur-sm">
      <div
        className="bg-white dark:bg-[#1C1C1C] rounded-2xl shadow-2xl border border-claude-border w-full max-w-2xl max-h-[80vh] flex flex-col mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-claude-border">
          <div className="flex items-center gap-3">
            {hasError ? (
              <XCircle size={22} className="text-red-500" />
            ) : (
              <CheckCircle2 size={22} className="text-green-500" />
            )}
            <div>
              <h3 className="text-[16px] font-medium text-claude-text">
                {hasError ? `Skill "${skillName}" Failed` : `Skill "${skillName}" Completed`}
              </h3>
              <p className="text-[12px] text-claude-textSecondary">
                {hasError ? result.error || 'Unknown error occurred' : 'Execution finished successfully'}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 hover:bg-claude-hover rounded-lg transition-colors text-claude-textSecondary"
          >
            <X size={18} />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
          {/* Summary */}
          {summaryText && (
            <div>
              <div className="flex items-center justify-between mb-2">
                <h4 className="text-[13px] font-medium text-claude-textSecondary uppercase tracking-wide">Summary</h4>
                <button
                  onClick={handleCopySummary}
                  className="flex items-center gap-1 text-[11px] text-claude-textSecondary hover:text-claude-text transition-colors"
                >
                  {copied ? <Check size={12} /> : <Copy size={12} />}
                  {copied ? 'Copied' : 'Copy'}
                </button>
              </div>
              <div className="bg-claude-hover/30 rounded-xl p-4 text-[14px] text-claude-text whitespace-pre-wrap leading-relaxed">
                {summaryText}
              </div>
            </div>
          )}

          {/* Tool/MCP Call Outputs */}
          {toolCallOutputs.length > 0 && (
            <div>
              <h4 className="text-[13px] font-medium text-claude-textSecondary uppercase tracking-wide mb-2">
                Tool Calls ({toolCallOutputs.length})
              </h4>
              <div className="space-y-2">
                {toolCallOutputs.map((output, idx) => (
                  <div
                    key={idx}
                    className="flex items-center gap-2 px-3 py-2 bg-claude-hover/20 rounded-lg text-[13px]"
                  >
                    <OutputIcon type={output.type} />
                    <span className="font-mono text-claude-text flex-1">
                      {output.tool_name || 'tool'} — {truncateContent(output.content, 100)}
                    </span>
                    {output.duration_ms != null && (
                      <span className="text-[11px] text-claude-textSecondary">
                        {formatDuration(output.duration_ms)}
                      </span>
                    )}
                    {output.status && (
                      <span className={`text-[11px] ${
                        output.status === 'done' ? 'text-green-500' :
                        output.status === 'error' ? 'text-red-500' : 'text-gray-400'
                      }`}>
                        {output.status}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Raw Error */}
          {hasError && result.error && (
            <div className="px-3 py-2 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-xl text-[13px] text-red-700 dark:text-red-400 font-mono whitespace-pre-wrap">
              {result.error}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 px-6 py-4 border-t border-claude-border">
          {summaryText && onApplyToChat && (
            <button
              onClick={() => onApplyToChat(summaryText)}
              className="px-4 py-2 rounded-lg text-[13px] font-medium bg-claude-hover hover:bg-claude-btn-hover text-claude-text transition-colors"
            >
              Apply to chat
            </button>
          )}
          {onReExecute && (
            <button
              onClick={onReExecute}
              className="px-4 py-2 rounded-lg text-[13px] font-medium bg-claude-hover hover:bg-claude-btn-hover text-claude-text transition-colors"
            >
              Re-execute
            </button>
          )}
          <button
            onClick={onClose}
            className="px-4 py-2 rounded-lg text-[13px] font-medium bg-[#D97757] hover:bg-[#C56A4F] text-white transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};

export default SkillExecutionResultModal;
