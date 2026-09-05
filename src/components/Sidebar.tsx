import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { createPortal } from 'react-dom';
import { useStreamingStore } from '../stores/useStreamingStore';
import { useUIStore } from '../stores/useUIStore';
import { useI18n } from '../hooks/useI18n';
import {
  IconSidebarToggle,
  IconChatBubble,
  IconCode,
  IconPlusCircle,
  IconProjects,
  IconDotsHorizontal,
  IconStarOutline,
  IconPencil,
  IconTrash,
  IconPalette,
  IconDirectory
} from './Icons';
import claudeImg from '../assets/icons/claude.png';
import searchIconImg from '../assets/icons/search-icon.png';
import customizeIconImg from '../assets/icons/customize-icon.png';
import { NAV_ITEMS } from '../constants';
import { ChevronUp, Settings, HelpCircle, LogOut, Shield, CreditCard, Search, Globe, Users, MessageSquare, Folder, FolderOpen, ChevronDown, ChevronRight, FolderPlus, Plus, MoreHorizontal } from 'lucide-react';
import { getConversations, deleteConversation, updateConversation, getUser, getUserUsage, logout, getUserProfile, getCodeSSO, getProjects, createProjectConversation, deleteProject, openFolder } from '../api';

import SearchModal from './SearchModal';
import CostTracker from './CostTracker';
import EmbeddedBrowser from './EmbeddedBrowser';
import BrowserLivePanel from './BrowserLivePanel';

interface SidebarProps {
  isCollapsed: boolean;
  toggleSidebar: () => void;
  refreshTrigger: number;
  onNewChatClick?: () => void;
  onOpenSettings?: () => void;
  onOpenUpgrade?: () => void;
  onOpenDirectory?: () => void;
  onCloseOverlays?: () => void;
  tunerConfig?: any;
  setTunerConfig?: (config: any) => void;
  titleBarHeight?: number;
  activeConversationId?: string;
}

interface RenameModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (newTitle: string) => void;
  initialTitle: string;
}

const RenameModal = ({ isOpen, onClose, onSave, initialTitle }: RenameModalProps) => {
  const { t } = useI18n();
  const [title, setTitle] = useState(initialTitle);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      setTitle(initialTitle);
      setTimeout(() => {
        if (inputRef.current) {
          inputRef.current.focus();
          inputRef.current.select();
        }
      }, 50);
    }
  }, [isOpen, initialTitle]);

  if (!isOpen) return null;

  return createPortal(
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40" onClick={onClose}>
      <div
        className="bg-claude-input rounded-2xl shadow-xl w-[400px] p-6 animate-fade-in"
        onClick={e => e.stopPropagation()}
      >
        <h3 className="text-[18px] font-semibold text-claude-text mb-4">{t('common.rename')}</h3>
        <input
          ref={inputRef}
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              if (title.trim()) onSave(title.trim());
            } else if (e.key === 'Escape') {
              onClose();
            }
          }}
          className="w-full px-3 py-2 bg-transparent border border-claude-border rounded-lg text-claude-text focus:outline-none focus:border-blue-500 mb-6 text-[15px]"
        />
        <div className="flex justify-end gap-3">
          <button
            onClick={onClose}
            className="px-4 py-2 text-[14px] font-medium text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
          >
            {t('common.cancel')}
          </button>
          <button
            onClick={() => {
              if (title.trim()) onSave(title.trim());
            }}
            disabled={!title.trim()}
            className="px-4 py-2 text-[14px] font-medium text-white bg-[#333333] hover:bg-[#1a1a1a] dark:bg-[#FFFFFF] dark:text-black dark:hover:bg-[#e5e5e5] rounded-lg transition-colors disabled:opacity-50"
          >
            {t('common.save')}
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
};

