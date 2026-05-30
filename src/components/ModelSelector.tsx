import React, { useState, useEffect, useRef } from 'react';
import { ChevronDown, Check } from 'lucide-react';

export interface SelectableModel {
  id: string;
  name: string;
  enabled: number;
  description?: string;
  tier?: 'opus' | 'sonnet' | 'haiku' | 'extra';
}

function getChatModelMap(): Map<string, { thinkingId?: string }> {
  try {
    const models = JSON.parse(localStorage.getItem('chat_models') || '[]');
    const map = new Map<string, { thinkingId?: string }>();
    for (const m of models) {
      map.set(m.id, { thinkingId: m.thinkingId });
      if (m.thinkingId) map.set(m.thinkingId, { thinkingId: m.thinkingId });
    }
    return map;
  } catch { return new Map(); }
}

function stripThinking(modelStr: string) {
  const map = getChatModelMap();
  for (const [baseId, cfg] of map) {
    if (cfg.thinkingId === modelStr) return baseId;
  }
  return (modelStr || '').replace(/-thinking$/, '');
}

function withThinking(base: string, thinking: boolean) {
  if (!thinking) return base;
  const map = getChatModelMap();
  const cfg = map.get(base);
  if (cfg?.thinkingId) return cfg.thinkingId;
  return `${base}-thinking`;
}

function isThinking(modelStr: string) {
  const map = getChatModelMap();
  for (const [, cfg] of map) {
    if (cfg.thinkingId === modelStr) return true;
  }
  return typeof modelStr === 'string' && modelStr.endsWith('-thinking');
}

function hasThinkingVariant(_modelId: string): boolean {
  return true;
}

function prettifyModelName(name?: string, id?: string): string {
  for (const candidate of [id, name]) {
    if (!candidate) continue;
    const m = candidate.match(/(opus|sonnet|haiku)-(\d+)-(\d+)/i);
    if (m) {
      const tier = m[1][0].toUpperCase() + m[1].slice(1).toLowerCase();
      return `${tier} ${m[2]}.${m[3]}`;
    }
  }
  const raw = name || id || '';
  if (!raw) return 'Model';
  const lastSlash = raw.lastIndexOf('/');
  if (lastSlash >= 0) {
    const suffix = raw.slice(lastSlash + 1);
    if (suffix.length <= 5 && !suffix.includes('-')) {
      return raw.replace(/-/g, ' ');
    }
    return suffix.replace(/-/g, ' ');
  }
  return raw.replace(/-/g, ' ');
}

const EFFORT_LEVELS = [
  { id: 'low', label: 'Low' },
  { id: 'medium', label: 'Medium' },
  { id: 'high', label: 'High' },
  { id: 'xhigh', label: 'Max' },
];

interface ModelSelectorProps {
  currentModelString: string;
  models: SelectableModel[];
  onModelChange: (newModelString: string) => void;
  isNewChat?: boolean;
  dropdownPosition?: 'top' | 'bottom';
  effort?: string;
  onEffortChange?: (effort: string) => void;
}

