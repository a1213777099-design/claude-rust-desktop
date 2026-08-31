import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';

interface Message {
  role: string;
  content: string | any[];
  thinking?: string;
  toolUse?: any;
  toolResult?: any;
}

interface ModelCatalog {
  common: any[];
  all: any[];
  fallback_model: string | null;
}

interface CompactStatus {
  state: 'idle' | 'compacting' | 'done' | 'error';
  message?: string;
}

interface ChatState {
  messages: any[];
  loading: boolean;
  inputText: string;
  conversationTitle: string;
  conversationId: string | null;
  modelCatalog: ModelCatalog | null;
  currentModel: string;
  userMode: string;
  researchMode: boolean;
  openedResearchMsgId: string | null;
  compactStatus: CompactStatus;
  compactInstruction: string;
  planMode: boolean;
  providersCache: any[];
  webSearchToast: string | null;
  webSearchEnabled: boolean;
  permissionMode: string;
  autoCompactEnabled: boolean;
  autoCompactThreshold: number;
  crossModeWarning: CrossModeWarning | null;

  setMessages: (messages: any[] | ((prev: any[]) => any[])) => void;
  appendMessage: (msg: any) => void;
  updateLastMessage: (updater: (msg: any) => any) => void;
  appendToLastMessage: (delta: string) => void;
  appendThinkingToLastMessage: (thinking: string) => void;
  setLoading: (loading: boolean) => void;
  setInputText: (text: string | ((prev: string) => string)) => void;
  setConversationTitle: (title: string) => void;
  setConversationId: (id: string | null) => void;
  setModelCatalog: (catalog: ModelCatalog | null) => void;
  setCurrentModel: (model: string | ((prev: string) => string)) => void;
  setUserMode: (mode: string) => void;
  setResearchMode: (mode: boolean) => void;
  setOpenedResearchMsgId: (id: string | null) => void;
  setCompactStatus: (status: CompactStatus) => void;
  setCompactInstruction: (instruction: string) => void;
  setPlanMode: (mode: boolean) => void;
  setProvidersCache: (providers: any[]) => void;
  setWebSearchToast: (toast: string | null) => void;
  setWebSearchEnabled: (enabled: boolean) => void;
  setPermissionMode: (mode: string) => void;
    setAutoCompactEnabled: (enabled: boolean) => void;
    setAutoCompactThreshold: (threshold: number) => void;
  setCrossModeWarning: (warning: CrossModeWarning | null) => void;
  resetChat: () => void;
}

interface CrossModeWarning {
  convId: string;
  originalModel: string;
  otherMode: string;
  fallbackModel: string;
}

const initialState = {
  messages: [] as any[],
  loading: false,
  inputText: '',
  conversationTitle: '',
  conversationId: null as string | null,
  modelCatalog: null as ModelCatalog | null,
  currentModel: '',
  userMode: '',
  researchMode: (() => { try { return localStorage.getItem('research_mode') === 'true'; } catch { return false; } })(),
  openedResearchMsgId: null as string | null,
  compactStatus: { state: 'idle' as const },
  compactInstruction: '',
  planMode: false,
  providersCache: [] as any[],
  webSearchToast: null as string | null,
  webSearchEnabled: (() => { try { return localStorage.getItem('web_search_enabled') === 'true'; } catch { return false; } })(),
  permissionMode: (() => {
    try {
      return localStorage.getItem('permission_mode') || 'accept_edits';
    } catch { return 'accept_edits'; }
  })() as string,
  autoCompactEnabled: (() => { try { return localStorage.getItem('auto_compact_enabled') !== 'false'; } catch { return true; } })(),
  autoCompactThreshold: (() => { try { return parseInt(localStorage.getItem('auto_compact_threshold') || '80'); } catch { return 80; } })(),
  crossModeWarning: null as CrossModeWarning | null,
};

