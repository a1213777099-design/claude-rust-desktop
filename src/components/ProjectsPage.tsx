import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { Search, Plus, ChevronDown, ArrowLeft, MoreVertical, Star, FileText, Trash, Pencil, X, Check, Archive } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { getProjects, createProject, getProject, updateProject, deleteProject, deleteProjectFile, createProjectConversation, getProjectConversations, Project, ProjectFile } from '../api';
import { tauriAPI } from '../utils/tauriAPI';
import startProjectsImg from '../assets/icons/start-projects.png';
import { useI18n } from '../hooks/useI18n';

const ProjectsPage = () => {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [searchQuery, setSearchQuery] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [projectName, setProjectName] = useState('');
  const [projectDescription, setProjectDescription] = useState('');
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [currentProject, setCurrentProject] = useState<any>(null);
  const [editingInstructions, setEditingInstructions] = useState(false);
  const [instructionsText, setInstructionsText] = useState('');
  const [editingName, setEditingName] = useState(false);
  const [editName, setEditName] = useState('');
  const [sortMenuOpen, setSortMenuOpen] = useState(false);
  const [sortBy, setSortBy] = useState<'activity' | 'edited' | 'created'>('activity');
  const [activeMenu, setActiveMenu] = useState<string | null>(null);
  const [projectToDelete, setProjectToDelete] = useState<Project | null>(null);
  const [projectToEdit, setProjectToEdit] = useState<Project | null>(null);
  const [editDetailsName, setEditDetailsName] = useState('');
  const [editDetailsDesc, setEditDetailsDesc] = useState('');
  const [showDetailMenu, setShowDetailMenu] = useState(false);
  const [editingWorkspace, setEditingWorkspace] = useState(false);

  // Create project page states
  const [createInstructionsText, setCreateInstructionsText] = useState('');
  const [createWorkspacePath, setCreateWorkspacePath] = useState('');
  const [createEditingInstructions, setCreateEditingInstructions] = useState(false);

  const loadProjects = useCallback(async () => {
    try {
      const data = await getProjects();
      setProjects(Array.isArray(data) ? data : []);
    } catch (err) {
      console.error('Failed to load projects:', err);
      setProjects([]);
    }
    setLoading(false);
  }, []);

  useEffect(() => { loadProjects(); }, [loadProjects]);

  useEffect(() => {
    if (!showDetailMenu) return;
    const handleClick = () => setShowDetailMenu(false);
    document.addEventListener('click', handleClick);
    return () => document.removeEventListener('click', handleClick);
  }, [showDetailMenu]);

  const loadProject = useCallback(async (id: string) => {
    try {
      const data = await getProject(id);
      setCurrentProject(data);
      setInstructionsText(data.instructions || '');
    } catch (_) { }
  }, []);

  const handleCreate = async () => {
    const name = projectName.trim() || 'Untitled Project';
    try {
      const project = await createProject(name, projectDescription.trim(), createWorkspacePath || undefined);
      // Save instructions if provided
      if (createInstructionsText.trim()) {
        await updateProject(project.id, { instructions: createInstructionsText.trim() });
      }
      setIsCreating(false);
      setProjectName('');
      setProjectDescription('');
      setCreateInstructionsText('');
      setCreateWorkspacePath('');

      // Create conversation and navigate to main chat
      const defaultModel = localStorage.getItem('default_model') || 'claude-sonnet-4-6';
      const conv = await createProjectConversation(
        project.id,
        name,
        defaultModel,
        createWorkspacePath || undefined
      );
      // Notify sidebar to refresh project cards and conversations
      window.dispatchEvent(new CustomEvent('conversationsUpdated'));
      navigate(`/chat/${conv.id}`);
      loadProjects();
    } catch (_) { }
  };

  const handleCreateProjectWithFolder = async () => {
    try {
      const selectedPath = await tauriAPI.selectDirectory();
      if (!selectedPath) return;

      const folderName = selectedPath.split(/[\\/]/).pop() || 'Untitled Project';
      const project = await createProject(folderName, '', selectedPath);
      const defaultModel = localStorage.getItem('default_model') || 'claude-sonnet-4-6';
      const conv = await createProjectConversation(project.id, folderName, defaultModel, selectedPath);
      // Notify sidebar to refresh project cards and conversations
      window.dispatchEvent(new CustomEvent('conversationsUpdated'));
      navigate(`/chat/${conv.id}`);
      loadProjects();
    } catch (err) {
      console.error('Failed to create project with folder:', err);
      alert('创建项目失败: ' + (err instanceof Error ? err.message : '未知错误'));
    }
  };

  const handleDelete = async () => {
    if (!currentProject) return;
    if (!window.confirm(t('customize.confirmDeleteProjectMsg', { name: currentProject.name }))) return;
    try {
      await deleteProject(currentProject.id);
      setCurrentProject(null);
      window.dispatchEvent(new CustomEvent('conversationsUpdated'));
      loadProjects();
    } catch (_) { }
  };

  const handleDeleteProject = async (p: Project) => {
    try {
      await deleteProject(p.id);
      if (currentProject && currentProject.id === p.id) {
        setCurrentProject(null);
      }
      window.dispatchEvent(new CustomEvent('conversationsUpdated'));
      setProjectToDelete(null);
      loadProjects();
    } catch (_) { }
  };

  const handleSaveEditDetails = async () => {
    if (!projectToEdit) return;
    try {
      await updateProject(projectToEdit.id, {
        name: editDetailsName,
        description: editDetailsDesc
      });
      setProjectToEdit(null);
      loadProjects();
      if (currentProject && currentProject.id === projectToEdit.id) {
        loadProject(currentProject.id);
      }
    } catch (_) { }
  };

  const handleSaveInstructions = async () => {
    if (!currentProject) return;
    await updateProject(currentProject.id, { instructions: instructionsText });
    setEditingInstructions(false);
    setCurrentProject(null);
    loadProjects();
  };

  const handleSelectFolder = async () => {
    if (!currentProject) return;
    try {
      const selectedPath = await tauriAPI.selectDirectory();
      if (!selectedPath) return;
      await updateProject(currentProject.id, { workspace_path: selectedPath });
      loadProject(currentProject.id);
    } catch (err) {
      console.error('Failed to select folder:', err);
    }
  };

  const handleDeleteFile = async (fileId: string) => {
    if (!currentProject) return;
    await deleteProjectFile(currentProject.id, fileId);
    loadProject(currentProject.id);
  };

  const handleNewChat = async () => {
    if (!currentProject) return;
    try {
      const conv = await createProjectConversation(currentProject.id, undefined, undefined, currentProject.workspace_path);
      navigate(`/chat/${conv.id}`);
    } catch (_) { }
  };

  const handleRenameSave = async () => {
    if (!currentProject || !editName.trim()) return;
    await updateProject(currentProject.id, { name: editName.trim() });
    setEditingName(false);
    loadProject(currentProject.id);
    loadProjects();
  };

  const filteredProjects = useMemo(() => {
    const filtered = projects.filter(p =>
      (p.name || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
      (p.description || '').toLowerCase().includes(searchQuery.toLowerCase())
    );
    return [...filtered].sort((a, b) => {
      if (sortBy === 'created') return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
      return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
    });
  }, [projects, searchQuery, sortBy]);

  if (currentProject) {
    return (
      <div className="flex-1 h-full bg-claude-bg overflow-y-auto">
        <div className="max-w-[800px] mx-auto px-8 py-12">
          <div className="mb-4">
            <button
              onClick={() => { setCurrentProject(null); loadProjects(); }}
              className="flex items-center gap-1.5 text-[14px] text-claude-textSecondary hover:text-claude-text transition-colors font-medium -ml-1"
            >
              <ArrowLeft size={16} />
              {t('customize.allProjects')}
            </button>
          </div>

          <div className="flex items-start justify-between mb-8 gap-4">
            <div className="flex-1 min-w-0">
              {editingName ? (
                <div className="flex items-center gap-2">
                  <input
                    autoFocus
                    value={editName}
                    onChange={e => setEditName(e.target.value)}
                    onKeyDown={e => { if (e.key === 'Enter') handleRenameSave(); if (e.key === 'Escape') setEditingName(false); }}
                    className="font-[Spectral] text-[32px] text-claude-text bg-transparent border-b-2 border-claude-accent outline-none w-full"
                    style={{ fontWeight: 500 }}
                  />
                </div>
              ) : (
                <h1
                  className="font-[Spectral] text-[32px] text-claude-text leading-tight mb-2"
                  style={{ fontWeight: 500 }}
                >
                  {currentProject.name}
                </h1>
              )}
              {currentProject.description && (
                <p className="text-[15.5px] text-claude-textSecondary">{currentProject.description}</p>
              )}
            </div>
            <div className="relative flex items-center gap-1 text-claude-textSecondary mt-2 flex-shrink-0">
              <button
                onClick={() => setShowDetailMenu(!showDetailMenu)}
                className="p-1 hover:text-claude-text hover:bg-black/5 dark:hover:bg-white/5 rounded-md transition-colors"
              >
                <MoreVertical size={18} />
              </button>
              {showDetailMenu && (
                <div className="absolute right-0 top-full mt-1 z-50 bg-claude-input border border-claude-border rounded-xl shadow-[0_4px_12px_rgba(0,0,0,0.08)] py-1.5 w-[200px]">
                  <button
                    onClick={() => {
                      setShowDetailMenu(false);
                      setEditDetailsName(currentProject.name || '');
                      setEditDetailsDesc(currentProject.description || '');
                      setProjectToEdit(currentProject);
                    }}
                    className="flex items-center gap-3 px-3 py-2 hover:bg-claude-hover text-left w-full transition-colors group"
                  >
                    <Pencil size={16} className="text-claude-textSecondary group-hover:text-claude-text" />
                    <span className="text-[13px] text-claude-text">{t('sidebar.editDetails') || 'Edit Details'}</span>
                  </button>
                  <button
                    onClick={async () => {
                      setShowDetailMenu(false);
                      const selectedPath = await tauriAPI.selectDirectory();
                      if (selectedPath && currentProject) {
                        await updateProject(currentProject.id, { workspace_path: selectedPath });
                        loadProject(currentProject.id);
                      }
                    }}
                    className="flex items-center gap-3 px-3 py-2 hover:bg-claude-hover text-left w-full transition-colors group"
                  >
                    <FileText size={16} className="text-claude-textSecondary group-hover:text-claude-text" />
                    <span className="text-[13px] text-claude-text">{t('customize.changeWorkspace') || 'Change Workspace'}</span>
                  </button>
                  <button
                    onClick={() => {
                      setShowDetailMenu(false);
                      setEditingInstructions(true);
                      setInstructionsText(currentProject.instructions || '');
                    }}
                    className="flex items-center gap-3 px-3 py-2 hover:bg-claude-hover text-left w-full transition-colors group"
                  >
                    <Plus size={16} className="text-claude-textSecondary group-hover:text-claude-text" />
                    <span className="text-[13px] text-claude-text">{t('customize.addProjectInstructions') || 'Add Instructions'}</span>
                  </button>
                  <div className="h-[1px] bg-claude-border my-1 mx-3" />
                  <button
                    onClick={async () => {
                      setShowDetailMenu(false);
                      if (!window.confirm(t('customize.confirmDeleteProjectMsg', { name: currentProject.name }) || `Delete "${currentProject.name}"?`)) return;
                      await deleteProject(currentProject.id);
                      setCurrentProject(null);
                      loadProjects();
                    }}
                    className="flex items-center gap-3 px-3 py-2 hover:bg-claude-hover text-left w-full transition-colors group"
                  >
                    <Trash size={16} className="text-[#B9382C]" />
                    <span className="text-[13px] text-[#B9382C]">{t('sidebar.delete')}</span>
                  </button>
                </div>
              )}
            </div>
          </div>

          <div className="space-y-4">
            <div className="w-full border border-claude-border rounded-[16px] overflow-hidden bg-transparent mt-2">
              <div
                className="p-5 border-b border-claude-border hover:bg-black/[0.015] dark:hover:bg-white/[0.015] transition-colors cursor-pointer group"
                onClick={() => { if (!editingInstructions) setEditingInstructions(true); }}
              >
                <div className="flex items-center justify-between">
                  <div className="flex-1">
                    <h3 className="font-semibold text-claude-text mb-0.5" style={{ fontSize: '15.5px' }}>{t('customize.instructions')}</h3>
                    {!editingInstructions && (
                      <p className="text-[13px] text-[#A1A1AA]">
                        {currentProject.instructions
                          ? currentProject.instructions.slice(0, 200) + (currentProject.instructions.length > 200 ? '...' : '')
                          : t('customize.addInstructions')}
                      </p>
                    )}
                  </div>
                  {!editingInstructions && (
                    <button className="text-[#A1A1AA] hover:text-claude-text transition-colors">
                      {currentProject.instructions ? <Pencil size={18} strokeWidth={1.5} /> : <Plus size={22} strokeWidth={1.5} />}
                    </button>
                  )}
                </div>
                {editingInstructions && (
                  <div
                    className="fixed inset-0 z-50 flex items-center justify-center bg-black/70"
                    onClick={() => { setEditingInstructions(false); setCurrentProject(null); loadProjects(); }}
                  >
                    <div
                      className="w-full max-w-[800px] bg-white dark:bg-[#2A2928] border border-claude-border rounded-[20px] shadow-2xl p-7"
                      onClick={e => e.stopPropagation()}
                    >
                      <h2 className="text-[20px] font-bold text-claude-text mb-2">{t('customize.setInstructionsTitle')}</h2>
                      <p className="text-[14px] text-[#A1A1AA] mb-5" dangerouslySetInnerHTML={{ __html: t('customize.setInstructionsDesc', { name: currentProject.name }) }} />

                      <textarea
                        autoFocus
                        value={instructionsText}
                        onChange={e => setInstructionsText(e.target.value)}
                        placeholder={t('customize.instructionsPlaceholder')}
                        className="w-full h-[400px] px-4 py-3 bg-claude-bg dark:bg-[#202020] border border-claude-border rounded-[12px] text-[15px] text-claude-text resize-none outline-none focus:border-[#3A7ADA] focus:ring-1 focus:ring-[#3A7ADA] transition-colors"
                      />

                      <div className="flex justify-end gap-3 mt-5">
                        <button
                          onClick={() => { setEditingInstructions(false); setCurrentProject(null); loadProjects(); }}
                          className="px-4 py-2 text-[14px] font-medium text-claude-text hover:bg-white/5 border border-transparent hover:border-claude-border rounded-xl transition-all"
                        >
                          {t('customize.cancelBtn')}
                        </button>
                        <button
                          onClick={handleSaveInstructions}
                          className="px-4 py-2 text-[14px] font-medium bg-[#E6E6E6] text-[#222] rounded-xl hover:opacity-90 transition-opacity"
                        >
                          {t('customize.saveInstructionsBtn')}
                        </button>
                      </div>
                    </div>
                  </div>
                )}
              </div>

              <div className="p-5 pb-6">
                <div className="flex items-center justify-between mb-4">
                  <h3 className="font-semibold text-claude-text" style={{ fontSize: '15.5px' }}>
                    {t('customize.files')} {currentProject.files?.length > 0 && <span className="text-claude-textSecondary text-[13px] ml-1">({currentProject.files.length})</span>}
                  </h3>
                  <button
                    onClick={handleSelectFolder}
                    className="text-[#A1A1AA] hover:text-claude-text transition-colors"
                    title="选择工作区文件夹"
                  >
                    <Plus size={22} strokeWidth={1.5} />
                  </button>
                </div>

                {currentProject.files && currentProject.files.length > 0 ? (
                  <div className="space-y-2">
                    {currentProject.files.map((f: ProjectFile) => (
                      <div key={f.id} className="flex items-center gap-3 px-3 py-2.5 rounded-[12px] bg-black/[0.02] dark:bg-white/[0.03] group border border-transparent hover:border-claude-border transition-all">
                        <FileText size={16} className="text-[#A1A1AA] flex-shrink-0" />
                        <div className="flex-1 min-w-0">
                          <div className="text-[13.5px] text-claude-text truncate font-medium">{f.file_name}</div>
                          <div className="text-[11.5px] text-[#A1A1AA]">
                            {f.file_size > 1024 * 1024 ? `${(f.file_size / 1024 / 1024).toFixed(1)} MB` : `${(f.file_size / 1024).toFixed(1)} KB`}
                          </div>
                        </div>
                        <button
                          onClick={() => handleDeleteFile(f.id)}
                          className="p-1 text-[#A1A1AA] hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity"
                        >
                          <X size={16} />
                        </button>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div
                    className="w-full bg-[#FAFAFA] dark:bg-[#191919] rounded-[16px] flex flex-col items-center justify-center py-8 border border-transparent dark:border-white/[0.04] cursor-pointer hover:bg-[#F3F3F3] dark:hover:bg-[#222222] transition-colors"
                    onClick={handleSelectFolder}
                  >
                    <div className="flex items-center justify-center mb-3">
                      <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="text-[#A1A1AA]">
                        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                      </svg>
                    </div>
                    <span className="text-[13px] text-[#A1A1AA] text-center max-w-[200px] leading-relaxed">
                      点击选择工作区文件夹
                    </span>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (isCreating) {
    return (
      <div className="flex-1 h-full bg-claude-bg overflow-y-auto">
        <div className="max-w-[560px] mx-auto px-8 pt-12 pb-8">
          <h1 className="font-[Spectral] text-[32px] text-claude-text mb-6" style={{ fontWeight: 600 }}>
            {t('customize.createProjectTitle')}
          </h1>

          <div className="bg-[#EFEEE7] dark:bg-[#2A2928] rounded-2xl p-6 mb-6 border border-transparent dark:border-white/5">
            <h3 className="font-semibold text-claude-text text-[15.5px] mb-2 text-[#403A35] dark:text-[#E3E0D8]">{t('customize.howToUseProjects')}</h3>
            <p className="text-[14.5px] leading-relaxed text-[#564E48] dark:text-[#A8A096] mb-3">
              {t('customize.projectsHelp1')}
            </p>
            <p className="text-[14.5px] leading-relaxed text-[#564E48] dark:text-[#A8A096]">
              {t('customize.projectsHelp2')}
            </p>
          </div>

          <div className="space-y-5">
            <div>
              <label className="block text-[15px] font-medium text-claude-textSecondary mb-2">{t('customize.projectNameLabel')}</label>
              <input
                type="text"
                placeholder={t('customize.projectNamePlaceholder')}
                value={projectName}
                onChange={e => setProjectName(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter' && projectName.trim()) handleCreate(); }}
                className="w-full px-4 py-3 bg-white dark:bg-claude-input border border-gray-200 dark:border-claude-border rounded-xl text-claude-text placeholder-gray-400 dark:placeholder-gray-500 focus:outline-none focus:border-[#387ee0] focus:ring-0 transition-all text-[15px]"
              />
            </div>
            <div>
              <label className="block text-[15px] font-medium text-claude-textSecondary mb-2">{t('customize.projectGoalLabel')}</label>
              <textarea
                placeholder={t('customize.projectGoalPlaceholder')}
                rows={3}
                value={projectDescription}
                onChange={e => setProjectDescription(e.target.value)}
                className="w-full px-4 py-3 bg-white dark:bg-claude-input border border-gray-200 dark:border-claude-border rounded-xl text-claude-text placeholder-gray-400 dark:placeholder-gray-500 focus:outline-none focus:border-[#387ee0] focus:ring-0 transition-all text-[15px] resize-none"
              />
            </div>
          </div>

          {/* Instructions Section */}
          <div className="w-full border border-claude-border rounded-[16px] overflow-hidden bg-transparent mt-6">
            <div
              className="p-5 border-b border-claude-border hover:bg-black/[0.015] dark:hover:bg-white/[0.015] transition-colors cursor-pointer group"
              onClick={() => { if (!createEditingInstructions) setCreateEditingInstructions(true); }}
            >
              <div className="flex items-center justify-between">
                <div className="flex-1">
                  <h3 className="font-semibold text-claude-text mb-0.5" style={{ fontSize: '15.5px' }}>{t('customize.instructions')}</h3>
                  {!createEditingInstructions && (
                    <p className="text-[13px] text-[#A1A1AA]">
                      {createInstructionsText
                        ? createInstructionsText.slice(0, 200) + (createInstructionsText.length > 200 ? '...' : '')
                        : t('customize.addInstructions')}
                    </p>
                  )}
                </div>
                {!createEditingInstructions && (
                  <button className="text-[#A1A1AA] hover:text-claude-text transition-colors">
                    {createInstructionsText ? <Pencil size={18} strokeWidth={1.5} /> : <Plus size={22} strokeWidth={1.5} />}
                  </button>
                )}
              </div>
              {createEditingInstructions && (
                <div
                  className="fixed inset-0 z-50 flex items-center justify-center bg-black/70"
                  onClick={() => { setCreateEditingInstructions(false); }}
                >
                  <div
                    className="w-full max-w-[800px] bg-white dark:bg-[#2A2928] border border-claude-border rounded-[20px] shadow-2xl p-7"
                    onClick={e => e.stopPropagation()}
                  >
                    <h2 className="text-[20px] font-bold text-claude-text mb-2">{t('customize.setInstructionsTitle')}</h2>
                    <p className="text-[14px] text-[#A1A1AA] mb-5">{t('customize.setInstructionsDesc', { name: projectName || t('customize.newProject') })}</p>

                    <textarea
                      autoFocus
                      value={createInstructionsText}
                      onChange={e => setCreateInstructionsText(e.target.value)}
                      placeholder={t('customize.instructionsPlaceholder')}
                      className="w-full h-[400px] px-4 py-3 bg-claude-bg dark:bg-[#202020] border border-claude-border rounded-[12px] text-[15px] text-claude-text resize-none outline-none focus:border-[#3A7ADA] focus:ring-1 focus:ring-[#3A7ADA] transition-colors"
                    />

                    <div className="flex justify-end gap-3 mt-5">
                      <button
                        onClick={() => { setCreateEditingInstructions(false); }}
                        className="px-4 py-2 text-[14px] font-medium text-claude-text hover:bg-white/5 border border-transparent hover:border-claude-border rounded-xl transition-all"
                      >
                        {t('customize.cancelBtn')}
                      </button>
                      <button
                        onClick={() => setCreateEditingInstructions(false)}
                        className="px-4 py-2 text-[14px] font-medium bg-[#E6E6E6] text-[#222] rounded-xl hover:opacity-90 transition-opacity"
                      >
                        {t('customize.saveInstructionsBtn')}
                      </button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Workspace Folder Section */}
          <div className="w-full border border-claude-border rounded-[16px] overflow-hidden bg-transparent mt-4">
            <div className="p-5 pb-6">
              <div className="flex items-center justify-between mb-4">
                <h3 className="font-semibold text-claude-text" style={{ fontSize: '15.5px' }}>
                  {t('customize.files')}
                </h3>
                {createWorkspacePath ? (
                  <button
                    onClick={() => setCreateWorkspacePath('')}
                    className="text-[#A1A1AA] hover:text-red-500 transition-colors"
                    title="清除选择"
                  >
                    <X size={18} strokeWidth={1.5} />
                  </button>
                ) : (
                  <button
                    onClick={async () => {
                      try {
                        const selectedPath = await tauriAPI.selectDirectory();
                        if (selectedPath) setCreateWorkspacePath(selectedPath);
                      } catch (err) {
                        console.error('Failed to select folder:', err);
                      }
                    }}
                    className="text-[#A1A1AA] hover:text-claude-text transition-colors"
                    title="选择工作区文件夹"
                  >
                    <Plus size={22} strokeWidth={1.5} />
                  </button>
                )}
              </div>

              {createWorkspacePath ? (
                <div className="flex items-center gap-3 px-3 py-2.5 rounded-[12px] bg-black/[0.02] dark:bg-white/[0.03] border border-claude-border">
                  <FileText size={16} className="text-[#A1A1AA] flex-shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-[13.5px] text-claude-text truncate font-medium">{createWorkspacePath.split(/[\\/]/).pop()}</div>
                    <div className="text-[11.5px] text-[#A1A1AA] truncate">{createWorkspacePath}</div>
                  </div>
                </div>
              ) : (
                <div
                  className="w-full bg-[#FAFAFA] dark:bg-[#191919] rounded-[16px] flex flex-col items-center justify-center py-8 border border-transparent dark:border-white/[0.04] cursor-pointer hover:bg-[#F3F3F3] dark:hover:bg-[#222222] transition-colors"
                  onClick={async () => {
                    try {
                      const selectedPath = await tauriAPI.selectDirectory();
                      if (selectedPath) setCreateWorkspacePath(selectedPath);
                    } catch (err) {
                      console.error('Failed to select folder:', err);
                    }
                  }}
                >
                  <div className="flex items-center justify-center mb-3">
                    <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="text-[#A1A1AA]">
                      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                    </svg>
                  </div>
                  <span className="text-[13px] text-[#A1A1AA] text-center max-w-[200px] leading-relaxed">
                    点击选择工作区文件夹
                  </span>
                </div>
              )}
            </div>
          </div>

          <div className="flex items-center justify-end gap-3 mt-6">
            <button
              onClick={() => { setIsCreating(false); setProjectName(''); setProjectDescription(''); setCreateInstructionsText(''); setCreateWorkspacePath(''); }}
              className="px-5 py-2.5 text-[15px] font-medium text-claude-text bg-white dark:bg-claude-bg border border-gray-300 dark:border-claude-border hover:bg-gray-50 dark:hover:bg-claude-hover rounded-xl transition-colors"
            >
              {t('common.cancel')}
            </button>
            <button
              onClick={handleCreate}
              disabled={!projectName.trim()}
              className="px-5 py-2.5 text-[15px] font-medium text-claude-bg bg-black dark:bg-white dark:text-black hover:opacity-90 rounded-xl transition-opacity disabled:opacity-40"
            >
              {t('customize.createProjectBtn')}
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 h-full bg-claude-bg overflow-y-auto">
      <div className="max-w-[800px] mx-auto px-8 py-12">
        <div className="flex items-center justify-between mb-8">
          <h1 className="font-[Spectral] text-[32px] text-claude-text" style={{ fontWeight: 500 }}>{t('projects.title')}</h1>
          {projects.length > 0 && (
            <button
              onClick={() => setIsCreating(true)}
              className="flex items-center gap-2 px-3.5 py-1.5 bg-claude-text text-claude-bg hover:opacity-90 rounded-lg transition-opacity font-medium"
              style={{ fontSize: '14px' }}
            >
              <Plus size={16} strokeWidth={2.5} />
              {t('customize.newProject')}
            </button>
          )}
        </div>

        {projects.length > 0 && (
          <>
            <div className="relative mb-6">
              <div className="absolute inset-y-0 left-3 flex items-center pointer-events-none">
                <Search className="h-5 w-5 text-claude-textSecondary opacity-80" />
              </div>
              <input
                type="text"
                placeholder={t('customize.searchProjects')}
                value={searchQuery}
                onChange={e => setSearchQuery(e.target.value)}
                className="w-full pl-10 pr-4 py-3 bg-white dark:bg-claude-input border border-gray-200 dark:border-claude-border rounded-xl text-claude-text placeholder-claude-textSecondary focus:outline-none focus:ring-2 focus:ring-blue-500/20 focus:border-blue-500 transition-all text-[15px]"
              />
            </div>

            <div className="flex justify-end mb-6">
              <div className="flex items-center gap-3 text-[14.5px] text-[#A1A1AA] relative">
                <span>{t('customize.sortBy')}</span>
                <button
                  onClick={() => setSortMenuOpen(!sortMenuOpen)}
                  className={`flex items-center gap-2 text-claude-text border border-[#3A3A3A] hover:border-[#4A4A4A] dark:border-claude-border dark:hover:bg-claude-hover rounded-[10px] px-3.5 py-1.5 transition-colors ${sortMenuOpen ? 'bg-claude-hover' : ''}`}
                >
                  {sortBy === 'activity' ? t('customize.recentActivity') : sortBy === 'edited' ? t('customize.lastEdited') : t('customize.dateCreated')}
                  <ChevronDown size={14} className="text-claude-textSecondary" />
                </button>
                {sortMenuOpen && (
                  <>
                    <div className="fixed inset-0 z-40" onClick={() => setSortMenuOpen(false)} />
                    <div className="absolute top-full right-0 mt-1.5 w-[200px] bg-white dark:bg-[#2A2928] border border-gray-200 dark:border-claude-border rounded-[14px] shadow-lg py-1.5 z-50">
                      {[
                        { id: 'activity', label: t('customize.recentActivity') },
                        { id: 'edited', label: t('customize.lastEdited') },
                        { id: 'created', label: t('customize.dateCreated') },
                      ].map(opt => (
                        <button
                          key={opt.id}
                          onClick={() => {
                            setSortBy(opt.id as any);
                            setSortMenuOpen(false);
                          }}
                          className="w-full flex items-center justify-between px-4 py-2.5 text-[15px] text-claude-text hover:bg-black/5 dark:hover:bg-white/5 transition-colors"
                        >
                          {opt.label}
                          {sortBy === opt.id && <Check size={16} className="text-claude-text opacity-80" />}
                        </button>
                      ))}
                    </div>
                  </>
                )}
              </div>
            </div>
          </>
        )}

        {loading ? (
          <div className="text-center text-claude-textSecondary text-[14px] mt-12">{t('customize.loadingProjects')}</div>
        ) : filteredProjects.length > 0 ? (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {filteredProjects.map(p => (
              <div
                key={p.id}
                onClick={async () => {
                  try {
                    // 先获取项目已有会话，如果有则跳转，没有才创建
                    const response = await getProjectConversations(p.id);
                    const existingConvs = response?.conversations || [];
                    if (Array.isArray(existingConvs) && existingConvs.length > 0) {
                      navigate(`/chat/${existingConvs[0].id}`);
                    } else {
                      const defaultModel = localStorage.getItem('default_model') || 'claude-sonnet-4-6';
                      const conv = await createProjectConversation(p.id, p.name, defaultModel, p.workspace_path);
                      navigate(`/chat/${conv.id}`);
                    }
                  } catch (err) {
                    console.error('Failed to navigate to project conversation:', err);
                  }
                }}
                className="flex flex-col p-5 border border-claude-border rounded-[12px] bg-transparent hover:bg-black/[0.02] dark:hover:bg-white/[0.02] cursor-pointer transition-colors group min-h-[170px]"
              >
                <div className="flex items-center justify-between mb-2.5 relative">
                  <div className="flex items-center gap-3">
                    <h3 className="text-[15.5px] font-medium text-claude-text truncate">{p.name}</h3>
                  </div>
                  <div className="relative" onClick={(e) => e.stopPropagation()}>
                    <button
                      onClick={(e) => { e.stopPropagation(); setActiveMenu(activeMenu === p.id ? null : p.id); }}
                      className={`p-1 text-[#A1A1AA] hover:text-claude-text hover:bg-black/5 dark:hover:bg-white/5 rounded-[6px] transition-all ${activeMenu === p.id ? 'opacity-100 bg-black/5 dark:bg-white/5' : 'opacity-0 group-hover:opacity-100'}`}
                    >
                      <MoreVertical size={18} />
                    </button>

                    {activeMenu === p.id && (
                      <>
                        <div className="fixed inset-0 z-40" onClick={(e) => { e.stopPropagation(); setActiveMenu(null); }} />
                        <div className="absolute top-full right-0 mt-1 w-[180px] bg-white dark:bg-[#30302E] rounded-[16px] shadow-[0_4px_24px_rgba(0,0,0,0.15)] border border-gray-200 dark:border-[#65645F] py-1.5 z-50">
                          <button className="w-full flex items-center gap-3 px-4 py-2.5 text-[14px] text-claude-text hover:bg-black/5 dark:hover:bg-white/5 transition-colors text-left" onClick={(e) => { e.stopPropagation(); setActiveMenu(null); }}>
                            <Star size={16} className="text-claude-textSecondary" />
                            {t('customize.star')}
                          </button>
                          <button className="w-full flex items-center gap-3 px-4 py-2.5 text-[14px] text-claude-text hover:bg-black/5 dark:hover:bg-white/5 transition-colors text-left" onClick={(e) => { e.stopPropagation(); setActiveMenu(null); setProjectToEdit(p); setEditDetailsName(p.name); setEditDetailsDesc(p.description || ''); }}>
                            <Pencil size={16} className="text-claude-textSecondary" />
                            {t('customize.editDetails')}
                          </button>
                          <button className="w-full flex items-center gap-3 px-4 py-2.5 text-[14px] text-claude-text hover:bg-black/5 dark:hover:bg-white/5 transition-colors text-left" onClick={async (e) => {
                            e.stopPropagation(); setActiveMenu(null);
                            const selectedPath = await tauriAPI.selectDirectory();
                            if (selectedPath) {
                              await updateProject(p.id, { workspace_path: selectedPath });
                              loadProjects();
                            }
                          }}>
                            <FileText size={16} className="text-claude-textSecondary" />
                            {t('customize.changeWorkspace') || 'Change Workspace'}
                          </button>
                          <button className="w-full flex items-center gap-3 px-4 py-2.5 text-[14px] text-claude-text hover:bg-black/5 dark:hover:bg-white/5 transition-colors text-left" onClick={async (e) => {
                            e.stopPropagation(); setActiveMenu(null);
                            const proj = await getProject(p.id);
                            setCurrentProject(proj);
                            setEditingInstructions(true);
                            setInstructionsText(proj.instructions || '');
                          }}>
                            <Plus size={16} className="text-claude-textSecondary" />
                            {t('customize.addProjectInstructions') || 'Add Instructions'}
                          </button>
                          <div className="my-1.5 border-t border-claude-border opacity-50" />
                          <button className="w-full flex items-center gap-3 px-4 py-2.5 text-[14px] text-claude-text hover:bg-black/5 dark:hover:bg-white/5 transition-colors text-left" onClick={(e) => { e.stopPropagation(); setActiveMenu(null); }}>
                            <Archive size={16} className="text-claude-textSecondary" />
                            {t('customize.archive')}
                          </button>
                          <button className="w-full flex items-center gap-3 px-4 py-2.5 text-[14px] text-[#E05A5A] hover:bg-red-500/10 transition-colors text-left" onClick={(e) => { e.stopPropagation(); setActiveMenu(null); setProjectToDelete(p); }}>
                            <Trash size={16} className="text-[#E05A5A]" />
                            {t('common.delete')}
                          </button>
                        </div>
                      </>
                    )}
                  </div>
                </div>

                <p className="text-[14px] text-claude-textSecondary line-clamp-2 leading-relaxed flex-1">
                  {p.description || t('customize.noDescriptionProvided')}
                </p>

                <div className="flex items-center gap-1.5 mt-2 text-[12px] text-claude-textSecondary/70 truncate">
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="flex-shrink-0 opacity-60">
                    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                  </svg>
                  <span className="truncate">{p.workspace_path || 'No workspace path'}</span>
                </div>

                <div className="mt-4 pt-1 flex items-center gap-4 text-[12px] text-claude-textSecondary/80">
                  <span>{t('customize.updated')} {new Date(p.updated_at).toLocaleDateString()}</span>
                  {(p.file_count ?? 0) > 0 && <span>• {p.file_count} {t('customize.files')}</span>}
                  {(p.chat_count ?? 0) > 0 && <span>• {p.chat_count} {t('sidebar.chats')}</span>}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center mt-12">
            <img src={startProjectsImg} alt="Start a project" className="w-[100px] h-auto mb-6 dark:invert opacity-90" />
            <h2 className="text-[17px] font-medium text-claude-text mb-3">{t('customize.lookingForProject')}</h2>
            <p className="text-[15px] text-claude-textSecondary text-center max-w-[400px] leading-relaxed mb-6">
              {t('customize.lookingForProjectDesc')}
            </p>
            <button
              onClick={() => setIsCreating(true)}
              className="flex items-center gap-2 px-4 py-2 bg-transparent border border-claude-border hover:bg-claude-hover rounded-xl text-claude-text transition-colors text-[14.5px] font-medium"
            >
              <Plus size={18} strokeWidth={2.5} />
              {t('customize.newProject')}
            </button>
          </div>
        )}
      </div>

      {projectToDelete && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
          <div className="bg-claude-input w-[460px] rounded-[16px] flex flex-col shadow-2xl relative border border-claude-border overflow-hidden">
            <div className="px-6 pt-6 pb-4 text-left">
              <h3 className="text-[19px] font-semibold text-claude-text mb-3">{t('customize.deleteProjectTitle')}</h3>
              <p className="text-[15px] text-claude-textSecondary leading-relaxed pr-4">
                {t('customize.confirmDeleteProjectMsg', { name: projectToDelete.name })}
              </p>
            </div>
            <div className="px-5 pb-5 pt-2 flex justify-end gap-3 mt-4">
              <button
                onClick={() => setProjectToDelete(null)}
                className="px-5 py-2 text-[14.5px] font-medium text-claude-text border border-claude-border hover:bg-claude-hover rounded-[8px] transition-colors"
              >
                {t('common.cancel')}
              </button>
              <button
                onClick={() => handleDeleteProject(projectToDelete)}
                className="px-5 py-2 text-[14.5px] font-medium text-white bg-[#E05A5A] hover:bg-[#E86B6B] rounded-[8px] transition-colors"
              >
                {t('common.delete')}
              </button>
            </div>
          </div>
        </div>
      )}

      {projectToEdit && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
          <div className="bg-claude-input w-[460px] rounded-[16px] flex flex-col shadow-2xl relative border border-claude-border overflow-hidden">
            <div className="px-6 pt-6 pb-4 text-left">
              <h3 className="text-[19px] font-semibold text-claude-text mb-5">{t('customize.editDetails')}</h3>

              <div className="space-y-4">
                <div>
                  <label className="block text-[14px] text-claude-textSecondary mb-2 font-medium">{t('customize.name')}</label>
                  <input
                    type="text"
                    value={editDetailsName}
                    onChange={(e) => setEditDetailsName(e.target.value)}
                    className="w-full px-3 py-2 bg-transparent border border-claude-border rounded-[8px] text-claude-text outline-none focus:border-[#3A7ADA] focus:ring-1 focus:ring-[#3A7ADA] transition-all text-[15px]"
                    autoFocus
                  />
                </div>
                <div>
                  <label className="block text-[14px] text-claude-textSecondary mb-2 font-medium">{t('customize.description')}</label>
                  <textarea
                    value={editDetailsDesc}
                    onChange={(e) => setEditDetailsDesc(e.target.value)}
                    rows={4}
                    className="w-full px-3 py-2 bg-claude-bg border border-claude-border rounded-[8px] text-claude-text outline-none focus:border-[#3A7ADA] focus:ring-1 focus:ring-[#3A7ADA] transition-all resize-none text-[14.5px] leading-relaxed"
                  />
                </div>
              </div>
            </div>

            <div className="px-6 pb-6 pt-2 flex justify-end gap-3 mt-4">
              <button
                onClick={() => setProjectToEdit(null)}
                className="px-5 py-2.5 text-[14.5px] font-medium text-claude-text border border-claude-border hover:bg-claude-hover rounded-[8px] transition-colors"
              >
                {t('common.cancel')}
              </button>
              <button
                onClick={handleSaveEditDetails}
                className="px-5 py-2.5 text-[14.5px] font-medium bg-claude-text text-claude-bg hover:opacity-90 rounded-[8px] transition-opacity"
              >
                {t('common.save')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default ProjectsPage;
