import React, { useEffect, useRef, useState, useCallback } from 'react';
import {
  ArrowLeft,
  ArrowRight,
  RefreshCw,
  ExternalLink,
  X,
  ImageOff,
  Home,
} from 'lucide-react';
import { apiBaseSync } from '../api';

interface BrowserLivePanelProps {
  onClose?: () => void;
  className?: string;
}

/**
 * 侧边栏内嵌真实浏览器面板：
 * - 实时画面 = 后端 headless Edge 的 CDP 截图流（画面即模型所见）。
 * - 人工可直接操作：点击/双击/滚轮/键盘事件按比例换算坐标后转发给真实页面。
 * - 地址栏可输入网址或关键词回车导航（后端自动补全协议或转搜索）。
 */
const BrowserLivePanel: React.FC<BrowserLivePanelProps> = ({ onClose, className = '' }) => {
  const [imgSrc, setImgSrc] = useState<string | null>(null);
  const [url, setUrl] = useState<string>('');
  const [addressInput, setAddressInput] = useState<string>('');
  const [navigating, setNavigating] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const imgRef = useRef<HTMLImageElement | null>(null);
  // 地址栏正在编辑时暂停自动回填，避免打字被打断
  const editingRef = useRef(false);

  const refresh = async () => {
    try {
      const res = await fetch(`${apiBaseSync()}/browser/view`);
      const data = await res.json();
      if (data?.success && data.data) {
        setImgSrc(`data:image/png;base64,${data.data}`);
        if (data.url) {
          setUrl(data.url);
          if (!editingRef.current) setAddressInput(data.url);
        }
        setError(null);
      } else if (data?.error) {
        setError(data.error);
      }
    } catch (e: any) {
      setError(e?.message || '无法连接浏览器会话');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setLoading(true);
    refresh();
    timerRef.current = setInterval(refresh, 1200);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const navigate = async (raw: string) => {
    const target = raw.trim();
    if (!target) return;
    setNavigating(true);
    try {
      const res = await fetch(`${apiBaseSync()}/browser/navigate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url: target }),
      });
      const data = await res.json();
      if (data?.url && !editingRef.current) setAddressInput(data.url);
      setTimeout(refresh, 400);
    } catch (e: any) {
      setError(e?.message || '导航失败');
    } finally {
      setNavigating(false);
    }
  };

  const goHome = () => navigate('home://start');
  const goBack = () => navigate('__back__');
  const goForward = () => navigate('__forward__');

  /** 把画面上的鼠标事件坐标换算成页面真实坐标（截图 1280 宽，画面可能被缩放显示） */
  const pagePoint = (e: React.MouseEvent) => {
    const el = imgRef.current;
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    const natural = el.naturalWidth || 1280;
    const scale = natural / rect.width;
    return {
      x: Math.round((e.clientX - rect.left) * scale),
      y: Math.round((e.clientY - rect.top) * scale),
    };
  };

  const interact = useCallback(async (payload: Record<string, unknown>) => {
    try {
      await fetch(`${apiBaseSync()}/browser/interact`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      setTimeout(refresh, 250);
    } catch {}
  }, []);

  const onClick = (e: React.MouseEvent) => {
    const p = pagePoint(e);
    if (p) interact({ action: 'click', x: p.x, y: p.y });
  };
  const onDoubleClick = (e: React.MouseEvent) => {
    const p = pagePoint(e);
    if (p) interact({ action: 'dblclick', x: p.x, y: p.y });
  };
  const onWheel = (e: React.WheelEvent) => {
    const p = pagePoint(e);
    if (p) interact({ action: 'wheel', x: p.x, y: p.y, dx: Math.round(e.deltaX), dy: Math.round(e.deltaY) });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    // 仅在画面容器聚焦时转发按键；修饰键单独按不转发
    const k = e.key;
    if (k === 'Control' || k === 'Shift' || k === 'Alt' || k === 'Meta') return;
    e.preventDefault();
    if (k.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
      interact({ action: 'type', text: k });
    } else {
      const nameMap: Record<string, string> = {
        ArrowUp: 'ArrowUp', ArrowDown: 'ArrowDown', ArrowLeft: 'ArrowLeft', ArrowRight: 'ArrowRight',
        Enter: 'Enter', Backspace: 'Backspace', Delete: 'Delete', Tab: 'Tab',
        Escape: 'Escape', Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown',
      };
      const mapped = nameMap[k] || k;
      interact({ action: 'key', key: mapped });
    }
  };

  return (
    <div className={`flex flex-col bg-claude-bg border border-claude-border rounded-xl overflow-hidden ${className}`}>
      {/* 工具栏 */}
      <div className="flex items-center gap-1 p-2 border-b border-claude-border bg-claude-input/30">
        <button
          onClick={goBack}
          className="p-1.5 rounded-lg text-claude-textSecondary hover:bg-claude-hover"
          title="后退"
        >
          <ArrowLeft size={16} />
        </button>
        <button
          onClick={goForward}
          className="p-1.5 rounded-lg text-claude-textSecondary hover:bg-claude-hover"
          title="前进"
        >
          <ArrowRight size={16} />
        </button>
        <button
          onClick={goHome}
          className="p-1.5 rounded-lg text-claude-textSecondary hover:bg-claude-hover"
          title="起始页"
        >
          <Home size={16} />
        </button>
        <button
          onClick={() => { setLoading(true); refresh(); }}
          className={`p-1.5 rounded-lg text-claude-textSecondary hover:bg-claude-hover ${loading ? 'animate-spin' : ''}`}
          title="刷新"
        >
          <RefreshCw size={16} />
        </button>

        <input
          value={addressInput}
          onChange={(e) => setAddressInput(e.target.value)}
          onFocus={() => { editingRef.current = true; }}
          onBlur={() => { editingRef.current = false; }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              const v = (e.target as HTMLInputElement).value;
              editingRef.current = false;
              (e.target as HTMLInputElement).blur();
              navigate(v);
            }
            e.stopPropagation();
          }}
          placeholder="输入网址或搜索词，回车访问"
          spellCheck={false}
          className="flex-1 mx-2 px-3 py-1.5 bg-claude-input border border-claude-border rounded-lg text-xs text-claude-text focus:outline-none focus:ring-1 focus:ring-blue-400/50 placeholder:text-claude-textSecondary/60"
          title={url}
        />

        <a
          href={url && url !== 'home://start' ? url : 'about:blank'}
          target="_blank"
          rel="noopener noreferrer"
          className="p-1.5 rounded-lg text-claude-textSecondary hover:bg-claude-hover"
          title="在新窗口打开"
        >
          <ExternalLink size={16} />
        </a>
        {onClose && (
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-claude-textSecondary hover:bg-red-500/20 hover:text-red-400"
            title="关闭"
          >
            <X size={16} />
          </button>
        )}
      </div>

      {/* 实时画面：可点击/滚轮/键盘，坐标按比例转发给真实页面 */}
      <div className="flex-1 relative bg-white overflow-auto">
        {imgSrc ? (
          <img
            ref={imgRef}
            src={imgSrc}
            alt="浏览器实时画面"
            className="w-full h-auto cursor-pointer select-none"
            style={{ imageRendering: 'auto', background: '#fff' }}
            draggable={false}
            onClick={onClick}
            onDoubleClick={onDoubleClick}
            onWheel={onWheel}
            tabIndex={0}
            onKeyDown={onKeyDown}
          />
        ) : (
          <div className="absolute inset-0 flex items-center justify-center bg-white">
            <div className="text-center px-6">
              {error ? (
                <>
                  <ImageOff size={40} className="mx-auto text-gray-300 mb-3" />
                  <p className="text-sm text-gray-400">{error}</p>
                </>
              ) : navigating ? (
                <p className="text-sm text-gray-400">正在导航…</p>
              ) : (
                <p className="text-sm text-gray-400">等待浏览器画面…</p>
              )}
            </div>
          </div>
        )}
      </div>

      {/* 状态条 */}
      <div className="flex items-center justify-between px-3 py-1 border-t border-claude-border bg-claude-input/20 text-[10px] text-claude-textSecondary">
        <span>画面可直接点击 / 滚动 / 键盘输入，与模型所见实时同步</span>
        <span className="truncate max-w-[50%]" title={url}>{url}</span>
      </div>
    </div>
  );
};

export default BrowserLivePanel;
