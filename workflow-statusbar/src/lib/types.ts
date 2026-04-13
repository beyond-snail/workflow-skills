export type CodexStatus = "running" | "waiting_input" | "stalled" | "idle" | "offline";
export type WorkflowStage = "bootstrap" | "requirement" | "execution" | "done" | "unknown";

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
  is_blocked: boolean;
  is_active_by_codex: boolean;
  progress_label: string;
  stage_label: string;
  codex_status: CodexStatus;
  codex_heartbeat_at: string;
  codex_thread_id: string;
  codex_thread_name: string;
  auto_resume_enabled: boolean;
};

export type ProjectGroup = {
  key: string;
  label: string;
  items: ProjectSnapshot[];
};

export type Summary = {
  bootstrap: number;
  requirement: number;
  execution: number;
  blocked: number;
  done: number;
};

export type RuntimeState = {
  codex: CodexState;
  projects: ProjectSnapshot[];
  groups: ProjectGroup[];
  summary: Summary;
  spotlight_project: ProjectSnapshot | null;
  updated_at: string;
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
