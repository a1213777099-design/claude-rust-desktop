import React, { useState, useEffect } from 'react';
import { X, FileText, Loader2, RefreshCw, Bell, BellOff, Copy, Check, ChevronDown, ChevronRight } from 'lucide-react';
import { McpResourceContentResult, mcpResourceRead, mcpResourceMonitor } from '../api';

interface ResourceViewerPanelProps {
  serverName: string;
  resourceUri: string;
  resourceName: string;
  mimeType?: string;
  onClose: () => void;
}

const ResourceViewerPanel: React.FC<ResourceViewerPanelProps> = ({
  serverName,
  resourceUri,
  resourceName,
  mimeType,
  onClose,
}) => {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [data, setData] = useState<McpResourceContentResult | null>(null);
  const [monitoring, setMonitoring] = useState(false);
  const [monitorLoading, setMonitorLoading] = useState(false);
  const [copied, setCopied] = useState(false);
  const [showMeta, setShowMeta] = useState(false);
  const [offset, setOffset] = useState(0);
  const [limit, setLimit] = useState(100);

  useEffect(() => {
    loadResource();
  }, [resourceUri]);

  const loadResource = async (newOffset?: number, newLimit?: number) => {
    setLoading(true);
    setError(null);
    try {
      const result = await mcpResourceRead(serverName, resourceUri, {
        offset: newOffset ?? offset,
        limit: newLimit ?? limit,
      });
      setData(result);
    } catch (e: any) {
      setError(e.message || 'Failed to load resource');
    } finally {
      setLoading(false);
    }
  };

  const handleToggleMonitor = async () => {
    setMonitorLoading(true);
    try {
      const result = await mcpResourceMonitor(serverName, resourceUri, !monitoring);
      setMonitoring(result.enabled);
    } catch (e: any) {
      console.error('Failed to toggle monitoring:', e);
    } finally {
      setMonitorLoading(false);
    }
  };

  const handleCopyContent = async () => {
    if (!data?.content) return;
    try {
      await navigator.clipboard.writeText(data.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {}
  };

  const handleReload = () => {
    loadResource();
  };

  const contentLength = data?.content?.length || 0;
  const isText = !mimeType || mimeType.startsWith('text/') || mimeType.includes('json') || mimeType.includes('yaml') || mimeType.includes('markdown');

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40 backdrop-blur-sm">
      <div
        className="bg-white dark:bg-[#1C1C1C] rounded-2xl shadow-2xl border border-claude-border w-full max-w-3xl max-h-[85vh] flex flex-col mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-claude-border">
          <div className="flex items-center gap-3 min-w-0">
            <FileText size={20} className="text-blue-400 flex-shrink-0" />
            <div className="min-w-0">
              <h3 className="text-[16px] font-medium text-claude-text truncate">{resourceName}</h3>
              <p className="text-[12px] text-claude-textSecondary truncate font-mono">{resourceUri}</p>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <button
              onClick={handleReload}
              disabled={loading}
              className="p-2 hover:bg-claude-hover rounded-lg transition-colors text-claude-textSecondary disabled:opacity-50"
              title="Reload"
            >
              <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
            </button>
            <button
              onClick={onClose}
              className="p-2 hover:bg-claude-hover rounded-lg transition-colors text-claude-textSecondary"
            >
              <X size={18} />
            </button>
          </div>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          {/* Metadata bar */}
          <div className="flex items-center gap-4 mb-4 text-[12px] text-claude-textSecondary flex-wrap">
            <span>Server: <span className="font-medium text-claude-text">{serverName}</span></span>
            {mimeType && <span>Type: <span className="font-medium text-claude-text">{mimeType}</span></span>}
            {data && <span>Size: <span className="font-medium text-claude-text">{contentLength} bytes</span></span>}
          </div>

          {/* Error */}
          {error && (
            <div className="mb-4 px-4 py-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-xl text-[13px] text-red-700 dark:text-red-400">
              {error}
            </div>
          )}

          {/* Content */}
          {loading ? (
            <div className="flex items-center justify-center py-16">
              <Loader2 size={24} className="animate-spin text-claude-textSecondary" />
            </div>
          ) : data ? (
            <div className="space-y-4">
              {/* Read Controls */}
              <div className="flex items-center gap-3">
                <label className="text-[12px] text-claude-textSecondary">Offset:</label>
                <input
                  type="number"
                  value={offset}
                  onChange={(e) => setOffset(Math.max(0, parseInt(e.target.value) || 0))}
                  className="w-20 px-2 py-1 border border-claude-border rounded-lg text-[12px] bg-transparent text-claude-text"
                />
                <label className="text-[12px] text-claude-textSecondary">Limit:</label>
                <input
                  type="number"
                  value={limit}
                  onChange={(e) => setLimit(Math.max(1, parseInt(e.target.value) || 1))}
                  className="w-20 px-2 py-1 border border-claude-border rounded-lg text-[12px] bg-transparent text-claude-text"
                />
                <button
                  onClick={() => loadResource()}
                  className="px-3 py-1 rounded-lg text-[12px] bg-claude-hover hover:bg-claude-btn-hover text-claude-text transition-colors"
                >
                  Reload
                </button>
              </div>

              {/* Content Display */}
              {isText ? (
                <div className="relative group">
                  <button
                    onClick={handleCopyContent}
                    className="absolute top-2 right-2 p-1.5 bg-claude-hover/80 hover:bg-claude-btn-hover rounded-lg transition-colors text-claude-textSecondary opacity-0 group-hover:opacity-100"
                    title="Copy content"
                  >
                    {copied ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                  <pre className="bg-claude-hover/30 rounded-xl p-4 text-[13px] text-claude-text font-mono whitespace-pre-wrap leading-relaxed max-h-[400px] overflow-y-auto">
                    {data.content || '(empty)'}
                  </pre>
                </div>
              ) : (
                <div className="bg-claude-hover/30 rounded-xl p-4 text-[13px] text-claude-text">
                  <p className="text-claude-textSecondary mb-2">Binary content (type: {mimeType || 'unknown'})</p>
                  <p className="font-mono text-[12px]">Length: {contentLength} bytes</p>
                </div>
              )}

              {/* Monitoring Toggle */}
              <div className="flex items-center justify-between px-4 py-3 bg-claude-hover/20 rounded-xl">
                <div className="flex items-center gap-2">
                  {monitoring ? (
                    <Bell size={16} className="text-green-500" />
                  ) : (
                    <BellOff size={16} className="text-claude-textSecondary" />
                  )}
                  <span className="text-[13px] text-claude-text">
                    Resource monitoring
                  </span>
                  <span className={`text-[11px] ${monitoring ? 'text-green-500' : 'text-claude-textSecondary'}`}>
                    {monitoring ? 'Enabled' : 'Disabled'}
                  </span>
                </div>
                <button
                  onClick={handleToggleMonitor}
                  disabled={monitorLoading}
                  className={`px-3 py-1.5 rounded-lg text-[12px] font-medium transition-colors ${
                    monitoring
                      ? 'bg-red-50 text-red-600 hover:bg-red-100'
                      : 'bg-claude-hover text-claude-text hover:bg-claude-btn-hover'
                  } disabled:opacity-50`}
                >
                  {monitorLoading ? '...' : monitoring ? 'Disable' : 'Enable'}
                </button>
              </div>

              {/* Metadata collapsible */}
              {data.metadata && (
                <div>
                  <button
                    onClick={() => setShowMeta(!showMeta)}
                    className="flex items-center gap-1 text-[12px] text-claude-textSecondary hover:text-claude-text transition-colors"
                  >
                    {showMeta ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                    Metadata
                  </button>
                  {showMeta && (
                    <pre className="mt-2 bg-claude-hover/20 rounded-xl p-3 text-[12px] text-claude-textSecondary font-mono whitespace-pre-wrap">
                      {JSON.stringify(data.metadata, null, 2)}
                    </pre>
                  )}
                </div>
              )}
            </div>
          ) : null}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end px-6 py-4 border-t border-claude-border">
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

export default ResourceViewerPanel;