export const useChatStore = create<ChatState>()(
  subscribeWithSelector((set) => ({
    ...initialState,

    setMessages: (messages) =>
      set((state) => ({
        messages: typeof messages === 'function' ? messages(state.messages) : messages,
      })),

    appendMessage: (msg) =>
      set((state) => ({ messages: [...state.messages, msg] })),

    updateLastMessage: (updater) =>
      set((state) => {
        if (state.messages.length === 0) return state;
        const lastIdx = state.messages.length - 1;
        const updated = updater(state.messages[lastIdx]);
        if (updated === state.messages[lastIdx]) return state;
        const messages = [...state.messages];
        messages[lastIdx] = updated;
        return { messages };
      }),

    appendToLastMessage: (delta) =>
      set((state) => {
        if (state.messages.length === 0) return state;
        const lastIdx = state.messages.length - 1;
        const lastMsg = state.messages[lastIdx];
        if (typeof lastMsg.content !== 'string') return state;
        const newContent = lastMsg.content + delta;
        if (newContent === lastMsg.content) return state;
        const messages = [...state.messages];
        messages[lastIdx] = { ...lastMsg, content: newContent };
        return { messages };
      }),

    appendThinkingToLastMessage: (thinking) =>
      set((state) => {
        if (state.messages.length === 0) return state;
        const lastIdx = state.messages.length - 1;
        const lastMsg = state.messages[lastIdx];
        const newThinking = (lastMsg.thinking ?? '') + thinking;
        if (newThinking === lastMsg.thinking) return state;
        const messages = [...state.messages];
        messages[lastIdx] = { ...lastMsg, thinking: newThinking };
        return { messages };
      }),

    setLoading: (loading) => set({ loading }),

    setInputText: (inputText) =>
      set((state) => ({
        inputText: typeof inputText === 'function' ? inputText(state.inputText) : inputText,
      })),

    setConversationTitle: (conversationTitle) => set({ conversationTitle }),
    setConversationId: (conversationId) => set({ conversationId }),
    setModelCatalog: (modelCatalog) => set({ modelCatalog }),

    setCurrentModel: (currentModel) =>
      set((state) => ({
        currentModel: typeof currentModel === 'function' ? currentModel(state.currentModel) : currentModel,
      })),

    setUserMode: (userMode) => set({ userMode }),
    setResearchMode: (researchMode) => { set({ researchMode }); try { localStorage.setItem("research_mode", String(researchMode)); } catch {} },
    setOpenedResearchMsgId: (openedResearchMsgId) => set({ openedResearchMsgId }),
    setCompactStatus: (compactStatus) => set({ compactStatus }),
    setCompactInstruction: (compactInstruction) => set({ compactInstruction }),
    setPlanMode: (planMode) => set({ planMode }),
    setProvidersCache: (providersCache) => set({ providersCache }),
    setWebSearchToast: (webSearchToast) => set({ webSearchToast }),
    setWebSearchEnabled: (webSearchEnabled) => { set({ webSearchEnabled }); try { localStorage.setItem('web_search_enabled', String(webSearchEnabled)); } catch {} },
    setPermissionMode: (permissionMode: string) => {
      set({ permissionMode });
      // Persist to localStorage
      try {
        localStorage.setItem('permission_mode', permissionMode);
      } catch {}
    },
    setAutoCompactEnabled: (autoCompactEnabled: boolean) => {
      set({ autoCompactEnabled });
      try {
        localStorage.setItem('auto_compact_enabled', String(autoCompactEnabled));
      } catch {}
    },
    setAutoCompactThreshold: (autoCompactThreshold: number) => {
      set({ autoCompactThreshold });
      try {
        localStorage.setItem('auto_compact_threshold', String(autoCompactThreshold));
      } catch {}
    },
    setCrossModeWarning: (crossModeWarning: CrossModeWarning | null) => set({ crossModeWarning }),
    resetChat: () => set(initialState),
  }))
);

let pendingDelta = '';
let deltaRafId: number | null = null;

const flushDelta = () => {
  if (pendingDelta) {
    useChatStore.getState().appendToLastMessage(pendingDelta);
    pendingDelta = '';
  }
  deltaRafId = null;
};

export function appendDeltaThrottled(delta: string) {
  pendingDelta += delta;
  if (!deltaRafId) {
    deltaRafId = requestAnimationFrame(flushDelta);
  }
}

let pendingThinking = '';
let thinkingRafId: number | null = null;

const flushThinking = () => {
  if (pendingThinking) {
    useChatStore.getState().appendThinkingToLastMessage(pendingThinking);
    pendingThinking = '';
  }
  thinkingRafId = null;
};

export function appendThinkingThrottled(thinking: string) {
  pendingThinking += thinking;
  if (!thinkingRafId) {
    thinkingRafId = requestAnimationFrame(flushThinking);
  }
}
