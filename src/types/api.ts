/**
 * Core API types for Claude Desktop frontend.
 *
 * Replaces `any` types in API calls and store state
 * with proper TypeScript interfaces.
 */

// === Chat & Conversation ===

export interface Message {
  id?: string;
  role: 'user' | 'assistant' | 'system';
  content: string | ContentBlock[];
  thinking?: string;
  toolUse?: ToolUseBlock;
  toolResult?: ToolResultBlock;
  isCompactBoundary?: boolean;
  createdAt?: string;
}

export interface ContentBlock {
  type: 'text' | 'tool_use' | 'tool_result' | 'thinking';
  text?: string;
  thinking?: string;
  tool_use_id?: string;
  tool_name?: string;
  tool_input?: Record<string, unknown>;
  content?: string;
  is_error?: boolean;
}

export interface ToolUseBlock {
  tool_use_id: string;
  tool_name: string;
  tool_input: Record<string, unknown>;
  text_before?: string;
}

export interface ToolResultBlock {
  tool_use_id: string;
  output: string;
  is_error: boolean;
}

export interface Conversation {
  id: string;
  title: string | null;
  model: string | null;
  provider: string | null;
  workspace_path: string | null;
  project_id: string | null;
  research_mode: boolean;
  pinned: boolean;
  archived: boolean;
  created_at: string;
  updated_at: string;
  message_count: number;
}

// === Provider & Model ===

export interface ProviderConfig {
  id: string;
  name: string;
  apiKey: string | null;
  baseUrl: string;
  format: 'anthropic' | 'openai';
  models: ModelConfig[];
  enabled: boolean;
  supportsWebSearch?: boolean;
  webSearchStrategy?: string | null;
  webSearchTestedAt?: number | null;
  webSearchTestReason?: string | null;
}

export interface ModelConfig {
  id: string;
  name: string;
  enabled: boolean;
  maxTokens?: number;
  contextWindow?: number;
  supportsVision?: boolean;
  supportsTools?: boolean;
}

export interface ModelOption {
  id: string;
  name: string;
  providerId: string;
  providerName: string;
}

// === Project ===

export interface Project {
  id: string;
  name: string;
  description: string | null;
  instructions: string | null;
  workspace_path: string | null;
  is_archived: boolean;
  created_at: string;
  updated_at: string;
}

// === Skill ===

export interface Skill {
  name: string;
  description: string;
  content?: string;
  enabled: boolean;
  category?: string;
}

// === MCP ===

export interface McpServerConfig {
  id: string;
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
}

export interface McpServerStatus extends McpServerConfig {
  running: boolean;
  pid: number | null;
  tools_count: number;
  resources_count: number;
  error: string | null;
  transport_type: string;
}

export interface McpTool {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
  server_name: string;
}

// === SSE Events ===

export interface StreamEvent {
  type: string;
  model?: string;
  delta?: {
    type: string;
    text?: string;
    thinking?: string;
    partial_json?: string;
    stop_reason?: string;
  };
  tool_use_id?: string;
  tool_name?: string;
  tool_input?: Record<string, unknown>;
  output?: string;
  content?: string;
  is_error?: boolean;
  textBefore?: string;
  usage?: TokenUsage;
  error?: string;
  full_text?: string;
  stop_reason?: string;
  request_id?: string;
  questions?: AskUserQuestion[];
}

export interface TokenUsage {
  input_tokens?: number;
  output_tokens?: number;
  cache_creation_input_tokens?: number;
  cache_read_input_tokens?: number;
}

export interface AskUserQuestion {
  question: string;
  options: Array<{
    label: string;
    description?: string;
  }>;
}

// === Chat Request/Response ===

export interface ChatRequest {
  conversation_id: string;
  messages?: Message[];
  message?: string;
  model: string;
  user_mode?: string;
  env_token?: string;
  env_base_url?: string;
  research_mode?: boolean;
  enable_streaming?: boolean;
  custom_system_prompt?: string;
  permission_mode?: string;
  web_search_enabled?: boolean;
  reasoning_effort?: string;
  extended_thinking?: boolean;
}

// === Analytics ===

export interface AnalyticsEvent {
  event_type: string;
  data?: Record<string, unknown>;
  timestamp: string;
}

export interface AnalyticsSummary {
  total_conversations: number;
  total_messages: number;
  total_tokens: number;
  total_cost: number;
  period: string;
}

// === System ===

export interface SystemStatus {
  platform: string;
  git_bash: {
    required: boolean;
    found: boolean;
    path: string | null;
  };
}

export interface ContextSize {
  tokens: number;
  limit: number;
  model: string;
  message_count: number;
  usage_percent: number;
}

// === Store State Types ===

export interface CompactStatus {
  state: 'idle' | 'compacting' | 'done' | 'error';
  message?: string;
}

export interface ModelCatalog {
  common: ModelOption[];
  all: ModelOption[];
  fallback_model: string | null;
}

// === Error Types ===

export interface ApiError {
  error: string;
  status?: number;
  details?: unknown;
}

// === Utility Types ===

export type PermissionMode = 'ask_permissions' | 'accept_edits' | 'plan_mode' | 'bypass_permissions';
export type UserMode = 'clawparrot' | 'selfhosted';
export type MessageRole = 'user' | 'assistant' | 'system';