const Sidebar = ({ isCollapsed, toggleSidebar, refreshTrigger, onNewChatClick, onOpenSettings, onOpenUpgrade, onOpenDirectory, onCloseOverlays, tunerConfig, setTunerConfig, titleBarHeight, activeConversationId }: SidebarProps) => {
  const { t } = useI18n();
  const navigate = useNavigate();
  const location = useLocation();
  const codeJumpUrl = ((import.meta as any).env?.VITE_CODE_JUMP_URL || '/code/').trim();
  const [chats, setChats] = useState<any[]>([]);
  const [projects, setProjects] = useState<any[]>([]);
  const [activeMenuId, setActiveMenuId] = useState<string | null>(null);
  const [expandedProjects, setExpandedProjects] = useState<Set<string>>(new Set());
  const [menuPosition, setMenuPosition] = useState<{ top: number, left: number } | null>(null);
  const [showRenameModal, setShowRenameModal] = useState(false);
  const [renameChatId, setRenameChatId] = useState<string | null>(null);
  const [renameInitialTitle, setRenameInitialTitle] = useState('');
  const [userUser, setUserUser] = useState<any>(null);
  const [showUserMenu, setShowUserMenu] = useState(false);
  const [showLogoutConfirm, setShowLogoutConfirm] = useState(false);
  const [showHelpModal, setShowHelpModal] = useState(false);
  const [userMenuPos, setUserMenuPos] = useState<{ bottom: number; left: number } | null>(null);
  const [planLabel, setPlanLabel] = useState('Free plan');
  const [usageData, setUsageData] = useState<{ token_used: number; token_quota: number; quota?: { window?: { used: number; limit: number; resetAt?: string }; week?: { used: number; limit: number; resetAt?: string } } } | null>(null);
  const [isAdmin, setIsAdmin] = useState(false);
  const [showSearch, setShowSearch] = useState(false);
  const [isRecentsCollapsed, setIsRecentsCollapsed] = useState(false);
  const [isNewChatAnimating, setIsNewChatAnimating] = useState(false);
  const [streamingIds, setStreamingIds] = useState<Set<string>>(new Set(useStreamingStore.getState().streamingIds));
  const [updateStatus, setUpdateStatus] = useState<{ type: string; version?: string; percent?: number } | null>(null);
  
  // New states for tabs and browser
  const [activeTab, setActiveTab] = useState<'chat' | 'cowork' | 'code'>('chat');
  const [showBrowser, setShowBrowser] = useState(false);
  const [browserUrl, setBrowserUrl] = useState('https://example.com');

  useEffect(() => {
    let prevSize = useStreamingStore.getState().streamingIds.size;
    const handler = () => {
      const newIds = new Set(useStreamingStore.getState().streamingIds);
      setStreamingIds(newIds);
      if (newIds.size < prevSize) {
        setTimeout(() => fetchPlan(), 1500);
      }
      prevSize = newIds.size;
    };
    window.addEventListener('streaming-change', handler);
    return () => window.removeEventListener('streaming-change', handler);
  }, []);

  // Sync active tab with route
  useEffect(() => {
    if (location.pathname === '/cowork') {
      setActiveTab('cowork');
    } else if (activeTab === 'cowork') {
      setActiveTab('chat');
    }
  }, [location.pathname]);

  // Listen for auto-update events (Tauri uses different update mechanism)
  useEffect(() => {
    // Tauri updates handled via tauriAPI.checkUpdate / tauriAPI.installUpdate
    // No persistent event listener needed
  }, []);

  const menuRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const userMenuRef = useRef<HTMLDivElement>(null);
  const userBtnRef = useRef<HTMLButtonElement>(null);

  // Map nav ids to the correct custom icon
  const getIcon = (id: string, size: number) => {
    const className = "dark:invert transition-[filter] duration-200";
    switch (id) {
      case 'chats': return <IconChatBubble size={size} className={className} />;
      case 'projects': return <IconProjects size={size} className={className} />;
      case 'design': return <IconPalette size={size} className={className} />;
      case 'directory': return <IconDirectory size={size} className={className} />;
      case 'code': return <IconCode size={size} className={className} />;
      default: return <IconChatBubble size={size} className={className} />;
    }
  };

  const handleNewChat = () => {
    setIsNewChatAnimating(true);
    setTimeout(() => setIsNewChatAnimating(false), 300);
    if (onNewChatClick) onNewChatClick();
    navigate('/');
  };

  const updateTuner = (key: string, value: number) => {
    if (setTunerConfig && tunerConfig) {
      setTunerConfig({ ...tunerConfig, [key]: value });
    }
  };

  const handleNavClick = (id: string) => {
    if (id === 'chats') {
      navigate('/chats');
      return;
    }
    if (id === 'projects') {
      navigate('/projects');
      return;
    }
    if (id === 'design') {
      navigate('/design');
      return;
    }
    if (id === 'directory') {
      onOpenDirectory?.();
      return;
    }
    if (id === 'code') {
      // Disabled temporarily
      return;
    }
  };

  const fetchChats = useCallback(async (): Promise<boolean> => {
    try {
      const data = await getConversations();
      console.log('[Sidebar] Fetched conversations:', data);
      if (Array.isArray(data)) {
        // 去重（可能存在 id 重复的旧数据）
        const seen = new Set<string>();
        const unique = data.filter((chat: any) => {
          if (seen.has(chat.id)) return false;
          seen.add(chat.id);
          return true;
        });
        setChats(unique);
        return true;
      }
      return false;
    } catch (e) {
      console.error("Failed to fetch chats", e);
      return false;
    }
  }, []);

  const fetchProjects = useCallback(async (): Promise<boolean> => {
    try {
      const data = await getProjects();
      if (Array.isArray(data)) {
        setProjects(data);
        return true;
      }
      return false;
    } catch (e) {
      console.error("Failed to fetch projects", e);
      return false;
    }
  }, []);

  const refreshAll = useCallback(() => {
    fetchChats();
    fetchProjects();
  }, [fetchChats, fetchProjects]);

  useEffect(() => {
    setUserUser(getUser());
    // 桥接服务可能比前端晚就绪（后端重启期间侧栏会拿到空数据），
    // 首次加载失败时自动重试，避免侧栏一直空白
    let cancelled = false;
    (async () => {
      for (let i = 0; i < 6 && !cancelled; i++) {
        const [cOk, pOk] = await Promise.all([fetchChats(), fetchProjects()]);
        if (cOk && pOk) break;
        await new Promise(r => setTimeout(r, 2000));
      }
    })();
    fetchPlan();
    getUserProfile().then((data: any) => {
      const p = data?.user || data;
      if (p?.role === 'admin' || p?.role === 'superadmin') setIsAdmin(true);
      if (p?.nickname || p?.full_name) {
        setUserUser((prev: any) => ({ ...prev, ...p }));
      }
    }).catch(() => { });

    // 閻╂垵鎯夐弽鍥暯閺囧瓨鏌婃禍瀣╂
    const handleTitleUpdate = () => {
      console.log('[Sidebar] Title update event received, fetching conversations...');
      refreshAll();
    };

    // 閻╂垵鎯夐悽銊﹀煕鐠у嫭鏋￠弴瀛樻煀娴滃娆?
    const handleProfileUpdate = () => {
      setUserUser(getUser());
      getUserProfile().then((data: any) => {
        const p = data?.user || data;
        if (p?.role === 'admin' || p?.role === 'superadmin') setIsAdmin(true);
        if (p?.nickname || p?.full_name) {
          setUserUser((prev: any) => ({ ...prev, ...p }));
        }
      }).catch(() => { });
    };

    window.addEventListener('conversationTitleUpdated', handleTitleUpdate);
    window.addEventListener('userProfileUpdated', handleProfileUpdate);
    window.addEventListener('conversationsUpdated', handleTitleUpdate);
    // 定时 + 窗口聚焦时刷新：后端重启/网络恢复后侧栏自动恢复显示
    const interval = setInterval(refreshAll, 30_000);
    const handleFocus = () => refreshAll();
    window.addEventListener('focus', handleFocus);

    return () => {
      cancelled = true;
      clearInterval(interval);
      window.removeEventListener('focus', handleFocus);
      window.removeEventListener('conversationTitleUpdated', handleTitleUpdate);
      window.removeEventListener('userProfileUpdated', handleProfileUpdate);
      window.removeEventListener('conversationsUpdated', handleTitleUpdate);
    };
  }, [refreshTrigger, refreshAll]);

  const fetchPlan = async () => {
    try {
      const data = await getUserUsage();
      const windowUsed = data?.quota?.window?.used || 0;
      const windowLimit = data?.quota?.window?.limit || 0;
      const weekUsed = data?.quota?.week?.used || 0;
      const weekLimit = data?.quota?.week?.limit || 0;
      const tokenUsed = Number(data?.token_used) || windowUsed || weekUsed || 0;
      const tokenQuota = Number(data?.token_quota) || windowLimit || weekLimit || 0;
      setUsageData({
        token_used: tokenUsed,
        token_quota: tokenQuota,
        quota: data?.quota,
      });
      if (data.plan && data.plan.name) {
        const nameMap: Record<string, string> = {
          '体验包': t('sidebar.trailPlan'),
          '基础月卡': t('sidebar.proPlan'),
          '专业月卡': t('sidebar.maxX5Plan'),
          '至尊月卡': t('sidebar.maxX20Plan'),
        };
        setPlanLabel(nameMap[data.plan.name] || data.plan.name);
      } else {
        setPlanLabel(t('sidebar.freePlan'));
      }
    } catch (e) {
      // 閼惧嘲褰囨径杈Е娣囨繃瀵旀妯款吇
    }
  };

  const handleRenameClick = (e: React.MouseEvent, chatId: string) => {
    e.stopPropagation();
    const chat = chats.find(c => c.id === chatId);
    if (chat) {
      setRenameChatId(chat.id);
      setRenameInitialTitle(chat.title || t('customize.untitledConversation'));
      setShowRenameModal(true);
    }
    setActiveMenuId(null);
  };

  const handleRenameSubmit = async (newTitle: string) => {
    if (!renameChatId) return;

    try {
      // Optimistic update
      setChats(chats.map(c => c.id === renameChatId ? { ...c, title: newTitle } : c));
      await updateConversation(renameChatId, { title: newTitle });

      // Notify other components (like Header) about the title change if it's the active chat
      if (location.pathname === `/chat/${renameChatId}`) {
        window.dispatchEvent(new CustomEvent('conversationTitleUpdated'));
      }
    } catch (err) {
      console.error('Failed to rename chat:', err);
      // Revert on failure
      fetchChats();
    }
    setShowRenameModal(false);
    setRenameChatId(null);
  };

  const handleDeleteChat = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setActiveMenuId(null);
    setChats(prev => prev.filter(c => c.id !== id));
    try {
      await deleteConversation(id);
      if (location.pathname === `/chat/${id}`) {
        navigate('/');
      }
    } catch (err) {
      console.error('[Sidebar] Delete failed:', err);
      fetchChats();
    }
  };

  // Close menu when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      // 韫囩晫鏆愰悽銊﹀煕閹稿鎸抽張顒冮煩閻ㄥ嫮鍋ｉ崙浼欑礄閻㈣鲸瀵滈柦?onClick 婢跺嫮鎮婇敍?
      if (userBtnRef.current && userBtnRef.current.contains(event.target as Node)) {
        return;
      }
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setActiveMenuId(null);
      }
      if (userMenuRef.current && !userMenuRef.current.contains(event.target as Node)) {
        setShowUserMenu(false);
      }
    };

    // Close on scroll
    const handleScroll = () => {
      if (activeMenuId !== null) setActiveMenuId(null);
      if (showUserMenu) setShowUserMenu(false);
    };

    if (activeMenuId !== null || showUserMenu) {
      document.addEventListener('click', handleClickOutside);
      // Attach scroll listener to the sidebar scroll container
      const scrollEl = scrollRef.current;
      scrollEl?.addEventListener('scroll', handleScroll);
      window.addEventListener('resize', handleScroll);
    }

    return () => {
      document.removeEventListener('click', handleClickOutside);
      const scrollEl = scrollRef.current;
      scrollEl?.removeEventListener('scroll', handleScroll);
      window.removeEventListener('resize', handleScroll);
    };
  }, [activeMenuId, showUserMenu]);

  const handleMenuClick = (e: React.MouseEvent, chatId: string) => {
    e.stopPropagation();
    e.preventDefault();

    if (activeMenuId === `chat:${chatId}`) {
      setActiveMenuId(null);
      return;
    }

    const button = e.currentTarget as HTMLElement;
    const buttonRect = button.getBoundingClientRect();
    const parentElement = button.parentElement;

    let leftPos = buttonRect.right - 200; // Fallback to button alignment

    if (parentElement) {
      const parentRect = parentElement.getBoundingClientRect();
      // Align right edge of menu (200px wide) with the right edge of the chat item container
      leftPos = parentRect.right - 200;
    }

    const menuHeight = 120; // Approximate height of the menu
    let topPos = buttonRect.bottom + 4;

    // Check if menu would overflow bottom of viewport
    if (topPos + menuHeight > window.innerHeight) {
      // Position above the button instead
      topPos = buttonRect.top - menuHeight - 4;
    }

    setMenuPosition({
      top: topPos,
      left: leftPos,
    });
    setActiveMenuId(`chat:${chatId}`);
  };

  const handleProjectMenuClick = (e: React.MouseEvent, projectId: string) => {
    e.stopPropagation();
    e.preventDefault();
    if (activeMenuId === `project:${projectId}`) {
      setActiveMenuId(null);
      return;
    }
    const button = e.currentTarget as HTMLElement;
    const buttonRect = button.getBoundingClientRect();
    const parentElement = button.parentElement;
    let leftPos = buttonRect.right - 200;
    if (parentElement) {
      const parentRect = parentElement.getBoundingClientRect();
      leftPos = parentRect.right - 200;
    }
    const menuHeight = 140;
    let topPos = buttonRect.bottom + 4;
    if (topPos + menuHeight > window.innerHeight) {
      topPos = buttonRect.top - menuHeight - 4;
    }
    setMenuPosition({ top: topPos, left: leftPos });
    setActiveMenuId(`project:${projectId}`);
  };

  const handleOpenProjectFolder = async (project: any) => {
    setActiveMenuId(null);
    const wsPath = project.workspace_path || project.path;
    if (!wsPath) {
      console.warn('Project has no workspace_path');
      return;
    }
    try {
      const result = await openFolder(wsPath);
      if (!result.ok) {
        console.error('openFolder failed:', result.error);
      }
    } catch (err) {
      console.error('openFolder threw:', err);
    }
  };

  const handleDeleteProjectCard = async (project: any) => {
    setActiveMenuId(null);
    const confirmed = window.confirm(`确定要删除项目「${project.name || '未命名'}」吗？该项目下的所有会话也会被删除。`);
    if (!confirmed) return;
    try {
      // If currently inside a conversation belonging to this project, leave first
      const match = location.pathname.match(/^\/chat\/([^/]+)/);
      const currentChatId = match ? match[1] : null;
      const insideProject = currentChatId
        ? chats.some((c: any) => c.id === currentChatId && c.project_id === project.id)
        : false;
      if (insideProject) {
        navigate('/');
      }
      await deleteProject(project.id);
      await refreshAll();
    } catch (err) {
      console.error('deleteProject failed:', err);
    }
  };

  // Build the display list: project cards (with their conversations) and regular chats, ordered by recency
  const displayList = React.useMemo(() => {
    const projectMap = new Map<string, { project: any; conversations: any[]; lastActive: string }>();
    const regular: any[] = [];

    for (const chat of chats) {
      if (chat.project_id) {
        if (!projectMap.has(chat.project_id)) {
          const project = projects.find((p: any) => p.id === chat.project_id);
          if (!project) {
            regular.push(chat);
            continue;
          }
          projectMap.set(chat.project_id, { project, conversations: [], lastActive: chat.updated_at || chat.created_at || '' });
        }
        const entry = projectMap.get(chat.project_id)!;
        entry.conversations.push(chat);
        const t = chat.updated_at || chat.created_at || '';
        if (t > entry.lastActive) entry.lastActive = t;
      } else {
        regular.push(chat);
      }
    }

    const projectItems = Array.from(projectMap.values()).sort((a, b) => (b.lastActive > a.lastActive ? 1 : -1));
    const regSorted = [...regular].sort((a: any, b: any) => ((b.updated_at || '') > (a.updated_at || '') ? 1 : -1));

    const merged: any[] = [];
    let i = 0;
    let j = 0;
    while (i < projectItems.length || j < regSorted.length) {
      if (i < projectItems.length && (j >= regSorted.length || projectItems[i].lastActive >= (regSorted[j].updated_at || ''))) {
        merged.push({ type: 'project', ...projectItems[i] });
        i++;
      } else if (j < regSorted.length) {
        merged.push({ type: 'chat', chat: regSorted[j] });
        j++;
      } else {
        break;
      }
    }
    return merged;
  }, [chats, projects]);

  const toggleProject = (projectId: string) => {
    setExpandedProjects(prev => {
      const next = new Set(prev);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      return next;
    });
  };

  const openProjectConversation = (convId: string) => {
    onCloseOverlays?.();
    navigate(`/chat/${convId}`);
  };

  const [creatingProjectConvs, setCreatingProjectConvs] = useState<Set<string>>(new Set());

  const handleAddProjectConversation = async (e: React.MouseEvent, project: any) => {
    e.stopPropagation();
    e.preventDefault();
    const projectId = project.id;
    if (creatingProjectConvs.has(projectId)) return;
    setCreatingProjectConvs(prev => new Set(prev).add(projectId));
    try {
      const workspacePath = project.workspace_path || project.path || undefined;
      const result = await createProjectConversation(projectId, undefined, undefined, workspacePath);
      const newConvId = result?.id;
      await refreshAll();
      // Auto-expand the project so the new conversation is visible
      setExpandedProjects(prev => {
        const next = new Set(prev);
        next.add(projectId);
        return next;
      });
      if (newConvId) {
        onCloseOverlays?.();
        navigate(`/chat/${newConvId}`);
      } else {
        setExpandedProjects(prev => {
          const next = new Set(prev);
          next.add(projectId);
          return next;
        });
      }
    } catch (err) {
      console.error('Failed to create project conversation:', err);
    } finally {
      setCreatingProjectConvs(prev => {
        const next = new Set(prev);
        next.delete(projectId);
        return next;
      });
    }
  };

  return (
    <>
      <div
        className={`
          h-screen bg-claude-sidebar border-r border-claude-border flex-shrink-0 text-claude-text antialiased flex flex-col transition-all duration-200 ease-in-out overflow-hidden relative
        `}
        style={{
          width: isCollapsed ? '46px' : (showBrowser ? `${tunerConfig?.sidebarWidth || 280}px` : `${tunerConfig?.sidebarWidth || 280}px`)
        }}
      >
        {/* New Tab Navigation - Chat/Cowork/Code */}
        {!isCollapsed && (
          <div
            className="flex-shrink-0 border-b border-claude-border"
            style={{
              marginTop: `${titleBarHeight || 44}px`,
              paddingLeft: '8px',
              paddingRight: '8px',
              paddingTop: '8px',
              paddingBottom: '4px'
            }}
          >
            <div className="flex gap-1">
              {[
                { id: 'chat' as const, icon: <MessageSquare size={14} />, labelKey: 'sidebar.chatTab' },
                { id: 'cowork' as const, icon: <Users size={14} />, labelKey: 'sidebar.coworkTab' },
                { id: 'code' as const, icon: <IconCode size={14} />, labelKey: 'sidebar.codeTab' }
              ].map(tab => (
                <button
                  key={tab.id}
                  onClick={() => {
                    if (tab.id === 'cowork') {
                      navigate('/cowork');
                    } else {
                      setActiveTab(tab.id);
                      if (location.pathname === '/cowork') navigate('/');
                    }
                  }}
                  className={`flex-1 flex items-center justify-center gap-1.5 py-1.5 px-2 rounded-md text-xs font-medium transition-all ${
                    (tab.id === 'cowork' && location.pathname === '/cowork') || (tab.id === activeTab && tab.id !== 'cowork')
                      ? 'bg-claude-hover text-claude-text'
                      : 'text-claude-textSecondary hover:bg-claude-hover hover:text-claude-text'
                  }`}
                >
                  {tab.icon}
                  {t(tab.labelKey)}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* New Chat Button */}
        <div
          className="flex-shrink-0"
          style={{
            marginTop: '8px',
            paddingLeft: '9px',
            paddingRight: '9px',
            marginBottom: '2px'
          }}
        >
          <button
            onClick={handleNewChat}
            className="w-full flex items-center justify-start text-claude-text hover:bg-claude-hover rounded-lg transition-colors group overflow-hidden whitespace-nowrap"
            style={{
              paddingTop: '2px',
              paddingBottom: '2px',
              paddingLeft: '0px',
              gap: '8px'
            }}
          >
            <div className={`text-claude-text flex-shrink-0 flex items-center justify-center`}>
              <IconPlusCircle
                size={27}
                className={`transition-all duration-200 group-hover:brightness-90 ${isNewChatAnimating ? "rotate-90 scale-100" : "group-hover:scale-110 group-hover:-rotate-3"}`}
              />
            </div>
            <span
              className={`leading-none transition-opacity duration-200 text-left ${isCollapsed ? 'opacity-0 w-0 hidden' : 'opacity-100 block'}`}
              style={{ fontSize: '14px', fontWeight: 400 }}
            >
              {t('sidebar.newChat')}
            </span>
          </button>
        </div>

        {/* Browser Toggle Button */}
        {!isCollapsed && (
          <div
            className="flex-shrink-0"
            style={{
              marginTop: '2px',
              paddingLeft: '9px',
              paddingRight: '9px',
              marginBottom: '8px'
            }}
          >
            <button
              onClick={() => setShowBrowser(!showBrowser)}
              className={`w-full flex items-center justify-start text-claude-text hover:bg-claude-hover rounded-lg transition-colors group overflow-hidden whitespace-nowrap ${showBrowser ? 'bg-claude-hover' : ''}`}
              style={{
                paddingTop: '2px',
                paddingBottom: '2px',
                paddingLeft: '0px',
                gap: '8px'
              }}
            >
              <div className={`text-claude-text flex-shrink-0 flex items-center justify-center`}>
                <Globe
                  size={27}
                  className={`transition-all duration-200 group-hover:brightness-90 group-hover:scale-110 ${showBrowser ? 'text-blue-400' : ''}`}
                />
              </div>
              <span
                className={`leading-none transition-opacity duration-200 text-left opacity-100 block`}
                style={{ fontSize: '14px', fontWeight: 400 }}
              >
                {t('sidebar.browser')}
              </span>
            </button>
          </div>
        )}

        {/* Customize - Fixed */}
        <div
          className="flex-shrink-0"
          style={{
            marginTop: '2px',
            paddingLeft: '9px',
            paddingRight: '9px',
            marginBottom: '16px'
          }}
        >
          <button
            onClick={() => navigate('/customize')}
            className={`w-full flex items-center justify-start text-claude-text hover:bg-claude-hover rounded-lg transition-colors group overflow-hidden whitespace-nowrap ${location.pathname === '/customize' ? 'bg-claude-hover' : ''}`}
            style={{
              paddingTop: '2px',
              paddingBottom: '2px',
              paddingLeft: '0px',
              gap: '8px'
            }}
          >
            <div className={`text-claude-text flex-shrink-0 flex items-center justify-center`} style={{ width: '27px', height: '27px' }}>
              <img
                src={customizeIconImg}
                alt="Customize"
                style={{ width: '24px', height: '24px' }}
                className="object-contain dark:invert transition-all duration-200 group-hover:brightness-90 group-hover:scale-110 group-hover:-rotate-3 group-active:rotate-12 group-active:scale-90"
              />
            </div>
            <span
              className={`leading-none transition-opacity duration-200 text-left ${isCollapsed ? 'opacity-0 w-0 hidden' : 'opacity-100 block'}`}
              style={{ fontSize: '14px', fontWeight: 400 }}
            >
              {t('sidebar.customize')}
            </span>
          </button>
        </div>

        {/* Scrollable Area containing Nav and Recents */}
        <div
          ref={scrollRef}
          className="flex-1 overflow-y-auto sidebar-scroll min-h-0 pb-6"
          style={{ paddingLeft: '9px', paddingRight: '9px', paddingTop: '0px' }}
        >

          {/* Navigation Links */}
          <nav className="space-y-0.5 mb-6">
            {NAV_ITEMS.map((item) => (
              <button
                key={item.id}
                onClick={() => handleNavClick(item.id)}
                className={`w-full flex items-center justify-start text-claude-text hover:bg-claude-hover rounded-lg transition-colors group overflow-hidden whitespace-nowrap ${(location.pathname === '/chats' && item.id === 'chats') || (location.pathname === '/projects' && item.id === 'projects') ? 'bg-claude-hover' : ''}`}
                style={{
                  fontWeight: 400,
                  paddingTop: '2px',
                  paddingBottom: '2px',
                  paddingLeft: '0px',
                  gap: '8px'
                }}
              >
                <div className={`text-claude-text flex-shrink-0 transition-colors flex items-center justify-center`}>
                  {getIcon(item.id, 27)}
                </div>
                <span
                  className={`leading-none transition-opacity duration-200 text-left ${isCollapsed ? 'opacity-0 w-0 hidden' : 'opacity-100 block'}`}
                  style={{ fontSize: '14px' }}
                >
                  {t(item.label)}
                </span>
              </button>
            ))}
          </nav>

          {/* Recents Section Header */}
          <div
            className={`group flex items-center gap-3 px-3 pb-2 transition-opacity duration-200 select-none ${isCollapsed ? 'opacity-0 hidden' : 'opacity-100'}`}
            style={{
              marginTop: `${tunerConfig?.recentsMt || 0}px`,
              paddingLeft: `${tunerConfig?.recentsPl || 12}px`,
              paddingRight: '12px'
            }}
          >
            <span className="text-[13px] font-medium text-claude-textSecondary">{t('sidebar.recents')}</span>
            <button
              onClick={(e) => {
                e.stopPropagation();
                setIsRecentsCollapsed(!isRecentsCollapsed);
              }}
              className="text-[13px] font-medium text-claude-textSecondary opacity-0 group-hover:opacity-60 hover:opacity-100 transition-opacity cursor-pointer outline-none"
            >
              {isRecentsCollapsed ? t('sidebar.show') : t('sidebar.hide')}
            </button>
          </div>

          {/* Recents List */}
          <div className={`space-y-0.5 pb-2 transition-all duration-200 ${isCollapsed || isRecentsCollapsed ? 'opacity-0 hidden h-0 overflow-hidden' : 'opacity-100'}`}>
            {displayList.slice(0, 30).map((item) => {
              if (item.type === 'project') {
                const isExpanded = expandedProjects.has(item.project.id);
                const projectChats = item.conversations || [];
                return (
                  <div key={`project-${item.project.id}`} className="mb-1">
                    {/* Project Card - Folder Icon Card */}
                    <div
                      onClick={() => toggleProject(item.project.id)}
                      className="relative group flex items-center w-full rounded-lg transition-colors cursor-pointer min-h-[32px] hover:bg-claude-hover"
                      style={{
                        paddingTop: `${tunerConfig?.recentsItemPy || 6}px`,
                        paddingBottom: `${tunerConfig?.recentsItemPy || 6}px`,
                        paddingLeft: `${tunerConfig?.recentsPl || 12}px`,
                        paddingRight: `${tunerConfig?.recentsPl || 12}px`,
                      }}
                    >
                      {/* Left: collapse/expand chevron */}
                      <div className="flex-shrink-0 mr-1.5 text-claude-textSecondary">
                        {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      </div>
                      {/* Left: folder icon */}
                      <div className="flex-shrink-0 mr-2 text-claude-textSecondary">
                        {isExpanded ? <FolderOpen size={18} /> : <Folder size={18} />}
                      </div>
                      <div className="flex-1 min-w-0 pr-14">
                        <div
                          className="text-claude-text truncate leading-snug font-medium"
                          style={{ fontSize: `${tunerConfig?.recentsFontSize || 13}px` }}
                        >
                          {item.project.name || 'Project'}
                        </div>
                      </div>
                      {/* Right: ... menu button (left of +) */}
                      <button
                        onClick={(e) => handleProjectMenuClick(e, item.project.id)}
                        title="更多操作"
                        className={`
                          absolute right-8 top-1/2 -translate-y-1/2 p-0.5 rounded text-claude-textSecondary hover:text-claude-text hover:bg-claude-border transition-all
                          ${activeMenuId === `project:${item.project.id}` ? 'opacity-100 block' : 'opacity-0 group-hover:opacity-100 hidden group-hover:block'}
                        `}
                      >
                        <MoreHorizontal size={16} />
                      </button>
                      {/* Right: add new conversation (+) button */}
                      <button
                        onClick={(e) => handleAddProjectConversation(e, item.project)}
                        disabled={creatingProjectConvs.has(item.project.id)}
                        title="新建会话"
                        className={`
                          absolute right-2 top-1/2 -translate-y-1/2 p-0.5 rounded text-claude-textSecondary hover:text-claude-text hover:bg-claude-border transition-all
                          ${creatingProjectConvs.has(item.project.id) ? 'opacity-60 cursor-wait' : 'opacity-0 group-hover:opacity-100 hidden group-hover:block'}
                        `}
                      >
                        {creatingProjectConvs.has(item.project.id) ? (
                          <svg width="14" height="14" viewBox="0 0 24 24" className="animate-spin" fill="none" stroke="currentColor" strokeWidth="2">
                            <path d="M21 12a9 9 0 1 1-6.219-8.56" />
                          </svg>
                        ) : (
                          <Plus size={16} />
                        )}
                      </button>
                    </div>
                    {/* Project Conversations (collapsible) */}
                    {isExpanded && projectChats.map((chat: any) => {
                      const isActive = location.pathname === `/chat/${chat.id}`;
                      return (
                        <div
                          key={chat.id}
                          onClick={() => openProjectConversation(chat.id)}
                          className={`
                            relative group flex items-center w-full rounded-lg transition-colors cursor-pointer min-h-[28px] ml-3
                            ${isActive || activeMenuId === `chat:${chat.id}` ? 'bg-claude-hover' : 'hover:bg-claude-hover'}
                          `}
                          style={{
                            width: `calc(100% - 12px)`,
                            paddingTop: `${tunerConfig?.recentsItemPy || 6}px`,
                            paddingBottom: `${tunerConfig?.recentsItemPy || 6}px`,
                            paddingLeft: `${tunerConfig?.recentsPl || 12}px`,
                            paddingRight: `${tunerConfig?.recentsPl || 12}px`,
                          }}
                        >
                          {/* Streaming indicator */}
                          {streamingIds.has(chat.id) && (
                            <span
                              className="flex-shrink-0 mr-2 w-[7px] h-[7px] rounded-full bg-neutral-700 dark:bg-neutral-300 animate-pulse"
                              style={{ animationDuration: '1.6s' }}
                            />
                          )}
                          {/* Chat Title */}
                          <div className="flex-1 min-w-0 pr-6">
                            <div
                              className="text-claude-text truncate leading-snug"
                              style={{ fontSize: `${tunerConfig?.recentsFontSize || 13}px` }}
                            >
                              {chat.title || t('customize.untitledConversation')}
                            </div>
                          </div>
                          {/* Three Dots Button */}
                          <button
                            onClick={(e) => { e.stopPropagation(); handleMenuClick(e, chat.id); }}
                            className={`
                              absolute right-2 top-1/2 -translate-y-1/2 p-0.5 rounded text-claude-textSecondary hover:text-claude-text transition-all
                              ${activeMenuId === `chat:${chat.id}` ? 'opacity-100 block' : 'opacity-0 group-hover:opacity-100 hidden group-hover:block'}
                            `}
                          >
                            <IconDotsHorizontal size={16} />
                          </button>
                        </div>
                      );
                    })}
                  </div>
                );
              }
              // Regular chat item
              const chat = item.chat;
              const isActive = location.pathname === `/chat/${chat.id}`;
              return (
                <div
                  key={chat.id}
                  onClick={() => { onCloseOverlays?.(); navigate(`/chat/${chat.id}`); }}
                  className={`
                    relative group flex items-center w-full rounded-lg transition-colors cursor-pointer min-h-[32px]
                    ${isActive || activeMenuId === `chat:${chat.id}` ? 'bg-claude-hover' : 'hover:bg-claude-hover'}
                  `}
                  style={{
                    paddingTop: `${tunerConfig?.recentsItemPy || 6}px`,
                    paddingBottom: `${tunerConfig?.recentsItemPy || 6}px`,
                    paddingLeft: `${tunerConfig?.recentsPl || 12}px`,
                    paddingRight: `${tunerConfig?.recentsPl || 12}px`
                  }}
                >
                  {/* Streaming indicator */}
                  {streamingIds.has(chat.id) && (
                    <span
                      className="flex-shrink-0 mr-2 w-[7px] h-[7px] rounded-full bg-neutral-700 dark:bg-neutral-300 animate-pulse"
                      style={{ animationDuration: '1.6s' }}
                    />
                  )}
                  {/* Chat Title */}
                  <div className="flex-1 min-w-0 pr-6">
                    <div
                      className="text-claude-text truncate leading-snug"
                      style={{ fontSize: `${tunerConfig?.recentsFontSize || 13}px` }}
                    >
                      {chat.title || t('customize.untitledConversation')}
                    </div>
                    {chat.project_name && (
                      <div className="text-[11px] text-claude-textSecondary truncate leading-snug mt-0.5 opacity-60">
                        {chat.project_name}
                      </div>
                    )}
                  </div>

                  {/* Three Dots Button */}
                  <button
                    onClick={(e) => handleMenuClick(e, chat.id)}
                    className={`
                      absolute right-2 top-1/2 -translate-y-1/2 p-0.5 rounded text-claude-textSecondary hover:text-claude-text transition-all
                      ${activeMenuId === `chat:${chat.id}` ? 'opacity-100 block' : 'opacity-0 group-hover:opacity-100 hidden group-hover:block'}
                    `}
                  >
                    <IconDotsHorizontal size={16} />
                  </button>
                </div>
              );
            })}
            {chats.length > 30 && (
              <button
                onClick={() => { onCloseOverlays?.(); navigate('/chats'); }}
                className="w-full flex items-center gap-2 rounded-lg hover:bg-claude-hover transition-colors text-claude-textSecondary hover:text-claude-text"
                style={{
                  paddingTop: `${tunerConfig?.recentsItemPy || 6}px`,
                  paddingBottom: `${tunerConfig?.recentsItemPy || 6}px`,
                  paddingLeft: `${tunerConfig?.recentsPl || 12}px`,
                }}
              >
                <IconDotsHorizontal size={18} className="opacity-60" />
                <span style={{ fontSize: `${tunerConfig?.recentsFontSize || 13}px` }} className="leading-tight">{t('sidebar.allChats')}</span>
              </button>
            )}
          </div>

        </div>

        {/* Update status banner */}
        {updateStatus && !isCollapsed && (updateStatus.type === 'available' || updateStatus.type === 'progress' || updateStatus.type === 'downloaded') && (
          <div className="mx-3 mb-2 mt-auto">
            {(updateStatus.type === 'available' || updateStatus.type === 'progress') && (
              <div className="flex items-center gap-2.5 px-3 py-2.5 rounded-lg bg-claude-hover">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-claude-textSecondary flex-shrink-0 animate-spin">
                  <path d="M21 12a9 9 0 1 1-6.219-8.56" />
                </svg>
                <div className="flex-1 min-w-0">
                  <div className="text-[12px] text-claude-textSecondary leading-tight">
                    {t('sidebar.updateDownloading')}{updateStatus.percent != null ? ` ${updateStatus.percent}%` : ''}
                  </div>
                  {updateStatus.percent != null && (
                    <div className="mt-1.5 h-[3px] rounded-full bg-claude-border overflow-hidden">
                      <div className="h-full rounded-full bg-claude-textSecondary transition-all duration-300" style={{ width: `${updateStatus.percent}%` }} />
                    </div>
                  )}
                </div>
              </div>
            )}
            {updateStatus.type === 'downloaded' && (
              <div className="px-3 py-3 rounded-lg bg-claude-hover">
                <div className="flex items-center gap-2 mb-1">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-claude-text flex-shrink-0">
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                    <polyline points="7 10 12 15 17 10" />
                    <line x1="12" y1="15" x2="12" y2="3" />
                  </svg>
                  <div className="text-[13px] text-claude-text font-medium leading-tight">{t('sidebar.updatedTo')} {updateStatus.version}</div>
                </div>
                <div className="text-[11.5px] text-claude-textSecondary mb-2.5 ml-6">{t('sidebar.relaunchToApply')}</div>
                <button
                  onClick={() => { import('../utils/tauriAPI').then(m => m.tauriAPI.installUpdate()); }}
                  className="w-full px-3 py-1.5 rounded-md bg-claude-bg border border-claude-border text-[13px] text-claude-text font-medium hover:bg-claude-btnHover transition-colors"
                >
                  {t('sidebar.relaunch')}
                </button>
              </div>
            )}
          </div>
        )}

        {/* Search - Fixed at bottom */}
        <div
          className="flex-shrink-0"
          style={{
            marginTop: '2px',
            paddingLeft: '9px',
            paddingRight: '9px',
            marginBottom: '2px'
          }}
        >
          <button
            onClick={() => setShowSearch(true)}
            className="w-full flex items-center justify-start text-claude-text hover:bg-claude-hover rounded-lg transition-colors group overflow-hidden whitespace-nowrap"
            style={{
              paddingTop: '2px',
              paddingBottom: '2px',
              paddingLeft: '0px',
              gap: '8px'
            }}
          >
            <div className={`text-claude-text flex-shrink-0 flex items-center justify-center`} style={{ width: '27px', height: '27px' }}>
              <img
                src={searchIconImg}
                alt="Search"
                style={{ width: '16px', height: '16px' }}
                className="object-contain dark:invert transition-[filter] duration-200"
              />
            </div>
            <span
              className={`leading-none transition-opacity duration-200 text-left ${isCollapsed ? 'opacity-0 w-0 hidden' : 'opacity-100 block'}`}
              style={{ fontSize: '14px', fontWeight: 400 }}
            >
              {t('sidebar.search')}
            </span>
          </button>
        </div>

        {/* User Profile Footer */}
        <div
          className={`${!updateStatus || isCollapsed || (updateStatus.type !== 'available' && updateStatus.type !== 'progress' && updateStatus.type !== 'downloaded') ? 'mt-auto' : ''} border-t border-claude-border flex-shrink-0 relative transition-all duration-200`}
          style={{
            paddingTop: `${tunerConfig?.profilePy || 12}px`,
            paddingBottom: `${tunerConfig?.profilePy || 12}px`,
            paddingLeft: isCollapsed ? '0px' : `${tunerConfig?.profilePx || 12}px`,
            paddingRight: isCollapsed ? '0px' : `${tunerConfig?.profilePx || 12}px`,
          }}
        >
          {!isCollapsed && <CostTracker conversationId={activeConversationId} compact />}
          <button
            ref={userBtnRef}
            onClick={() => {
              if (!showUserMenu && userBtnRef.current) {
                const rect = userBtnRef.current.getBoundingClientRect();
                setUserMenuPos({ bottom: window.innerHeight - rect.top + 4, left: rect.left });
              }
              setShowUserMenu(!showUserMenu);
            }}
            className={`w-full flex items-center gap-2 hover:bg-claude-hover rounded-lg transition-all duration-200 overflow-hidden whitespace-nowrap`}
            style={{
              padding: isCollapsed ? '8px 0px 8px 5px' : '8px'
            }}
          >
            <div
              className="rounded-full bg-claude-avatar text-claude-avatarText flex items-center justify-center text-[15px] font-medium flex-shrink-0"
              style={{ width: `${tunerConfig?.userAvatarSize || 32}px`, height: `${tunerConfig?.userAvatarSize || 32}px` }}
            >
              {(userUser?.display_name || userUser?.full_name || userUser?.nickname || 'U').charAt(0).toUpperCase()}
            </div>
            <div className={`flex items-center justify-between w-full transition-opacity duration-200 ${isCollapsed ? 'opacity-0' : 'opacity-100'}`}>
              <div className="text-left overflow-hidden flex-1 min-w-0">
                <div
                  className="font-medium text-claude-text leading-tight"
                  style={{ fontSize: `${tunerConfig?.userNameSize || 15}px`, whiteSpace: 'nowrap', textOverflow: 'ellipsis', overflow: 'hidden' }}
                >
                  {userUser?.display_name || userUser?.full_name || userUser?.nickname || 'User'}
                </div>
                {localStorage.getItem('user_mode') === 'selfhosted' ? (
                  <div className="text-[13px] text-claude-textSecondary mt-1 leading-tight">{t('sidebar.selfHosted')}</div>
                ) : usageData && (usageData.token_quota > 0 || usageData.quota) ? (
                  <div className="mt-1.5 mr-3">
                    {usageData.quota?.window && usageData.quota.window.limit > 0 ? (
                      <>
                        <div className="h-1 w-full rounded-full bg-claude-hover overflow-hidden">
                          <div
                            className="h-full bg-neutral-700 dark:bg-neutral-300 transition-[width] duration-300"
                            style={{ width: `${Math.min(100, (usageData.quota.window.used / usageData.quota.window.limit) * 100)}%` }}
                          />
                        </div>
                        <div className="text-[10px] text-claude-textSecondary mt-1 leading-none tabular-nums">
                          ${usageData.quota.window.used.toFixed(2)} / ${usageData.quota.window.limit.toFixed(2)}
                        </div>
                      </>
                    ) : usageData.token_quota > 0 ? (
                      <>
                        <div className="h-1 w-full rounded-full bg-claude-hover overflow-hidden">
                          <div
                            className="h-full bg-neutral-700 dark:bg-neutral-300 transition-[width] duration-300"
                            style={{ width: `${Math.min(100, (usageData.token_used / usageData.token_quota) * 100)}%` }}
                          />
                        </div>
                        <div className="text-[10px] text-claude-textSecondary mt-1 leading-none tabular-nums">
                          ${usageData.token_used.toFixed(2)} / ${usageData.token_quota.toFixed(2)}
                        </div>
                      </>
                    ) : (
                      <div className="text-[13px] text-claude-textSecondary leading-tight">{planLabel}</div>
                    )}
                  </div>
                ) : (
                  <div className="text-[13px] text-claude-textSecondary mt-1 leading-tight">{planLabel}</div>
                )}
              </div>
              <ChevronUp size={16} className="text-claude-textSecondary shrink-0 ml-1" />
            </div>
          </button>

          {/* User Menu Popup */}
          {showUserMenu && userMenuPos && (
            <div ref={userMenuRef} className="fixed w-[220px] bg-claude-input border border-claude-border rounded-xl shadow-[0_4px_16px_rgba(0,0,0,0.12)] py-1.5 z-[60]"
              style={{ bottom: `${userMenuPos.bottom}px`, left: `${userMenuPos.left}px` }}
            >
              {/* User info header */}
              <div className="px-4 py-2.5 border-b border-claude-border">
                <div className="text-[13px] font-medium text-claude-text">{userUser?.display_name || userUser?.full_name || userUser?.nickname || 'User'}</div>
                <div className="text-[12px] text-claude-textSecondary mt-0.5">{userUser?.email || ''}</div>
              </div>
              {/* Menu items */}
              <div className="py-1">
                <button
                  onClick={() => { setShowUserMenu(false); onOpenSettings?.(); }}
                  className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                >
                  <Settings size={16} className="text-claude-textSecondary" />
                  {t('sidebar.settings')}
                </button>
                {localStorage.getItem('user_mode') !== 'selfhosted' && (
                  <button
                    className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                    onClick={() => { setShowUserMenu(false); onOpenUpgrade?.(); }}
                  >
                    <CreditCard size={16} className="text-claude-textSecondary" />
                    {t('sidebar.payment')}
                  </button>
                )}
                {isAdmin && localStorage.getItem('user_mode') !== 'selfhosted' && (
                  <button
                    className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                    onClick={() => { setShowUserMenu(false); navigate('/admin'); }}
                  >
                    <Shield size={16} className="text-claude-textSecondary" />
                    {t('sidebar.adminPanel')}
                  </button>
                )}
                <button
                  onClick={() => { setShowUserMenu(false); setShowHelpModal(true); }}
                  className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                >
                  <HelpCircle size={16} className="text-claude-textSecondary" />
                  {t('sidebar.getHelp')}
                </button>
              </div>
              <div className="h-[1px] bg-claude-border mx-3" />
              <div className="py-1">
                <button
                  onClick={() => { setShowUserMenu(false); setShowLogoutConfirm(true); }}
                  className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                >
                  <LogOut size={16} className="text-claude-textSecondary" />
                  {t('sidebar.logout')}
                </button>
              </div>
            </div>
          )}
        </div>
      </div >

      {/* Fixed Context Menu Portal */}
      {
        activeMenuId !== null && menuPosition && activeMenuId.startsWith('chat:') && chats.find(c => c.id === activeMenuId.slice(5)) && (
          <div
            ref={menuRef}
            className="fixed z-50 bg-claude-input border border-claude-border rounded-xl shadow-[0_4px_12px_rgba(0,0,0,0.08)] py-1.5 flex flex-col w-[200px]"
            style={{
              top: `${menuPosition.top}px`,
              left: `${menuPosition.left}px`
            }}
          >
            <button className="flex items-center gap-3 px-3 py-2 hover:bg-claude-hover text-left w-full transition-colors group">
              <IconStarOutline size={16} className="text-claude-textSecondary group-hover:text-claude-text" />
              <span className="text-[13px] text-claude-text">{t('sidebar.star')}</span>
            </button>
            <button
              onClick={(e) => handleRenameClick(e, activeMenuId.slice(5))}
              className="flex items-center gap-3 px-3 py-2 hover:bg-claude-hover text-left w-full transition-colors group"
            >
              <IconPencil size={16} className="text-claude-textSecondary group-hover:text-claude-text" />
              <span className="text-[13px] text-claude-text">{t('sidebar.rename')}</span>
            </button>
            <div className="h-[1px] bg-claude-border my-1 mx-3" />
            <button
              onClick={(e) => handleDeleteChat(activeMenuId.slice(5), e)}
              className="flex items-center gap-3 px-3 py-2 hover:bg-claude-hover text-left w-full transition-colors group"
            >
              <IconTrash size={16} className="text-[#B9382C]" />
              <span className="text-[13px] text-[#B9382C]">{t('sidebar.delete')}</span>
            </button>
          </div>
        )
      }

      {/* Project Card Context Menu Portal */}
      {
        activeMenuId !== null && menuPosition && activeMenuId.startsWith('project:') && (() => {
          const projectId = activeMenuId.slice(8);
          const project = projects.find((p: any) => p.id === projectId);
          if (!project) return null;
          return (
            <div
              ref={menuRef}
              className="fixed z-50 bg-claude-input border border-claude-border rounded-xl shadow-[0_4px_12px_rgba(0,0,0,0.08)] py-1.5 flex flex-col w-[220px]"
              style={{
                top: `${menuPosition.top}px`,
                left: `${menuPosition.left}px`
              }}
            >
              <button
                onClick={() => handleOpenProjectFolder(project)}
                className="flex items-center gap-3 px-3 py-2 hover:bg-claude-hover text-left w-full transition-colors group"
              >
                <IconDirectory size={16} className="text-claude-textSecondary group-hover:text-claude-text" />
                <span className="text-[13px] text-claude-text">在资源管理器打开</span>
              </button>
              <div className="h-[1px] bg-claude-border my-1 mx-3" />
              <button
                onClick={() => handleDeleteProjectCard(project)}
                className="flex items-center gap-3 px-3 py-2 hover:bg-claude-hover text-left w-full transition-colors group"
              >
                <IconTrash size={16} className="text-[#B9382C]" />
                <span className="text-[13px] text-[#B9382C]">删除项目</span>
              </button>
            </div>
          );
        })()
      }

      {/* Fixed Layout Tuner (Removed) */}

      <SearchModal
        isOpen={showSearch}
        onClose={() => setShowSearch(false)}
        chats={chats}
      />

      {/* Embedded Browser Panel - Slides in from the right */}
      {showBrowser && !isCollapsed && (
        <div
          className="fixed z-40 border-l border-claude-border bg-claude-sidebar flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{
            top: `${titleBarHeight || 44}px`,
            left: `${tunerConfig?.sidebarWidth || 280}px`,
            width: 'min(700px, 60vw)',
            height: `calc(100vh - ${titleBarHeight || 44}px)`,
            borderRadius: '0 12px 12px 0'
          }}
        >
          <BrowserLivePanel
            onClose={() => setShowBrowser(false)}
            className="flex-1"
          />
        </div>
      )}

      {/* Rename Modal */}
      <RenameModal
        isOpen={showRenameModal}
        onClose={() => {
          setShowRenameModal(false);
          setRenameChatId(null);
        }}
        onSave={handleRenameSubmit}
        initialTitle={renameInitialTitle}
      />

      {/* Logout Confirmation Modal */}
      {showLogoutConfirm && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40">
          <div className="bg-claude-input rounded-2xl shadow-xl w-[360px] p-6">
            <h3 className="text-[16px] font-semibold text-claude-text mb-2">{t('sidebar.logoutConfirm')}</h3>
            <p className="text-[14px] text-claude-textSecondary mb-6">{t('sidebar.logoutMessage')}</p>
            <div className="flex justify-end gap-3">
              <button
                onClick={() => setShowLogoutConfirm(false)}
                className="px-4 py-2 text-[13px] text-claude-text bg-claude-btn-hover hover:bg-claude-hover rounded-lg transition-colors"
              >
                {t('sidebar.cancelLogout')}
              </button>
              <button
                onClick={() => { setShowLogoutConfirm(false); logout(); }}
                className="px-4 py-2 text-[13px] text-white bg-[#B9382C] hover:bg-[#A02E23] rounded-lg transition-colors"
              >
                {t('sidebar.confirmLogout')}
              </button>
            </div>
          </div>
        </div>
      )}

      {showHelpModal && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40" onClick={() => setShowHelpModal(false)}>
          <div
            className="bg-claude-input rounded-2xl shadow-xl w-[360px] p-6"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-[16px] font-semibold text-claude-text mb-2">{t('sidebar.supportTitle')}</h3>
            <p className="text-[14px] text-claude-textSecondary mb-3">{t('sidebar.supportQQLabel')}</p>
            <div className="px-4 py-3 mb-6 rounded-xl bg-claude-btn-hover text-[20px] font-semibold tracking-wide text-claude-text text-center select-all">
              629466903
            </div>
            <div className="flex justify-end">
              <button
                onClick={() => setShowHelpModal(false)}
                className="px-4 py-2 text-[13px] text-claude-text bg-claude-btn-hover hover:bg-claude-hover rounded-lg transition-colors"
              >
                {t('sidebar.close')}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
};

export default Sidebar;
