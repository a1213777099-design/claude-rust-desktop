import React, { useEffect, useState } from 'react';
import { Loader2, CheckCircle2, XCircle, Clock } from 'lucide-react';

export interface ExecutionStep {
  label: string;
  status: 'pending' | 'running' | 'done' | 'error';
  detail?: string;
}

interface SkillExecutionIndicatorProps {
  skillName: string;
  steps: ExecutionStep[];
  elapsedMs: number;
  isFinished: boolean;
  hasError: boolean;
  errorMessage?: string;
  onCancel?: () => void;
}

const StepIcon: React.FC<{ status: ExecutionStep['status'] }> = ({ status }) => {
  switch (status) {
    case 'running':
      return <Loader2 size={14} className="text-blue-400 animate-spin" />;
    case 'done':
      return <CheckCircle2 size={14} className="text-green-500" />;
    case 'error':
      return <XCircle size={14} className="text-red-500" />;
    default:
      return <Clock size={14} className="text-gray-400" />;
  }
};

const SkillExecutionIndicator: React.FC<SkillExecutionIndicatorProps> = ({
  skillName,
  steps,
  elapsedMs,
  isFinished,
  hasError,
  errorMessage,
  onCancel,
}) => {
  const [elapsedDisplay, setElapsedDisplay] = useState('0.0s');

  useEffect(() => {
    if (isFinished) {
      setElapsedDisplay(`${(elapsedMs / 1000).toFixed(1)}s`);
      return;
    }
    const interval = setInterval(() => {
      const ms = Date.now() - (Date.now() - elapsedMs + 100);
      setElapsedDisplay(`${(ms / 1000).toFixed(1)}s`);
    }, 100);
    return () => clearInterval(interval);
  }, [elapsedMs, isFinished]);

  return (
    <div className={`border rounded-xl px-4 py-3 my-2 transition-colors ${
      hasError ? 'border-red-200 bg-red-50/30' : isFinished ? 'border-green-200 bg-green-50/20' : 'border-claude-border bg-claude-hover/20'
    }`}>
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          {isFinished ? (
            hasError ? (
              <XCircle size={16} className="text-red-500" />
            ) : (
              <CheckCircle2 size={16} className="text-green-500" />
            )
          ) : (
            <Loader2 size={16} className="text-blue-400 animate-spin" />
          )}
          <span className="text-[14px] font-medium text-claude-text">
            {isFinished
              ? hasError
                ? `Skill "${skillName}" failed`
                : `Skill "${skillName}" completed`
              : `Executing skill: "${skillName}"`}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-claude-textSecondary font-mono">
            {elapsedDisplay}
          </span>
          {!isFinished && onCancel && (
            <button
              onClick={onCancel}
              className="text-[11px] px-2 py-0.5 rounded-md text-red-500 hover:bg-red-50 transition-colors"
            >
              Cancel
            </button>
          )}
        </div>
      </div>

      {/* Steps */}
      {steps.length > 0 && (
        <div className="space-y-1 ml-1">
          {steps.map((step, idx) => (
            <div key={idx} className="flex items-center gap-2 text-[12px]">
              <StepIcon status={step.status} />
              <span className={
                step.status === 'done' ? 'text-green-700 dark:text-green-400' :
                step.status === 'error' ? 'text-red-600' :
                step.status === 'running' ? 'text-blue-600 dark:text-blue-400' :
                'text-claude-textSecondary'
              }>
                {step.label}
              </span>
              {step.detail && (
                <span className="text-claude-textSecondary/70 truncate max-w-[200px]">
                  — {step.detail}
                </span>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Error message */}
      {hasError && errorMessage && (
        <div className="mt-2 px-3 py-1.5 bg-red-50 text-red-600 rounded-lg text-[12px] leading-relaxed">
          {errorMessage}
        </div>
      )}
    </div>
  );
};

export default SkillExecutionIndicator;