const ModelSelector: React.FC<ModelSelectorProps> = ({
  currentModelString,
  models,
  onModelChange,
  dropdownPosition,
  effort = 'medium',
  onEffortChange,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const [dropUp, setDropUp] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const currentBase = stripThinking(currentModelString);
  const thinking = isThinking(currentModelString);
  const currentModel = models.find(m => m && m.id === currentBase);
  const currentLabel = prettifyModelName(currentModel?.name, currentModel?.id || currentBase);

  const mainModels = models.filter(m => m && m.tier !== 'extra');
  const currentHasThinking = hasThinkingVariant(currentBase);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleToggleOpen = () => {
    if (!isOpen && containerRef.current) {
      const rect = containerRef.current.getBoundingClientRect();
      const spaceBelow = window.innerHeight - rect.bottom;
      setDropUp(dropdownPosition === 'top' ? true : (dropdownPosition === 'bottom' ? false : spaceBelow < 380));
    }
    setIsOpen(!isOpen);
  };

  const handleSelectModel = (baseId: string, enabled: number) => {
    if (!enabled) return;
    const targetHasThinking = hasThinkingVariant(baseId);
    onModelChange(withThinking(baseId, targetHasThinking ? thinking : false));
    setIsOpen(false);
  };

  const handleToggleThinking = () => {
    if (!currentHasThinking) return;
    onModelChange(withThinking(currentBase, !thinking));
  };

  const currentEffortLabel = EFFORT_LEVELS.find(e => e.id === effort)?.label || 'Medium';

  const renderModelItem = (m: SelectableModel) => {
    const active = currentBase === m.id;
    const disabled = Number(m.enabled) !== 1;
    return (
      <button
        key={m.id || Math.random()}
        onClick={() => handleSelectModel(m.id, m.enabled)}
        disabled={disabled}
        className={`w-full px-4 ${m.description ? 'py-2.5' : 'py-2'} flex items-center justify-between text-left ${disabled ? 'opacity-45 cursor-not-allowed' : 'hover:bg-claude-hover cursor-pointer'}`}
      >
        <div className="flex-1 min-w-0">
          <div className="text-[14.5px] font-[500] text-claude-text truncate">{prettifyModelName(m.name, m.id)}</div>
          {m.description && <div className="text-[12.5px] text-claude-textSecondary mt-0.5">{m.description}</div>}
        </div>
        {active && <Check size={18} className="text-[#3b82f6] ml-2 shrink-0" />}
      </button>
    );
  };

  return (
    <div className="relative inline-block text-right" ref={containerRef}>
      <button
        onClick={handleToggleOpen}
        className="flex items-center gap-1.5 text-[15px] font-medium text-claude-text hover:bg-claude-hover px-3 py-2 rounded-md transition-colors"
      >
        <span>{currentLabel}</span>
        {thinking && <span className="text-claude-textSecondary font-normal">Extended</span>}
        <ChevronDown size={14} className="text-claude-textSecondary" />
      </button>

      {isOpen && (
        <div className={`absolute ${dropUp ? 'bottom-full mb-2' : 'top-full mt-2'} right-0 w-[260px] bg-claude-input rounded-xl shadow-xl border border-claude-border z-50 overflow-hidden py-1 text-left`}>
          {/* Main tier models */}
          {mainModels.map(renderModelItem)}

          {/* Extended thinking toggle */}
          <div className="h-[1px] bg-claude-border my-1 mx-4" />
          <div className={`px-4 py-2 flex items-center justify-between text-left select-none ${currentHasThinking ? 'hover:bg-claude-hover cursor-pointer' : ''}`}>
            <div className="flex-1">
              <div className={`text-[14.5px] font-[500] ${currentHasThinking ? 'text-claude-text' : 'text-claude-textSecondary/50'}`}>Extended thinking</div>
              <div className={`text-[12.5px] mt-0.5 ${currentHasThinking ? 'text-claude-textSecondary' : 'text-claude-textSecondary/30'}`}>Think longer for complex tasks</div>
            </div>
            <button
              onClick={(e) => {
                e.stopPropagation();
                handleToggleThinking();
              }}
              disabled={!currentHasThinking}
              className={`w-10 h-6 rounded-full relative transition-colors duration-200 ${!currentHasThinking ? 'bg-claude-border/50 cursor-not-allowed' : thinking ? 'bg-[#3A6FE0]' : 'bg-claude-border cursor-pointer'}`}
            >
              <div className={`absolute top-1 w-4 h-4 rounded-full bg-white shadow-sm transition-transform duration-200 ${thinking && currentHasThinking ? 'left-5' : 'left-1'}`} />
            </button>
          </div>

          {/* Effort level */}
          <div className="h-[1px] bg-claude-border my-1 mx-4" />
          <div className="px-4 py-2">
            <div className="text-[14.5px] font-[500] text-claude-text mb-2">Reasoning Effort</div>
            <div className="flex gap-1">
              {EFFORT_LEVELS.map(level => (
                <button
                  key={level.id}
                  onClick={(e) => {
                    e.stopPropagation();
                    onEffortChange?.(level.id);
                  }}
                  className={`flex-1 px-1.5 py-1 text-[12px] font-medium rounded-md transition-colors ${
                    effort === level.id
                      ? 'bg-[#3A6FE0] text-white'
                      : 'bg-claude-border/40 text-claude-textSecondary hover:bg-claude-border'
                  }`}
                >
                  {level.label}
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default ModelSelector;