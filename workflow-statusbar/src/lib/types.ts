export type CodexStatus = "running" | "waiting_input" | "stalled" | "idle" | "offline";
export type WorkflowStage = "idle" | "bootstrap" | "requirement" | "execution" | "done" | "unknown";
export type HostKind = "codex" | "claude";

export type HostSession = {
  host: HostKind;
  status: CodexStatus;
  heartbeat_at: string;
  thread_id: string;
  thread_name: string;
  project_path: string;
  last_message_role: string;
  last_message_text: string;
  process_running: boolean;
  source: string;
  confidence: string;
  token_total: number;
  token_input: number;
  token_output: number;
  token_reasoning: number;
  auto_resume_enabled: boolean;
  follow_up_prompted: boolean;
  updated_at: number;
};

export type CodexState = {
  status: CodexStatus;
  heartbeat_at: string;
  active_thread_id: string;
  active_thread_name: string;
  last_message_role: string;
  last_message_text: string;
  active_ide_project_name: string;
  active_project_path: string;
  source: string;
  confidence: string;
  process_running: boolean;
  auto_resume_enabled: boolean;
  monitored_project_name: string;
};

export type ProjectSnapshot = {
  name: string;
  path: string;
  workflow_stage: WorkflowStage;
  gate_status: string;
  health: string;
  risk: string;
  current_req_id: string;
  current_req_title: string;
  current_task_id: string;
  current_task_title: string;
  current_task_status: string;
  current_mode: string;
  last_sync_at: string;
  sync_source: string;
  active_host?: HostKind | null;
  other_host_summary?: string;
  hosts?: HostSession[];
  is_blocked: boolean;
  is_active_by_codex: boolean;
  is_open_in_ide: boolean;
  progress_label: string;
  stage_label: string;
  codex_status: CodexStatus;
  codex_heartbeat_at: string;
  codex_thread_id: string;
  codex_thread_name: string;
  last_message_role: string;
  last_message_text: string;
  token_total: number;
  token_input: number;
  token_output: number;
  token_reasoning: number;
  auto_resume_enabled: boolean;
};

export type ProjectGroup = {
  key: string;
  label: string;
  items: ProjectSnapshot[];
};

export type Summary = {
  idle: number;
  bootstrap: number;
  requirement: number;
  execution: number;
  blocked: number;
  done: number;
};

export type KnowledgebasePushState = {
  enabled: boolean;
  endpoint: string;
  connected: boolean;
  last_push_at: string;
  failure_count: number;
  last_error: string;
};

export type RuntimeState = {
  codex: CodexState;
  active_host?: HostKind | null;
  other_host_summary?: string;
  hosts?: HostSession[];
  knowledgebase_push: KnowledgebasePushState;
  projects: ProjectSnapshot[];
  groups: ProjectGroup[];
  summary: Summary;
  spotlight_project: ProjectSnapshot | null;
  updated_at: string;
};

export type KbStats = {
  projects: number;
  items: number;
  events: number;
  links: number;
};

export type KbProjectStatus = {
  project: string;
  path: string;
  item_count: number;
  event_count: number;
  document_count: number;
  conversation_count: number;
  last_item_at: string;
};

export type KbCollectProjectResult = {
  project: string;
  events: number;
  processed_files: number;
  documents: number;
  scanned_files: number;
};

export type KbSearchItem = {
  item_id: string;
  item_type: string;
  title: string;
  source_path: string;
  snippet: string;
};

export type KbSearchResponse = {
  query: string;
  items: KbSearchItem[];
};

export type KbTraceItem = {
  item_id: string;
  item_type: string;
  title: string;
  source_path: string;
};

export type KbTraceLink = {
  from_id: string;
  to_id: string;
  relation_type: string;
};

export type KbTraceResponse = {
  item: KbTraceItem | null;
  links: KbTraceLink[];
  related_items: KbTraceItem[];
};

export type AlertProviderMode = "disabled" | "bridge" | "feishu";

export type AlertSettings = {
  mode: AlertProviderMode;
  local_notifications_enabled: boolean;
  remote_notifications_enabled: boolean;
  local_notify_task_completed: boolean;
  remote_notify_task_completed: boolean;
  local_notify_project_completed: boolean;
  remote_notify_project_completed: boolean;
  local_notify_project_blocked: boolean;
  remote_notify_project_blocked: boolean;
  local_notify_task_interrupted: boolean;
  remote_notify_task_interrupted: boolean;
  local_notify_auto_resume_failed: boolean;
  remote_notify_auto_resume_failed: boolean;
  bridge_endpoint: string;
  bridge_token: string;
  feishu_app_id: string;
  feishu_app_secret: string;
  feishu_open_id: string;
  feishu_chat_id: string;
};
