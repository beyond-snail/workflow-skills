use chrono::{Datelike, Local};
use dirs::home_dir;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    ActivationPolicy, Emitter, Manager, PhysicalPosition, Position, Rect, Size, WebviewWindow,
};
use tauri_plugin_notification::NotificationExt;

const MAX_GROUP_ITEMS: usize = 5;
const PROJECT_ROTATION_SECONDS: i64 = 8;
const AUTO_RESUME_COOLDOWN_SECONDS: i64 = 90;
const OTHER_HOST_SUMMARY_FRESH_WINDOW_SECONDS: i64 = 2 * 60 * 60;
const POLL_INTERVAL_SECONDS: u64 = 8;
const TRAY_HIDE_DELAY_MS: u64 = 260;
const TRAY_MENU_OPEN_DASHBOARD: &str = "open_dashboard";
const TRAY_MENU_OPEN_ALERT_SETTINGS: &str = "open_alert_settings";
const TRAY_MENU_QUIT: &str = "quit";

type SharedRuntimeCache = Arc<Mutex<RuntimeCache>>;
type SharedAlertSettings = Arc<Mutex<AlertSettings>>;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CodexStatus {
    Running,
    WaitingInput,
    Stalled,
    Idle,
    Offline,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
enum HostKind {
    Codex,
    Claude,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct HostSession {
    host: HostKind,
    status: CodexStatus,
    heartbeat_at: String,
    thread_id: String,
    thread_name: String,
    project_path: String,
    last_message_role: String,
    last_message_text: String,
    process_running: bool,
    source: String,
    confidence: String,
    token_total: i64,
    token_input: i64,
    token_output: i64,
    token_reasoning: i64,
    auto_resume_enabled: bool,
    follow_up_prompted: bool,
    updated_at: i64,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkflowStage {
    Idle,
    Bootstrap,
    Requirement,
    Execution,
    Done,
    Unknown,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct CodexState {
    status: CodexStatus,
    heartbeat_at: String,
    active_thread_id: String,
    active_thread_name: String,
    last_message_role: String,
    last_message_text: String,
    active_ide_project_name: String,
    active_project_path: String,
    source: String,
    confidence: String,
    process_running: bool,
    auto_resume_enabled: bool,
    monitored_project_name: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct ProjectSnapshot {
    name: String,
    path: String,
    workflow_stage: WorkflowStage,
    gate_status: String,
    health: String,
    risk: String,
    current_req_id: String,
    current_req_title: String,
    current_task_id: String,
    current_task_title: String,
    current_task_status: String,
    current_mode: String,
    last_sync_at: String,
    sync_source: String,
    active_host: Option<HostKind>,
    other_host_summary: String,
    hosts: Vec<HostSession>,
    is_blocked: bool,
    is_active_by_codex: bool,
    is_open_in_ide: bool,
    progress_label: String,
    stage_label: String,
    codex_status: CodexStatus,
    codex_heartbeat_at: String,
    codex_thread_id: String,
    codex_thread_name: String,
    last_message_role: String,
    last_message_text: String,
    token_total: i64,
    token_input: i64,
    token_output: i64,
    token_reasoning: i64,
    auto_resume_enabled: bool,
    follow_up_prompted: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct ProjectGroup {
    key: String,
    label: String,
    items: Vec<ProjectSnapshot>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct Summary {
    idle: usize,
    bootstrap: usize,
    requirement: usize,
    execution: usize,
    blocked: usize,
    done: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct RuntimeState {
    codex: CodexState,
    active_host: Option<HostKind>,
    other_host_summary: String,
    hosts: Vec<HostSession>,
    projects: Vec<ProjectSnapshot>,
    groups: Vec<ProjectGroup>,
    summary: Summary,
    spotlight_project: Option<ProjectSnapshot>,
    updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
enum AlertProviderMode {
    #[default]
    Disabled,
    Bridge,
    Feishu,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct AlertSettings {
    #[serde(default)]
    mode: AlertProviderMode,
    #[serde(default = "default_true")]
    local_notifications_enabled: bool,
    #[serde(default = "default_true")]
    remote_notifications_enabled: bool,
    #[serde(default = "default_true")]
    local_notify_task_completed: bool,
    #[serde(default = "default_true")]
    remote_notify_task_completed: bool,
    #[serde(default = "default_true")]
    local_notify_project_completed: bool,
    #[serde(default = "default_true")]
    remote_notify_project_completed: bool,
    #[serde(default = "default_true")]
    local_notify_project_blocked: bool,
    #[serde(default = "default_true")]
    remote_notify_project_blocked: bool,
    #[serde(default = "default_true")]
    local_notify_task_interrupted: bool,
    #[serde(default = "default_true")]
    remote_notify_task_interrupted: bool,
    #[serde(default = "default_true")]
    local_notify_auto_resume_failed: bool,
    #[serde(default = "default_true")]
    remote_notify_auto_resume_failed: bool,
    #[serde(default)]
    bridge_endpoint: String,
    #[serde(default)]
    bridge_token: String,
    #[serde(default)]
    feishu_app_id: String,
    #[serde(default)]
    feishu_app_secret: String,
    #[serde(default)]
    feishu_open_id: String,
    #[serde(default)]
    feishu_chat_id: String,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            mode: AlertProviderMode::default(),
            local_notifications_enabled: true,
            remote_notifications_enabled: true,
            local_notify_task_completed: true,
            remote_notify_task_completed: true,
            local_notify_project_completed: true,
            remote_notify_project_completed: true,
            local_notify_project_blocked: true,
            remote_notify_project_blocked: true,
            local_notify_task_interrupted: true,
            remote_notify_task_interrupted: true,
            local_notify_auto_resume_failed: true,
            remote_notify_auto_resume_failed: true,
            bridge_endpoint: String::new(),
            bridge_token: String::new(),
            feishu_app_id: String::new(),
            feishu_app_secret: String::new(),
            feishu_open_id: String::new(),
            feishu_chat_id: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeSignature {
    codex_status: CodexStatus,
    focus_project_path: String,
    focus_task_id: String,
    focus_task_status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectRuntimeSignature {
    path: String,
    codex_status: CodexStatus,
    workflow_stage: WorkflowStage,
    task_id: String,
    task_status: String,
    thread_id: String,
    auto_resume_enabled: bool,
    follow_up_prompted: bool,
}

#[derive(Default)]
struct RuntimeCache {
    latest: Option<RuntimeState>,
    signature: Option<RuntimeSignature>,
    project_signatures: HashMap<String, ProjectRuntimeSignature>,
    last_auto_resume: Option<AutoResumeRecord>,
    startup_resume_checked: bool,
}

#[derive(Clone, Debug)]
struct AutoResumeRecord {
    thread_id: String,
    task_id: String,
    attempted_at: i64,
}

#[derive(Clone)]
enum AlertDispatchConfig {
    Bridge {
        endpoint: String,
        token: String,
    },
    Feishu {
        app_id: String,
        app_secret: String,
        open_id: String,
        chat_id: String,
    },
}

#[derive(Serialize)]
struct RemoteAlertPayload {
    event_type: String,
    title: String,
    body: String,
    project_name: String,
    project_path: String,
    active_host: String,
    thread_id: String,
    task_id: String,
    task_title: String,
    workflow_stage: String,
    codex_status: String,
    heartbeat_at: String,
    occurred_at: i64,
}

#[derive(Serialize)]
struct DebugHostEntry {
    host: HostKind,
    thread_id: String,
    project_path: String,
    status: CodexStatus,
    updated_at: i64,
}

#[derive(Serialize)]
struct DebugProjectEntry {
    name: String,
    path: String,
    is_open_in_ide: bool,
    thread_name: String,
    workflow_stage: WorkflowStage,
    active_host_before_apply: Option<HostKind>,
    active_host_after_apply: Option<HostKind>,
    other_host_summary: String,
    hosts_count: usize,
    hosts: Vec<DebugHostEntry>,
}

#[derive(Serialize)]
struct RuntimeDebugSnapshot<'a> {
    updated_at: String,
    code_titles: &'a [String],
    known_paths: &'a [String],
    frontmost_project_paths: &'a [String],
    open_project_paths: &'a [String],
    claude_threads: &'a [ClaudeThreadDebugEntry],
    claude_probe: Option<ClaudeProbeSnapshot>,
    spotlight_before_apply_path: String,
    spotlight_before_apply_host: Option<HostKind>,
    spotlight_after_apply_path: String,
    spotlight_after_apply_host: Option<HostKind>,
    projects: Vec<DebugProjectEntry>,
}

#[derive(Deserialize)]
struct FeishuTenantAccessTokenResponse {
    code: i64,
    msg: String,
    #[serde(default)]
    tenant_access_token: String,
}

#[derive(Deserialize)]
struct FeishuApiResponse {
    code: i64,
    msg: String,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Default)]
struct ProjectFile {
    #[serde(default)]
    project: ProjectFileProject,
    #[serde(default)]
    workflow: ProjectFileWorkflow,
    #[serde(default)]
    metrics: ProjectFileMetrics,
    #[serde(default)]
    sync: ProjectFileSync,
}

#[derive(Deserialize, Default)]
struct ProjectFileProject {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
}

#[derive(Deserialize, Default)]
struct ProjectFileWorkflow {
    #[serde(default)]
    stage: String,
    #[serde(default)]
    gate_status: String,
    #[serde(default)]
    #[serde(alias = "gateStatus")]
    gate_status_alias: String,
    #[serde(default)]
    health: String,
    #[serde(default)]
    risk: String,
    #[serde(default)]
    #[serde(alias = "currentReqId")]
    current_req_id: String,
    #[serde(default)]
    #[serde(alias = "currentReqTitle")]
    current_req_title: String,
    #[serde(default)]
    #[serde(alias = "currentTaskId")]
    current_task_id: String,
    #[serde(default)]
    #[serde(alias = "currentTaskTitle")]
    current_task_title: String,
    #[serde(default)]
    #[serde(alias = "currentTaskStatus")]
    current_task_status: String,
    #[serde(default)]
    #[serde(alias = "currentMode")]
    current_mode: String,
}

#[derive(Deserialize, Default)]
struct ProjectFileMetrics {
    #[serde(default)]
    total_tasks: usize,
    #[serde(default)]
    #[serde(alias = "totalTasks")]
    total_tasks_alias: usize,
    #[serde(default)]
    done: usize,
}

#[derive(Deserialize, Default)]
struct ProjectFileSync {
    #[serde(default)]
    source: String,
    #[serde(default)]
    #[serde(alias = "lastSyncAt")]
    last_sync_at: String,
}

#[derive(Clone)]
struct CodexThread {
    id: String,
    title: String,
    cwd: String,
    rollout_path: String,
    updated_at: i64,
    tokens_used: i64,
}

#[derive(Clone)]
struct ThreadRuntime {
    thread: CodexThread,
    last_log_ts: i64,
    status: CodexStatus,
    follow_up_prompted: bool,
}

#[derive(Clone, Default)]
struct ProjectTokenUsage {
    total: i64,
    today_input: i64,
    today_output: i64,
    today_reasoning: i64,
}

#[derive(Clone)]
struct ProjectRuntime {
    primary_thread: ThreadRuntime,
    token_usage: ProjectTokenUsage,
}

#[derive(Default)]
struct ThreadLastMessage {
    role: String,
    text: String,
}

#[derive(Clone)]
struct ClaudeThread {
    id: String,
    project_path: String,
    updated_at: i64,
    last_message_role: String,
    last_message_text: String,
}

#[derive(Clone, Serialize)]
struct ClaudeThreadDebugEntry {
    id: String,
    project_path: String,
    updated_at: i64,
    file_path: String,
    discovery_status: String,
    matched_project_path: String,
    match_status: String,
    matched_project_name: String,
}

#[derive(Clone, Serialize)]
struct ClaudeProbeSnapshot {
    home: String,
    projects_root_exists: bool,
    project_dir_count: usize,
    project_jsonl_count: usize,
    history_session_count: usize,
    session_file_count: usize,
    discovered_threads_count: usize,
}

impl ClaudeThreadDebugEntry {
    fn discovered(thread: &ClaudeThread, file_path: &Path) -> Self {
        Self {
            id: thread.id.clone(),
            project_path: thread.project_path.clone(),
            updated_at: thread.updated_at,
            file_path: file_path.to_string_lossy().to_string(),
            discovery_status: "discovered".into(),
            matched_project_path: String::new(),
            match_status: "pending_match".into(),
            matched_project_name: String::new(),
        }
    }

    fn skipped(session_id: &str, project_path: &str, file_path: Option<&Path>, reason: &str) -> Self {
        Self {
            id: session_id.into(),
            project_path: project_path.into(),
            updated_at: 0,
            file_path: file_path
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            discovery_status: reason.into(),
            matched_project_path: String::new(),
            match_status: "not_applicable".into(),
            matched_project_name: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct TokenUsage {
    input: i64,
    output: i64,
    reasoning: i64,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct TokenThreadCacheEntry {
    day_key: String,
    baseline: TokenUsage,
    latest: TokenUsage,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct TokenUsageCache {
    threads: HashMap<String, TokenThreadCacheEntry>,
}

#[derive(Clone, Debug)]
struct IdeProcess {
    pid: i32,
}

#[derive(Default)]
struct IdeSignal {
    frontmost_project_paths: Vec<String>,
    open_project_paths: Vec<String>,
    frontmost_project_name: String,
}

const IDE_PROCESS_NAMES: &[&str] = &[
    "Code",
    "Cursor",
    "Windsurf",
    "Trae",
    "Xcode",
    "idea",
    "IntelliJ IDEA",
    "WebStorm",
    "PyCharm",
    "GoLand",
    "Android Studio",
];

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn is_follow_up_resume_candidate(status: &CodexStatus) -> bool {
    matches!(
        status,
        CodexStatus::WaitingInput | CodexStatus::Stalled | CodexStatus::Idle
    )
}

fn fmt_relative_age(ts: i64) -> String {
    if ts <= 0 {
        return "未采集".into();
    }

    let diff = unix_now().saturating_sub(ts);
    if diff < 60 {
        format!("{diff} 秒前")
    } else if diff < 3600 {
        format!("{} 分钟前", diff / 60)
    } else {
        format!("{} 小时前", diff / 3600)
    }
}

fn format_sync_text(input: &str) -> String {
    if input.trim().is_empty() {
        "未同步".into()
    } else {
        input.into()
    }
}

fn path_matches(project_path: &str, candidate_path: &str) -> bool {
    let project = project_path.trim_end_matches('/');
    let candidate = candidate_path.trim_end_matches('/');

    if project.is_empty() || candidate.is_empty() {
        return false;
    }

    candidate == project || candidate.starts_with(&(project.to_string() + "/"))
}

fn debug_log_path() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(home.join("Library/Logs/workflow-statusbar/runtime-debug.log"))
}

fn token_usage_cache_path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/workflow-statusbar/token-usage-cache.json")
}

fn read_token_usage_cache(home: &Path) -> TokenUsageCache {
    let path = token_usage_cache_path(home);
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => TokenUsageCache::default(),
    }
}

fn save_token_usage_cache(home: &Path, cache: &TokenUsageCache) {
    let path = token_usage_cache_path(home);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(cache) {
        let _ = fs::write(path, content);
    }
}

fn write_runtime_debug_snapshot(
    code_titles: &[String],
    known_paths: &[String],
    ide_signal: &IdeSignal,
    claude_threads: &[ClaudeThreadDebugEntry],
    claude_probe: Option<ClaudeProbeSnapshot>,
    projects_before_apply: &[ProjectSnapshot],
    spotlight_before_apply: Option<&ProjectSnapshot>,
    spotlight_after_apply: Option<&ProjectSnapshot>,
    projects: &[ProjectSnapshot],
) {
    let Some(path) = debug_log_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let before_active_host_by_path: HashMap<String, Option<HostKind>> = projects_before_apply
        .iter()
        .map(|project| (project.path.clone(), project.active_host.clone()))
        .collect();

    let snapshot = RuntimeDebugSnapshot {
        updated_at: Local::now().to_rfc3339(),
        code_titles,
        known_paths,
        frontmost_project_paths: &ide_signal.frontmost_project_paths,
        open_project_paths: &ide_signal.open_project_paths,
        claude_threads,
        claude_probe,
        spotlight_before_apply_path: spotlight_before_apply
            .map(|item| item.path.clone())
            .unwrap_or_default(),
        spotlight_before_apply_host: spotlight_before_apply
            .and_then(|item| item.active_host.clone()),
        spotlight_after_apply_path: spotlight_after_apply
            .map(|item| item.path.clone())
            .unwrap_or_default(),
        spotlight_after_apply_host: spotlight_after_apply
            .and_then(|item| item.active_host.clone()),
        projects: projects
            .iter()
            .map(|project| DebugProjectEntry {
                name: project.name.clone(),
                path: project.path.clone(),
                is_open_in_ide: project.is_open_in_ide,
                thread_name: project.codex_thread_name.clone(),
                workflow_stage: project.workflow_stage.clone(),
                active_host_before_apply: before_active_host_by_path
                    .get(&project.path)
                    .cloned()
                    .flatten(),
                active_host_after_apply: project.active_host.clone(),
                other_host_summary: project.other_host_summary.clone(),
                hosts_count: project.hosts.len(),
                hosts: project
                    .hosts
                    .iter()
                    .map(|host| DebugHostEntry {
                        host: host.host.clone(),
                        thread_id: host.thread_id.clone(),
                        project_path: host.project_path.clone(),
                        status: host.status.clone(),
                        updated_at: host.updated_at,
                    })
                    .collect(),
            })
            .collect(),
    };

    if let Ok(content) = serde_json::to_string_pretty(&snapshot) {
        let _ = fs::write(path, content);
    }
}

fn project_name_key(input: &str) -> String {
    input.trim().to_lowercase()
}

fn project_name_from_path(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
}

fn stage_from_str(input: &str) -> WorkflowStage {
    match input {
        "idle" => WorkflowStage::Idle,
        "bootstrap" => WorkflowStage::Bootstrap,
        "requirement" => WorkflowStage::Requirement,
        "execution" => WorkflowStage::Execution,
        "done" => WorkflowStage::Done,
        _ => WorkflowStage::Unknown,
    }
}

fn stage_label(stage: &WorkflowStage) -> String {
    match stage {
        WorkflowStage::Idle => "已接入".into(),
        WorkflowStage::Bootstrap => "底座".into(),
        WorkflowStage::Requirement => "需求".into(),
        WorkflowStage::Execution => "执行".into(),
        WorkflowStage::Done => "完成".into(),
        WorkflowStage::Unknown => "未同步".into(),
    }
}

fn lookup_state_file(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(path) = current {
        let candidate = path.join(".ai/runtime/project-state.json");
        if candidate.exists() {
            return Some(candidate);
        }
        current = path.parent();
    }
    None
}

fn read_recent_threads(home: &Path) -> Vec<CodexThread> {
    let db_path = home.join(".codex/state_5.sqlite");
    let connection = match Connection::open(db_path) {
        Ok(connection) => connection,
        Err(_) => return Vec::new(),
    };

    let mut statement = match connection.prepare(
        "select id, title, cwd, rollout_path, updated_at, tokens_used from threads where archived = 0 order by updated_at desc",
    ) {
        Ok(statement) => statement,
        Err(_) => return Vec::new(),
    };

    let rows = statement.query_map([], |row| {
        Ok(CodexThread {
            id: row.get(0)?,
            title: row.get(1)?,
            cwd: row.get(2)?,
            rollout_path: row.get(3)?,
            updated_at: row.get(4)?,
            tokens_used: row.get(5)?,
        })
    });

    match rows {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

fn detect_follow_up_prompt(rollout_path: &str) -> bool {
    if rollout_path.trim().is_empty() {
        return false;
    }

    let mut file = match fs::File::open(rollout_path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let file_len = file.metadata().map(|metadata| metadata.len()).unwrap_or_default();
    let start = file_len.saturating_sub(256 * 1024);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return false;
    }

    let markers = [
        "下一步可以直接做",
        "如果你要继续",
        "如果要继续",
        "继续的话",
        "你发我一个主题",
        "我直接继续",
        "直接进入",
    ];

    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return false;
    }

    let tail = String::from_utf8_lossy(&bytes);
    for line in tail.lines() {
        if !line.contains("\"type\":\"event_msg\"")
            && !line.contains("\"type\":\"response_item\"")
            && !line.contains("\"last_agent_message\"")
        {
            continue;
        }

        if markers.iter().any(|marker| line.contains(marker)) {
            return true;
        }
    }

    false
}

fn read_last_thread_message(rollout_path: &str) -> ThreadLastMessage {
    if rollout_path.trim().is_empty() {
        return ThreadLastMessage::default();
    }

    let mut file = match fs::File::open(rollout_path) {
        Ok(file) => file,
        Err(_) => return ThreadLastMessage::default(),
    };
    let file_len = file.metadata().map(|metadata| metadata.len()).unwrap_or_default();
    let start = file_len.saturating_sub(512 * 1024);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return ThreadLastMessage::default();
    }

    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return ThreadLastMessage::default();
    }
    let content = String::from_utf8_lossy(&bytes);

    let mut last_role = String::new();
    let mut last_text = String::new();

    for line in content.lines() {
        if line.contains("\"role\":\"assistant\"") {
            if let Some(text) = extract_json_field(line, "\"text\":\"") {
                if !text.trim().is_empty() {
                    last_role = "assistant".into();
                    last_text = text;
                }
            } else if let Some(text) = extract_json_field(line, "\"message\":\"") {
                if !text.trim().is_empty() {
                    last_role = "assistant".into();
                    last_text = text;
                }
            }
        } else if line.contains("\"role\":\"user\"") {
            if let Some(text) = extract_json_field(line, "\"text\":\"") {
                if !text.trim().is_empty() {
                    last_role = "user".into();
                    last_text = text;
                }
            }
        } else if line.contains("\"type\":\"agent_message\"") {
            if let Some(text) = extract_json_field(line, "\"message\":\"") {
                if !text.trim().is_empty() {
                    last_role = "assistant".into();
                    last_text = text;
                }
            }
        }
    }

    ThreadLastMessage {
        role: last_role,
        text: sanitize_inline_text(&last_text),
    }
}

fn extract_json_field(line: &str, marker: &str) -> Option<String> {
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let mut escaped = false;
    let mut value = String::new();

    for ch in rest.chars() {
        if escaped {
            match ch {
                'n' | 'r' => value.push(' '),
                't' => value.push(' '),
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                _ => value.push(ch),
            }
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => break,
            _ => value.push(ch),
        }
    }

    Some(value)
}

fn sanitize_inline_text(input: &str) -> String {
    input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn read_file_tail(path: &str, max_bytes: u64) -> Option<String> {
    if path.trim().is_empty() {
        return None;
    }

    let mut file = fs::File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    let start = file_len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn local_today() -> (i32, u32, u32) {
    let now = Local::now();
    (now.year(), now.month(), now.day())
}

fn format_day_key(day: (i32, u32, u32)) -> String {
    format!("{:04}-{:02}-{:02}", day.0, day.1, day.2)
}

fn parse_line_day(line: &str) -> Option<(i32, u32, u32)> {
    let marker = "\"timestamp\":\"";
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let year = rest.get(0..4)?.parse::<i32>().ok()?;
    let month = rest.get(5..7)?.parse::<u32>().ok()?;
    let day = rest.get(8..10)?.parse::<u32>().ok()?;
    Some((year, month, day))
}

fn unix_day(ts: i64) -> Option<(i32, u32, u32)> {
    chrono::DateTime::from_timestamp(ts, 0).map(|dt| {
        let local = dt.with_timezone(&Local);
        (local.year(), local.month(), local.day())
    })
}

fn extract_json_number_after(line: &str, scope_marker: &str, marker: &str) -> i64 {
    let scoped = line
        .find(scope_marker)
        .and_then(|start| line.get(start..))
        .unwrap_or(line);
    let Some(start) = scoped.find(marker).map(|index| index + marker.len()) else {
        return 0;
    };
    let rest = &scoped[start..];
    let Some(colon_index) = rest.find(':') else {
        return 0;
    };
    let number = rest[colon_index + 1..]
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();

    number.parse::<i64>().unwrap_or_default()
}

fn read_latest_log_ts(home: &Path) -> Option<i64> {
    let db_path = home.join(".codex/logs_2.sqlite");
    let connection = Connection::open(db_path).ok()?;
    connection
        .query_row("select max(ts) from logs", [], |row| row.get::<_, Option<i64>>(0))
        .ok()
        .flatten()
}

fn read_thread_log_ts(home: &Path) -> HashMap<String, i64> {
    let db_path = home.join(".codex/logs_2.sqlite");
    let connection = match Connection::open(db_path) {
        Ok(connection) => connection,
        Err(_) => return HashMap::new(),
    };

    let mut statement = match connection.prepare(
        "select thread_id, max(ts) from logs where thread_id is not null and thread_id != '' group by thread_id",
    ) {
        Ok(statement) => statement,
        Err(_) => return HashMap::new(),
    };

    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    });

    match rows {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => HashMap::new(),
    }
}

fn build_thread_runtime(
    thread: &CodexThread,
    thread_log_ts: &HashMap<String, i64>,
    process_running: bool,
    now: i64,
    active_thread_id: &str,
) -> ThreadRuntime {
    let last_log_ts = thread_log_ts.get(&thread.id).copied().unwrap_or_default();
    let is_active_thread = !active_thread_id.is_empty() && thread.id == active_thread_id;
    ThreadRuntime {
        thread: thread.clone(),
        last_log_ts,
        status: if is_active_thread {
            codex_status_from_activity(process_running, last_log_ts, now)
        } else if last_log_ts > 0 && now.saturating_sub(last_log_ts) <= 90 {
            CodexStatus::WaitingInput
        } else {
            CodexStatus::Idle
        },
        follow_up_prompted: false,
    }
}

fn enrich_primary_thread_runtime(runtime: &mut ThreadRuntime) {
    runtime.follow_up_prompted = detect_follow_up_prompt(&runtime.thread.rollout_path);
}

fn build_project_token_usage(
    runtimes: &[ThreadRuntime],
    today: (i32, u32, u32),
    cache: &mut TokenUsageCache,
) -> ProjectTokenUsage {
    let mut usage = ProjectTokenUsage::default();
    let day_key = format_day_key(today);

    for runtime in runtimes {
        usage.total += runtime.thread.tokens_used.max(0);
        if unix_day(runtime.thread.updated_at) != Some(today) {
            continue;
        }

        let entry = cache
            .threads
            .entry(runtime.thread.id.clone())
            .or_default();
        if entry.day_key != day_key {
            *entry = TokenThreadCacheEntry {
                day_key: day_key.clone(),
                baseline: TokenUsage::default(),
                latest: TokenUsage::default(),
            };
        }

        let Some(content) = read_file_tail(&runtime.thread.rollout_path, 1024 * 1024) else {
            usage.today_input += (entry.latest.input - entry.baseline.input).max(0);
            usage.today_output += (entry.latest.output - entry.baseline.output).max(0);
            usage.today_reasoning += (entry.latest.reasoning - entry.baseline.reasoning).max(0);
            continue;
        };
        let mut first_today: Option<TokenUsage> = None;
        let mut latest_today: Option<TokenUsage> = None;

        for line in content.lines() {
            if !line.contains("\"token_count\"") || !line.contains("\"total_token_usage\"") {
                continue;
            }
            if parse_line_day(&line) != Some(today) {
                continue;
            }

            let total = extract_json_number_after(&line, "\"total_token_usage\"", "\"total_tokens\"");
            if total <= 0 {
                continue;
            }

            let current = TokenUsage {
                input: extract_json_number_after(&line, "\"total_token_usage\"", "\"input_tokens\""),
                output: extract_json_number_after(&line, "\"total_token_usage\"", "\"output_tokens\""),
                reasoning: extract_json_number_after(&line, "\"total_token_usage\"", "\"reasoning_output_tokens\""),
            };

            if first_today.is_none() {
                first_today = Some(current.clone());
            }
            latest_today = Some(current);
        }

        if let Some(first) = first_today {
            let baseline_empty =
                entry.baseline.input == 0 && entry.baseline.output == 0 && entry.baseline.reasoning == 0;
            let first_total = first.input + first.output + first.reasoning;
            let baseline_total = entry.baseline.input + entry.baseline.output + entry.baseline.reasoning;
            if baseline_empty || first_total < baseline_total {
                entry.baseline = first.clone();
            }
        }

        if let Some(last) = latest_today {
            let last_total = last.input + last.output + last.reasoning;
            let latest_total = entry.latest.input + entry.latest.output + entry.latest.reasoning;
            if last_total >= latest_total {
                entry.latest = last;
            }
        }

        usage.today_input += (entry.latest.input - entry.baseline.input).max(0);
        usage.today_output += (entry.latest.output - entry.baseline.output).max(0);
        usage.today_reasoning += (entry.latest.reasoning - entry.baseline.reasoning).max(0);
    }

    usage
}

fn infer_thread_project_key(thread: &CodexThread) -> String {
    project_name_from_path(&thread.cwd)
        .map(|name| project_name_key(&name))
        .unwrap_or_else(|| project_name_key(&thread.cwd))
}

fn match_thread_for_placeholder(name: &str, threads: &[CodexThread]) -> Vec<CodexThread> {
    let target = project_name_key(name);
    threads
        .iter()
        .filter(|thread| infer_thread_project_key(thread) == target)
        .cloned()
        .collect()
}

fn match_threads_for_path(path: &str, threads: &[CodexThread]) -> Vec<CodexThread> {
    threads
        .iter()
        .filter(|thread| path_matches(path, &thread.cwd) || path_matches(&thread.cwd, path))
        .cloned()
        .collect()
}

fn codex_status_from_activity(process_running: bool, last_log_ts: i64, now: i64) -> CodexStatus {
    let log_age = if last_log_ts > 0 {
        now.saturating_sub(last_log_ts)
    } else {
        i64::MAX
    };

    if process_running && log_age <= 20 {
        CodexStatus::Running
    } else if process_running && log_age <= 90 {
        CodexStatus::WaitingInput
    } else if process_running {
        CodexStatus::Stalled
    } else {
        CodexStatus::Idle
    }
}

fn codex_process_running() -> bool {
    Command::new("pgrep")
        .args(["-f", "codex app-server|codex"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn claude_process_running() -> bool {
    Command::new("pgrep")
        .args(["-f", "claude"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn read_recent_claude_threads(
    home: &Path,
) -> (
    Vec<ClaudeThread>,
    Vec<ClaudeThreadDebugEntry>,
    ClaudeProbeSnapshot,
) {
    let history_path = home.join(".claude/history.jsonl");
    let content = fs::read_to_string(history_path).unwrap_or_default();

    let mut project_by_session: HashMap<String, (String, i64)> = HashMap::new();
    for line in content.lines() {
        let Ok(payload) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(session_id) = payload.get("sessionId").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(project_path) = payload.get("project").and_then(|value| value.as_str()) else {
            continue;
        };
        let timestamp_ms = payload.get("timestamp").and_then(|value| value.as_i64()).unwrap_or_default();
        let timestamp = if timestamp_ms > 0 { timestamp_ms / 1000 } else { 0 };
        if timestamp <= 0 {
            continue;
        }

        let replace = project_by_session
            .get(session_id)
            .map(|(_, existing_ts)| timestamp >= *existing_ts)
            .unwrap_or(true);
        if replace {
            project_by_session.insert(session_id.to_string(), (project_path.to_string(), timestamp));
        }
    }

    let projects_root = home.join(".claude/projects");
    let now = unix_now();
    let stale_threshold = 7 * 24 * 3600;
    let mut session_files: HashMap<String, (PathBuf, i64)> = HashMap::new();
    let mut debug_entries = Vec::new();
    let mut project_dir_count = 0usize;
    let mut project_jsonl_count = 0usize;

    if let Ok(project_dirs) = fs::read_dir(&projects_root) {
        for project_dir in project_dirs.flatten() {
            let dir_path = project_dir.path();
            if !dir_path.is_dir() {
                continue;
            }
            project_dir_count += 1;
            let Ok(files) = fs::read_dir(dir_path) else {
                continue;
            };
            for file in files.flatten() {
                let file_path = file.path();
                if file_path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
                    continue;
                }
                project_jsonl_count += 1;
                let Some(session_id) = file_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .map(|value| value.to_string())
                else {
                    continue;
                };
                let modified_at = fs::metadata(&file_path)
                    .ok()
                    .and_then(|meta| meta.modified().ok())
                    .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs() as i64)
                    .unwrap_or_default();

                let in_history = project_by_session.contains_key(&session_id);
                if !in_history
                    && modified_at > 0
                    && now.saturating_sub(modified_at) > stale_threshold
                {
                    continue;
                }

                let replace = session_files
                    .get(&session_id)
                    .map(|(_, existing_ts)| modified_at >= *existing_ts)
                    .unwrap_or(true);
                if replace {
                    session_files.insert(session_id, (file_path, modified_at));
                }
            }
        }
    }

    for (session_id, (project_path, _)) in &project_by_session {
        if session_files.contains_key(session_id) {
            continue;
        }
        let escaped = project_path.trim_start_matches('/').replace('/', "-");
        let candidate = projects_root.join(format!("-{escaped}/{session_id}.jsonl"));
        if !candidate.is_file() {
            debug_entries.push(ClaudeThreadDebugEntry::skipped(
                session_id,
                project_path,
                Some(&candidate),
                "candidate_missing",
            ));
            continue;
        }
        let modified_at = fs::metadata(&candidate)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default();
        session_files.insert(session_id.clone(), (candidate, modified_at));
    }

    let session_file_count = session_files.len();
    let mut threads = Vec::new();
    let mut seen = HashSet::new();
    for (session_id, (file_path, _)) in session_files {
        let (mut project_path, mut updated_at) = project_by_session
            .get(&session_id)
            .cloned()
            .unwrap_or_else(|| (String::new(), 0));
        let mut normalized_session_id = session_id.clone();

        let mut last_role = String::new();
        let mut last_text = String::new();
        if let Ok(file_content) = fs::read_to_string(&file_path) {
            for line in file_content.lines() {
                let Ok(payload) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                if let Some(value) = payload.get("sessionId").and_then(|item| item.as_str()) {
                    if !value.trim().is_empty() {
                        normalized_session_id = value.to_string();
                    }
                }
                if let Some(cwd) = payload.get("cwd").and_then(|value| value.as_str()) {
                    if !cwd.trim().is_empty() {
                        project_path = cwd.trim().to_string();
                    }
                }
                if let Some(ts) = payload.get("timestamp").and_then(|value| value.as_str()) {
                    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) {
                        updated_at = updated_at.max(parsed.timestamp());
                    }
                }
                if let Some(ts_ms) = payload.get("timestamp").and_then(|value| value.as_i64()) {
                    if ts_ms > 0 {
                        updated_at = updated_at.max(ts_ms / 1000);
                    }
                }

                let Some(message) = payload.get("message") else {
                    continue;
                };
                let Some(role) = message.get("role").and_then(|value| value.as_str()) else {
                    continue;
                };
                let Some(content) = message.get("content").and_then(|value| value.as_array()) else {
                    continue;
                };
                let text = content
                    .iter()
                    .filter_map(|item| item.get("text").and_then(|value| value.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ");
                if text.trim().is_empty() {
                    continue;
                }

                last_role = role.to_string();
                last_text = sanitize_inline_text(&text);
            }
        }

        if project_path.trim().is_empty() {
            debug_entries.push(ClaudeThreadDebugEntry::skipped(
                &normalized_session_id,
                "",
                Some(&file_path),
                "missing_project_path",
            ));
            continue;
        }

        if seen.insert((normalized_session_id.clone(), project_path.clone())) {
            let thread = ClaudeThread {
                id: normalized_session_id,
                project_path,
                updated_at,
                last_message_role: last_role,
                last_message_text: last_text,
            };
            debug_entries.push(ClaudeThreadDebugEntry::discovered(&thread, &file_path));
            threads.push(thread);
        }
    }

    threads.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    debug_entries.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    let probe = ClaudeProbeSnapshot {
        home: home.to_string_lossy().to_string(),
        projects_root_exists: projects_root.exists(),
        project_dir_count,
        project_jsonl_count,
        history_session_count: project_by_session.len(),
        session_file_count,
        discovered_threads_count: threads.len(),
    };
    (threads, debug_entries, probe)
}

fn read_frontmost_pid() -> Option<i32> {
    let front = Command::new("lsappinfo").arg("front").output().ok()?;
    if !front.status.success() {
        return None;
    }
    let front_stdout = String::from_utf8_lossy(&front.stdout).to_string();
    let asn = front_stdout
        .trim()
        .strip_prefix("ASN:")?
        .trim();
    if asn.is_empty() {
        return None;
    }

    let info = Command::new("lsappinfo")
        .args(["info", "-only", "pid", asn])
        .output()
        .ok()?;
    if !info.status.success() {
        return None;
    }

    String::from_utf8_lossy(&info.stdout)
        .lines()
        .find_map(|line| line.split_once('='))
        .and_then(|(_, value)| value.trim().parse::<i32>().ok())
}

fn read_frontmost_app_name() -> Option<String> {
    let front = Command::new("lsappinfo").arg("front").output().ok()?;
    if !front.status.success() {
        return None;
    }
    let front_stdout = String::from_utf8_lossy(&front.stdout).to_string();
    let asn = front_stdout
        .trim()
        .strip_prefix("ASN:")?
        .trim();
    if asn.is_empty() {
        return None;
    }

    let info = Command::new("lsappinfo")
        .args(["info", "-only", "name", asn])
        .output()
        .ok()?;
    if !info.status.success() {
        return None;
    }

    String::from_utf8_lossy(&info.stdout)
        .lines()
        .find_map(|line| line.split_once('='))
        .map(|(_, value)| value.trim().trim_matches('"').to_string())
}

fn read_ide_processes() -> Vec<IdeProcess> {
    let output = match Command::new("ps").args(["-axo", "pid=,args="]).output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let markers = [
        "/Visual Studio Code.app/",
        "/Code Helper",
        "/Cursor.app/",
        "/Cursor Helper",
        "/Windsurf.app/",
        "/Windsurf Helper",
        "/Trae.app/",
        "/Trae Helper",
        "/Xcode.app/",
    ];

    let mut seen = HashSet::new();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let (pid_raw, args_raw) = trimmed.split_once(' ')?;
            let pid = pid_raw.trim().parse::<i32>().ok()?;
            let args = args_raw.trim();
            if markers.iter().any(|marker| args.contains(marker)) && seen.insert(pid) {
                Some(IdeProcess { pid })
            } else {
                None
            }
        })
        .collect()
}

fn read_window_titles(process_name: &str) -> Vec<String> {
    let script = format!(
        "with timeout of 1 second\n\
tell application \"System Events\"\n\
tell process \"{process_name}\" to get name of every window\n\
end tell\n\
end timeout"
    );
    let output = match Command::new("osascript").args(["-e", &script]).output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    String::from_utf8_lossy(&output.stdout)
        .split(',')
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn is_process_running(process_name: &str) -> bool {
    Command::new("pgrep")
        .args(["-x", process_name])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn read_all_ide_window_titles() -> Vec<String> {
    let mut titles = Vec::new();
    let mut seen = HashSet::new();

    for process_name in IDE_PROCESS_NAMES {
        if !is_process_running(process_name) {
            continue;
        }

        for title in read_window_titles(process_name) {
            if seen.insert(title.clone()) {
                titles.push(title);
            }
        }
    }

    titles
}

fn project_paths_from_titles(projects: &[ProjectSnapshot], titles: &[String]) -> Vec<String> {
    let mut matched = Vec::new();
    let mut seen = HashSet::new();

    for title in titles {
        let lower_title = title.to_lowercase();
        for project in projects {
            if lower_title.contains(&project.name.to_lowercase()) && seen.insert(project.path.clone()) {
                matched.push(project.path.clone());
            }
        }
    }

    matched
}

fn extract_project_name_from_title(title: &str) -> Option<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((_, suffix)) = trimmed.rsplit_once(" — ") {
        let candidate = suffix.trim();
        if !candidate.is_empty() && candidate != "Code" {
            return Some(candidate.to_string());
        }
    }

    if let Some(open_bracket) = trimmed.rfind('[') {
        if trimmed.ends_with(']') {
            let candidate = trimmed[open_bracket + 1..trimmed.len() - 1].trim();
            if !candidate.is_empty() {
                return Some(candidate.to_string());
            }
        }
    }

    if let Some((prefix, _)) = trimmed.split_once(" – ") {
        let candidate = prefix.trim();
        if !candidate.is_empty() {
            return Some(candidate.to_string());
        }
    }

    let ignored_titles = [
        "Code",
        "Welcome",
        "Settings",
        "Extensions",
        "Search",
        "Run and Debug",
        "Source Control",
        "Timeline",
        "Output",
        "Terminal",
        "Problems",
    ];
    if ignored_titles.iter().any(|item| item.eq_ignore_ascii_case(trimmed)) {
        return None;
    }

    Some(trimmed.to_string())
}

fn known_project_paths_for_pid(pid: i32, known_paths: &[String]) -> Vec<String> {
    let output = match Command::new("lsof")
        .args(["-Fn", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let mut matched = Vec::new();
    let mut seen = HashSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some(path) = line.strip_prefix('n') else {
            continue;
        };
        for known_path in known_paths {
            if path_matches(known_path, path) && seen.insert(known_path.clone()) {
                matched.push(known_path.clone());
            }
        }
    }

    matched
}

fn read_ide_signal(projects: &[ProjectSnapshot], known_paths: &[String]) -> IdeSignal {
    if projects.is_empty() {
        let code_titles = read_all_ide_window_titles();
        return IdeSignal {
            frontmost_project_name: infer_project_name_from_titles(&code_titles),
            ..IdeSignal::default()
        };
    }

    let frontmost_pid = read_frontmost_pid();
    let frontmost_app_name = read_frontmost_app_name();
    let mut frontmost_project_paths = Vec::new();
    let mut open_project_paths = Vec::new();
    let mut seen = HashSet::new();

    let code_titles = read_all_ide_window_titles();
    let code_title_paths = project_paths_from_titles(projects, &code_titles);
    for path in &code_title_paths {
        if seen.insert(path.clone()) {
            open_project_paths.push(path.clone());
        }
    }

    for process in read_ide_processes() {
        let project_paths = known_project_paths_for_pid(process.pid, known_paths);
        if project_paths.is_empty() {
            continue;
        }

        let is_frontmost = frontmost_pid == Some(process.pid);
        for project_path in project_paths {
            if seen.insert(project_path.clone()) {
                open_project_paths.push(project_path.clone());
            }
            if is_frontmost {
                frontmost_project_paths.push(project_path);
            }
        }
    }

    if frontmost_project_paths.is_empty() && IDE_PROCESS_NAMES.iter().any(|name| frontmost_app_name.as_deref() == Some(*name)) {
        frontmost_project_paths = code_title_paths.clone();
    }

    IdeSignal {
        frontmost_project_paths,
        open_project_paths,
        frontmost_project_name: infer_project_name_from_titles(&code_titles),
    }
}

fn infer_project_name_from_titles(titles: &[String]) -> String {
    titles
        .first()
        .and_then(|title| extract_project_name_from_title(title))
        .filter(|value| !value.is_empty() && !IDE_PROCESS_NAMES.contains(&value.as_str()))
        .unwrap_or_default()
}

fn infer_projects_from_titles(titles: &[String]) -> Vec<(String, String)> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for title in titles {
        if let Some(project_name) = extract_project_name_from_title(title) {
            if IDE_PROCESS_NAMES.contains(&project_name.as_str()) || !seen.insert(project_name.clone()) {
                continue;
            }

            let pseudo_path = format!("ide://{project_name}");
            items.push((project_name, pseudo_path));
        }
    }

    items
}

fn placeholder_project_snapshot(
    name: &str,
    path: &str,
    active_project_path: &str,
    project_runtime: Option<&ProjectRuntime>,
) -> ProjectSnapshot {
    let hosts = project_runtime
        .map(|runtime| vec![build_codex_project_host_session(runtime, path, false)])
        .unwrap_or_default();
    ProjectSnapshot {
        name: name.into(),
        path: path.into(),
        workflow_stage: WorkflowStage::Unknown,
        gate_status: "未接入 workflow".into(),
        health: "待接入".into(),
        risk: "未知".into(),
        current_req_id: String::new(),
        current_req_title: String::new(),
        current_task_id: String::new(),
        current_task_title: String::new(),
        current_task_status: String::new(),
        current_mode: String::new(),
        last_sync_at: "未同步".into(),
        sync_source: "ide".into(),
        active_host: hosts.first().map(|session| session.host.clone()),
        other_host_summary: String::new(),
        hosts,
        is_blocked: false,
        is_active_by_codex: !active_project_path.is_empty() && path_matches(path, active_project_path),
        is_open_in_ide: true,
        progress_label: "未接入 workflow".into(),
        stage_label: "未同步".into(),
        codex_status: project_runtime
            .map(|runtime| runtime.primary_thread.status.clone())
            .unwrap_or(CodexStatus::Idle),
        codex_heartbeat_at: project_runtime
            .map(|runtime| fmt_relative_age(runtime.primary_thread.last_log_ts))
            .unwrap_or_else(|| "未采集".into()),
        codex_thread_id: project_runtime
            .map(|runtime| runtime.primary_thread.thread.id.clone())
            .unwrap_or_default(),
        codex_thread_name: project_runtime
            .map(|runtime| runtime.primary_thread.thread.title.clone())
            .unwrap_or_else(|| name.into()),
        last_message_role: project_runtime
            .map(|runtime| read_last_thread_message(&runtime.primary_thread.thread.rollout_path).role)
            .unwrap_or_default(),
        last_message_text: project_runtime
            .map(|runtime| read_last_thread_message(&runtime.primary_thread.thread.rollout_path).text)
            .unwrap_or_default(),
        token_total: project_runtime
            .map(|runtime| runtime.token_usage.total)
            .unwrap_or_default(),
        token_input: project_runtime
            .map(|runtime| runtime.token_usage.today_input)
            .unwrap_or_default(),
        token_output: project_runtime
            .map(|runtime| runtime.token_usage.today_output)
            .unwrap_or_default(),
        token_reasoning: project_runtime
            .map(|runtime| runtime.token_usage.today_reasoning)
            .unwrap_or_default(),
        auto_resume_enabled: false,
        follow_up_prompted: project_runtime
            .map(|runtime| runtime.primary_thread.follow_up_prompted)
            .unwrap_or(false),
    }
}

fn read_project_snapshot(
    state_path: &Path,
    active_project_path: &str,
    project_runtime: Option<&ProjectRuntime>,
) -> Option<ProjectSnapshot> {
    let content = fs::read_to_string(state_path).ok()?;
    let payload: ProjectFile = serde_json::from_str(&content).ok()?;
    let stage = stage_from_str(&payload.workflow.stage);
    let gate_status = if payload.workflow.gate_status.is_empty() {
        payload.workflow.gate_status_alias
    } else {
        payload.workflow.gate_status
    };
    let total_tasks = if payload.metrics.total_tasks > 0 {
        payload.metrics.total_tasks
    } else {
        payload.metrics.total_tasks_alias
    };
    let progress_label = if total_tasks > 0 {
        format!("任务 {} / {}", payload.metrics.done, total_tasks)
    } else if !payload.workflow.current_req_id.is_empty() {
        format!("当前需求 {}", payload.workflow.current_req_id)
    } else {
        "等待任务同步".into()
    };
    let project_path = if payload.project.path.is_empty() {
        state_path
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    } else {
        payload.project.path
    };
    let risk = if payload.workflow.risk.is_empty() {
        "未知".into()
    } else {
        payload.workflow.risk
    };
    let health = if payload.workflow.health.is_empty() {
        "待扫描".into()
    } else {
        payload.workflow.health
    };
    let name = if payload.project.name.is_empty() {
        Path::new(&project_path)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "未命名项目".into())
    } else {
        payload.project.name
    };
    let is_blocked = payload.workflow.current_task_status == "blocked"
        || gate_status.contains("阻塞")
        || risk == "高";
    let auto_resume_enabled = project_runtime.is_some()
        && !is_blocked
        && matches!(
            stage,
            WorkflowStage::Bootstrap | WorkflowStage::Requirement | WorkflowStage::Execution
        );
    let hosts = project_runtime
        .map(|runtime| vec![build_codex_project_host_session(runtime, &project_path, auto_resume_enabled)])
        .unwrap_or_default();

    Some(ProjectSnapshot {
        name,
        path: project_path.clone(),
        workflow_stage: stage.clone(),
        gate_status,
        health,
        risk,
        current_req_id: payload.workflow.current_req_id,
        current_req_title: payload.workflow.current_req_title,
        current_task_id: payload.workflow.current_task_id,
        current_task_title: payload.workflow.current_task_title,
        current_task_status: payload.workflow.current_task_status,
        current_mode: payload.workflow.current_mode,
        last_sync_at: format_sync_text(&payload.sync.last_sync_at),
        sync_source: payload.sync.source,
        active_host: hosts.first().map(|session| session.host.clone()),
        other_host_summary: String::new(),
        hosts,
        is_blocked,
        is_active_by_codex: !active_project_path.is_empty() && path_matches(&project_path, active_project_path),
        is_open_in_ide: false,
        progress_label,
        stage_label: stage_label(&stage),
        codex_status: project_runtime
            .map(|runtime| runtime.primary_thread.status.clone())
            .unwrap_or(CodexStatus::Idle),
        codex_heartbeat_at: project_runtime
            .map(|runtime| fmt_relative_age(runtime.primary_thread.last_log_ts))
            .unwrap_or_else(|| "未采集".into()),
        codex_thread_id: project_runtime
            .map(|runtime| runtime.primary_thread.thread.id.clone())
            .unwrap_or_default(),
        codex_thread_name: project_runtime
            .map(|runtime| runtime.primary_thread.thread.title.clone())
            .unwrap_or_default(),
        last_message_role: project_runtime
            .map(|runtime| read_last_thread_message(&runtime.primary_thread.thread.rollout_path).role)
            .unwrap_or_default(),
        last_message_text: project_runtime
            .map(|runtime| read_last_thread_message(&runtime.primary_thread.thread.rollout_path).text)
            .unwrap_or_default(),
        token_total: project_runtime
            .map(|runtime| runtime.token_usage.total)
            .unwrap_or_default(),
        token_input: project_runtime
            .map(|runtime| runtime.token_usage.today_input)
            .unwrap_or_default(),
        token_output: project_runtime
            .map(|runtime| runtime.token_usage.today_output)
            .unwrap_or_default(),
        token_reasoning: project_runtime
            .map(|runtime| runtime.token_usage.today_reasoning)
            .unwrap_or_default(),
        auto_resume_enabled,
        follow_up_prompted: project_runtime
            .map(|runtime| runtime.primary_thread.follow_up_prompted)
            .unwrap_or(false),
    })
}

fn build_groups(projects: &[ProjectSnapshot]) -> Vec<ProjectGroup> {
    let specs = [
        ("execution", "执行中"),
        ("idle", "已接入"),
        ("requirement", "需求中"),
        ("bootstrap", "待初始化"),
        ("blocked", "已阻塞"),
        ("done", "已完成"),
    ];

    specs
        .iter()
        .map(|(key, label)| {
            let items = match *key {
                "execution" => projects
                    .iter()
                    .filter(|item| matches!(item.workflow_stage, WorkflowStage::Execution) && !item.is_blocked)
                    .take(MAX_GROUP_ITEMS)
                    .cloned()
                    .collect(),
                "idle" => projects
                    .iter()
                    .filter(|item| matches!(item.workflow_stage, WorkflowStage::Idle) && !item.is_blocked)
                    .take(MAX_GROUP_ITEMS)
                    .cloned()
                    .collect(),
                "requirement" => projects
                    .iter()
                    .filter(|item| matches!(item.workflow_stage, WorkflowStage::Requirement) && !item.is_blocked)
                    .take(MAX_GROUP_ITEMS)
                    .cloned()
                    .collect(),
                "bootstrap" => projects
                    .iter()
                    .filter(|item| matches!(item.workflow_stage, WorkflowStage::Bootstrap))
                    .take(MAX_GROUP_ITEMS)
                    .cloned()
                    .collect(),
                "blocked" => projects
                    .iter()
                    .filter(|item| item.is_blocked)
                    .take(MAX_GROUP_ITEMS)
                    .cloned()
                    .collect(),
                "done" => projects
                    .iter()
                    .filter(|item| matches!(item.workflow_stage, WorkflowStage::Done))
                    .take(MAX_GROUP_ITEMS)
                    .cloned()
                    .collect(),
                _ => Vec::new(),
            };

            ProjectGroup {
                key: (*key).into(),
                label: (*label).into(),
                items,
            }
        })
        .collect()
}

fn build_summary(projects: &[ProjectSnapshot]) -> Summary {
    let mut summary = Summary {
        idle: 0,
        bootstrap: 0,
        requirement: 0,
        execution: 0,
        blocked: 0,
        done: 0,
    };

    for project in projects {
        if project.is_blocked {
            summary.blocked += 1;
        }

        match project.workflow_stage {
            WorkflowStage::Idle => summary.idle += 1,
            WorkflowStage::Bootstrap => summary.bootstrap += 1,
            WorkflowStage::Requirement => summary.requirement += 1,
            WorkflowStage::Execution => summary.execution += 1,
            WorkflowStage::Done => summary.done += 1,
            WorkflowStage::Unknown => {}
        }
    }

    summary
}

fn project_signature(project: &ProjectSnapshot) -> ProjectRuntimeSignature {
    ProjectRuntimeSignature {
        path: project.path.clone(),
        codex_status: project.codex_status.clone(),
        workflow_stage: project.workflow_stage.clone(),
        task_id: current_task_key(project),
        task_status: project.current_task_status.clone(),
        thread_id: project.codex_thread_id.clone(),
        auto_resume_enabled: project.auto_resume_enabled,
        follow_up_prompted: project.follow_up_prompted,
    }
}

fn codex_status_label(status: &CodexStatus) -> &'static str {
    match status {
        CodexStatus::Running => "执行中",
        CodexStatus::WaitingInput => "等待中",
        CodexStatus::Stalled => "可能卡住",
        CodexStatus::Idle => "空闲",
        CodexStatus::Offline => "离线",
    }
}

fn codex_status_key(status: &CodexStatus) -> &'static str {
    match status {
        CodexStatus::Running => "running",
        CodexStatus::WaitingInput => "waiting_input",
        CodexStatus::Stalled => "stalled",
        CodexStatus::Idle => "idle",
        CodexStatus::Offline => "offline",
    }
}

fn host_kind_label(host: Option<&HostKind>) -> &'static str {
    match host {
        Some(HostKind::Claude) => "Claude",
        _ => "Codex",
    }
}

fn build_codex_global_state(
    codex_status: CodexStatus,
    latest_log_ts: i64,
    latest_thread: Option<&CodexThread>,
    last_message: &ThreadLastMessage,
    spotlight: Option<&ProjectSnapshot>,
    ide_signal: &IdeSignal,
    active_project_path: &str,
    process_running: bool,
    auto_resume_project: Option<&ProjectSnapshot>,
) -> CodexState {
    CodexState {
        status: codex_status,
        heartbeat_at: fmt_relative_age(latest_log_ts),
        active_thread_id: latest_thread
            .map(|thread| thread.id.clone())
            .unwrap_or_default(),
        active_thread_name: latest_thread
            .map(|thread| thread.title.clone())
            .unwrap_or_else(|| "暂无活跃会话".into()),
        last_message_role: last_message.role.clone(),
        last_message_text: last_message.text.clone(),
        active_ide_project_name: spotlight
            .map(|project| project.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| ide_signal.frontmost_project_name.clone()),
        active_project_path: active_project_path.into(),
        source: "state_5.sqlite + logs_2.sqlite".into(),
        confidence: if process_running { "high".into() } else { "medium".into() },
        process_running,
        auto_resume_enabled: auto_resume_project.is_some(),
        monitored_project_name: auto_resume_project
            .map(|project| project.name.clone())
            .unwrap_or_default(),
    }
}

fn build_claude_global_host_session(
    process_running: bool,
    thread: Option<&ClaudeThread>,
    now: i64,
) -> HostSession {
    let log_ts = thread.map(|item| item.updated_at).unwrap_or_default();
    let status = if home_dir().map(|path| path.join(".claude").exists()).unwrap_or(false) {
        codex_status_from_activity(process_running, log_ts, now)
    } else {
        CodexStatus::Offline
    };
    HostSession {
        host: HostKind::Claude,
        status,
        heartbeat_at: fmt_relative_age(log_ts),
        thread_id: thread.map(|item| item.id.clone()).unwrap_or_default(),
        thread_name: thread
            .map(|item| {
                project_name_from_path(&item.project_path)
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| item.project_path.clone())
            })
            .unwrap_or_else(|| "暂无活跃会话".into()),
        project_path: thread.map(|item| item.project_path.clone()).unwrap_or_default(),
        last_message_role: thread.map(|item| item.last_message_role.clone()).unwrap_or_default(),
        last_message_text: thread.map(|item| item.last_message_text.clone()).unwrap_or_default(),
        process_running,
        source: "history.jsonl + projects/*.jsonl".into(),
        confidence: if process_running { "medium".into() } else { "low".into() },
        token_total: 0,
        token_input: 0,
        token_output: 0,
        token_reasoning: 0,
        auto_resume_enabled: false,
        follow_up_prompted: false,
        updated_at: log_ts,
    }
}

fn build_codex_project_host_session(
    runtime: &ProjectRuntime,
    project_path: &str,
    auto_resume_enabled: bool,
) -> HostSession {
    let last_message = read_last_thread_message(&runtime.primary_thread.thread.rollout_path);
    HostSession {
        host: HostKind::Codex,
        status: runtime.primary_thread.status.clone(),
        heartbeat_at: fmt_relative_age(runtime.primary_thread.last_log_ts),
        thread_id: runtime.primary_thread.thread.id.clone(),
        thread_name: runtime.primary_thread.thread.title.clone(),
        project_path: project_path.into(),
        last_message_role: last_message.role,
        last_message_text: last_message.text,
        process_running: !matches!(runtime.primary_thread.status, CodexStatus::Idle | CodexStatus::Offline),
        source: "state_5.sqlite + logs_2.sqlite".into(),
        confidence: "high".into(),
        token_total: runtime.token_usage.total,
        token_input: runtime.token_usage.today_input,
        token_output: runtime.token_usage.today_output,
        token_reasoning: runtime.token_usage.today_reasoning,
        auto_resume_enabled,
        follow_up_prompted: runtime.primary_thread.follow_up_prompted,
        updated_at: runtime.primary_thread.thread.updated_at,
    }
}

fn build_codex_global_host_session(state: &CodexState, updated_at: i64) -> HostSession {
    HostSession {
        host: HostKind::Codex,
        status: state.status.clone(),
        heartbeat_at: state.heartbeat_at.clone(),
        thread_id: state.active_thread_id.clone(),
        thread_name: state.active_thread_name.clone(),
        project_path: state.active_project_path.clone(),
        last_message_role: state.last_message_role.clone(),
        last_message_text: state.last_message_text.clone(),
        process_running: state.process_running,
        source: state.source.clone(),
        confidence: state.confidence.clone(),
        token_total: 0,
        token_input: 0,
        token_output: 0,
        token_reasoning: 0,
        auto_resume_enabled: state.auto_resume_enabled,
        follow_up_prompted: false,
        updated_at,
    }
}

fn host_priority(status: &CodexStatus) -> i32 {
    match status {
        CodexStatus::Running => 5,
        CodexStatus::WaitingInput => 4,
        CodexStatus::Stalled => 3,
        CodexStatus::Idle => 2,
        CodexStatus::Offline => 1,
    }
}

fn host_rank_order(left: &HostSession, right: &HostSession) -> std::cmp::Ordering {
    host_priority(&left.status)
        .cmp(&host_priority(&right.status))
        .then_with(|| left.updated_at.cmp(&right.updated_at))
        .then_with(|| {
            if matches!(left.host, HostKind::Codex) && !matches!(right.host, HostKind::Codex) {
                std::cmp::Ordering::Greater
            } else if !matches!(left.host, HostKind::Codex) && matches!(right.host, HostKind::Codex) {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
}

fn select_active_host_session<'a>(
    hosts: &'a [HostSession],
    preferred_project_path: Option<&str>,
) -> Option<&'a HostSession> {
    if let Some(preferred_project_path) = preferred_project_path {
        let selected = hosts
            .iter()
            .filter(|host| {
                !host.project_path.trim().is_empty()
                    && path_matches(preferred_project_path, &host.project_path)
            })
            .max_by(|left, right| host_rank_order(left, right));
        if selected.is_some() {
            return selected;
        }
    }

    hosts.iter().max_by(|left, right| host_rank_order(left, right))
}

fn should_include_other_host_in_summary(host: &HostSession, now: i64) -> bool {
    let age = if host.updated_at > 0 {
        now.saturating_sub(host.updated_at)
    } else {
        i64::MAX
    };

    match host.status {
        CodexStatus::Running | CodexStatus::WaitingInput => true,
        CodexStatus::Stalled | CodexStatus::Idle => age <= OTHER_HOST_SUMMARY_FRESH_WINDOW_SECONDS,
        CodexStatus::Offline => false,
    }
}

fn other_host_summary_for(hosts: &[HostSession], active_host: Option<&HostKind>, now: i64) -> String {
    let Some(active_host) = active_host else {
        return String::new();
    };
    let mut other_hosts = hosts
        .iter()
        .filter(|host| &host.host != active_host)
        .filter(|host| should_include_other_host_in_summary(host, now))
        .map(|host| host.host.clone())
        .collect::<Vec<_>>();

    other_hosts.sort_by(|left, right| match (left, right) {
        (HostKind::Codex, HostKind::Claude) => std::cmp::Ordering::Less,
        (HostKind::Claude, HostKind::Codex) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    });
    other_hosts.dedup();

    let other_hosts = other_hosts
        .iter()
        .map(|host| match host {
            HostKind::Codex => "Codex".to_string(),
            HostKind::Claude => "Claude".to_string(),
        })
        .collect::<Vec<_>>();

    if other_hosts.is_empty() {
        String::new()
    } else if other_hosts.len() == 1 {
        format!("另有 {} 会话", other_hosts[0])
    } else {
        format!("另有 {} 个 Host 会话", other_hosts.len())
    }
}

fn apply_legacy_codex_fields_from_hosts(project: &mut ProjectSnapshot) {
    let Some(primary) = project
        .hosts
        .iter()
        .find(|session| matches!(session.host, HostKind::Codex))
        .or_else(|| project.hosts.first())
    else {
        return;
    };

    project.codex_status = primary.status.clone();
    project.codex_heartbeat_at = primary.heartbeat_at.clone();
    project.codex_thread_id = primary.thread_id.clone();
    project.codex_thread_name = primary.thread_name.clone();
    if project.last_message_role.is_empty() {
        project.last_message_role = primary.last_message_role.clone();
    }
    if project.last_message_text.is_empty() {
        project.last_message_text = primary.last_message_text.clone();
    }
    if project.token_total == 0 {
        project.token_total = primary.token_total;
        project.token_input = primary.token_input;
        project.token_output = primary.token_output;
        project.token_reasoning = primary.token_reasoning;
    }
}

fn apply_runtime_host_compatibility(state: &mut RuntimeState, now: i64) {
    if state.hosts.is_empty() {
        state.hosts.push(build_codex_global_host_session(&state.codex, now));
    }

    let active_session = select_active_host_session(&state.hosts, None);
    state.active_host = active_session.map(|session| session.host.clone());
    state.other_host_summary = other_host_summary_for(&state.hosts, state.active_host.as_ref(), now);

    for project in &mut state.projects {
        let project_active_session = select_active_host_session(&project.hosts, Some(&project.path));
        project.active_host = project_active_session.map(|session| session.host.clone());
        project.other_host_summary =
            other_host_summary_for(&project.hosts, project.active_host.as_ref(), now);
        apply_legacy_codex_fields_from_hosts(project);
    }
}

fn enrich_projects_with_claude_host(
    projects: &mut [ProjectSnapshot],
    claude_threads: &[ClaudeThread],
    claude_debug_entries: &mut [ClaudeThreadDebugEntry],
    now: i64,
) {
    let process_running = claude_process_running();
    for thread in claude_threads {
        let matched_project = find_best_project_index(projects, &thread.project_path)
            .map(|project_index| (project_index, projects[project_index].path.clone()));

        if let Some(debug_entry) = claude_debug_entries
            .iter_mut()
            .find(|entry| entry.id == thread.id && entry.project_path == thread.project_path)
        {
            match &matched_project {
                Some((project_index, project_path)) => {
                    debug_entry.matched_project_path = project_path.clone();
                    debug_entry.matched_project_name = projects[*project_index].name.clone();
                    debug_entry.match_status = "matched_project".into();
                }
                None => {
                    debug_entry.match_status = "no_project_match".into();
                }
            }
        }

        let Some((project_index, _)) = matched_project else {
            continue;
        };
        let project = &mut projects[project_index];
        if project
            .hosts
            .iter()
            .any(|host| matches!(host.host, HostKind::Claude) && host.thread_id == thread.id)
        {
            continue;
        }
        project
            .hosts
            .push(build_claude_global_host_session(process_running, Some(thread), now));
    }
}

fn project_path_match_score(project_path: &str, thread_project_path: &str) -> Option<(i32, usize)> {
    let project = project_path.trim_end_matches('/');
    let thread = thread_project_path.trim_end_matches('/');

    if project.is_empty() || thread.is_empty() {
        return None;
    }
    if project == thread {
        return Some((4, project.len()));
    }
    if path_matches(project, thread) {
        return Some((3, project.len()));
    }
    None
}

fn preferred_project_match_bonus(project: &ProjectSnapshot, thread_project_path: &str) -> i32 {
    let thread = thread_project_path.trim_end_matches('/');
    if thread.is_empty() {
        return 0;
    }

    let mut bonus = 0;
    if !project.path.trim().is_empty() && project.path.trim_end_matches('/') == thread {
        bonus += 100;
    }
    if !project.is_active_by_codex && project.codex_thread_id.is_empty() {
        bonus += 10;
    }
    if matches!(project.workflow_stage, WorkflowStage::Unknown) {
        bonus += 5;
    }
    bonus
}

fn find_best_project_index(projects: &[ProjectSnapshot], thread_project_path: &str) -> Option<usize> {
    projects
        .iter()
        .enumerate()
        .filter_map(|(index, project)| {
            project_path_match_score(&project.path, thread_project_path).map(|score| {
                (
                    index,
                    (
                        score.0 + preferred_project_match_bonus(project, thread_project_path),
                        score.1,
                    ),
                )
            })
        })
        .max_by(|left, right| left.1.cmp(&right.1))
        .map(|(index, _)| index)
}

fn workflow_stage_key(stage: &WorkflowStage) -> &'static str {
    match stage {
        WorkflowStage::Idle => "idle",
        WorkflowStage::Bootstrap => "bootstrap",
        WorkflowStage::Requirement => "requirement",
        WorkflowStage::Execution => "execution",
        WorkflowStage::Done => "done",
        WorkflowStage::Unknown => "unknown",
    }
}

fn app_alert_settings_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|err| err.to_string())?;
    Ok(dir.join("alert-settings.json"))
}

fn read_alert_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> AlertSettings {
    let path = match app_alert_settings_path(app) {
        Ok(path) => path,
        Err(_) => return read_env_alert_settings(),
    };

    match fs::read_to_string(path) {
        Ok(content) => parse_alert_settings(&content).unwrap_or_else(read_env_alert_settings),
        Err(_) => read_env_alert_settings(),
    }
}

fn sync_legacy_event_toggle(
    payload: &mut serde_json::Map<String, serde_json::Value>,
    legacy_key: &str,
    local_key: &str,
    remote_key: &str,
) {
    let Some(value) = payload.get(legacy_key).cloned() else {
        return;
    };

    if !payload.contains_key(local_key) {
        payload.insert(local_key.into(), value.clone());
    }
    if !payload.contains_key(remote_key) {
        payload.insert(remote_key.into(), value);
    }
}

fn parse_alert_settings(content: &str) -> Option<AlertSettings> {
    let mut payload = serde_json::from_str::<serde_json::Value>(content).ok()?;
    let object = payload.as_object_mut()?;

    sync_legacy_event_toggle(
        object,
        "notify_task_completed",
        "local_notify_task_completed",
        "remote_notify_task_completed",
    );
    sync_legacy_event_toggle(
        object,
        "notify_project_completed",
        "local_notify_project_completed",
        "remote_notify_project_completed",
    );
    sync_legacy_event_toggle(
        object,
        "notify_project_blocked",
        "local_notify_project_blocked",
        "remote_notify_project_blocked",
    );
    sync_legacy_event_toggle(
        object,
        "notify_task_interrupted",
        "local_notify_task_interrupted",
        "remote_notify_task_interrupted",
    );
    sync_legacy_event_toggle(
        object,
        "notify_auto_resume_failed",
        "local_notify_auto_resume_failed",
        "remote_notify_auto_resume_failed",
    );

    serde_json::from_value(payload).ok()
}

fn read_env_alert_settings() -> AlertSettings {
    let provider = env::var("WORKFLOW_ALERT_PROVIDER")
        .unwrap_or_default()
        .trim()
        .to_string();

    if provider == "feishu" {
        let endpoint = env::var("WORKFLOW_ALERT_ENDPOINT").unwrap_or_default();
        if !endpoint.trim().is_empty() {
            return AlertSettings {
                mode: AlertProviderMode::Bridge,
                local_notifications_enabled: true,
                remote_notifications_enabled: true,
                local_notify_task_completed: true,
                remote_notify_task_completed: true,
                local_notify_project_completed: true,
                remote_notify_project_completed: true,
                local_notify_project_blocked: true,
                remote_notify_project_blocked: true,
                local_notify_task_interrupted: true,
                remote_notify_task_interrupted: true,
                local_notify_auto_resume_failed: true,
                remote_notify_auto_resume_failed: true,
                bridge_endpoint: endpoint.trim().into(),
                bridge_token: env::var("WORKFLOW_ALERT_TOKEN").unwrap_or_default(),
                ..AlertSettings::default()
            };
        }
    }

    AlertSettings::default()
}

fn save_alert_settings<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    settings: &AlertSettings,
) -> Result<(), String> {
    let path = app_alert_settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let content = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    fs::write(path, content).map_err(|err| err.to_string())
}

fn alert_dispatch_config(settings: &AlertSettings) -> Option<AlertDispatchConfig> {
    match settings.mode {
        AlertProviderMode::Disabled => None,
        AlertProviderMode::Bridge => {
            let endpoint = settings.bridge_endpoint.trim();
            if endpoint.is_empty() {
                return None;
            }
            Some(AlertDispatchConfig::Bridge {
                endpoint: endpoint.into(),
                token: settings.bridge_token.trim().into(),
            })
        }
        AlertProviderMode::Feishu => {
            let app_id = settings.feishu_app_id.trim();
            let app_secret = settings.feishu_app_secret.trim();
            let open_id = settings.feishu_open_id.trim();
            let chat_id = settings.feishu_chat_id.trim();
            if app_id.is_empty() || app_secret.is_empty() || (open_id.is_empty() && chat_id.is_empty()) {
                return None;
            }
            Some(AlertDispatchConfig::Feishu {
                app_id: app_id.into(),
                app_secret: app_secret.into(),
                open_id: open_id.into(),
                chat_id: chat_id.into(),
            })
        }
    }
}

fn is_notification_enabled(settings: &AlertSettings, event_type: &str, dispatch_remote: bool) -> bool {
    let channel_enabled = if dispatch_remote {
        settings.remote_notifications_enabled
    } else {
        settings.local_notifications_enabled
    };
    if !channel_enabled {
        return false;
    }

    match event_type {
        "task_completed" => {
            if dispatch_remote {
                settings.remote_notify_task_completed
            } else {
                settings.local_notify_task_completed
            }
        }
        "project_completed" => {
            if dispatch_remote {
                settings.remote_notify_project_completed
            } else {
                settings.local_notify_project_completed
            }
        }
        "project_blocked" => {
            if dispatch_remote {
                settings.remote_notify_project_blocked
            } else {
                settings.local_notify_project_blocked
            }
        }
        "task_interrupted" => {
            if dispatch_remote {
                settings.remote_notify_task_interrupted
            } else {
                settings.local_notify_task_interrupted
            }
        }
        "auto_resume_failed" => {
            if dispatch_remote {
                settings.remote_notify_auto_resume_failed
            } else {
                settings.local_notify_auto_resume_failed
            }
        }
        "manual_test" => true,
        _ => true,
    }
}

fn post_bridge_alert(endpoint: &str, token: &str, payload: &RemoteAlertPayload) -> Result<(), String> {
    let mut request = ureq::post(endpoint).set("Content-Type", "application/json");
    if !token.trim().is_empty() {
        request = request.set("Authorization", &format!("Bearer {}", token.trim()));
    }
    request
        .send_json(serde_json::json!({
            "provider": "feishu",
            "payload": payload,
        }))
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn request_feishu_tenant_access_token(app_id: &str, app_secret: &str) -> Result<String, String> {
    let response = ureq::post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "app_id": app_id,
            "app_secret": app_secret,
        }))
        .map_err(|err| err.to_string())?;

    let body: FeishuTenantAccessTokenResponse = response.into_json().map_err(|err| err.to_string())?;
    if body.code != 0 || body.tenant_access_token.trim().is_empty() {
        return Err(format!("feishu token error: {} ({})", body.msg, body.code));
    }
    Ok(body.tenant_access_token)
}

fn post_feishu_alert(
    app_id: &str,
    app_secret: &str,
    open_id: &str,
    chat_id: &str,
    payload: &RemoteAlertPayload,
) -> Result<(), String> {
    let token = request_feishu_tenant_access_token(app_id, app_secret)?;
    let receive_id_type = if !chat_id.trim().is_empty() {
        "chat_id"
    } else {
        "open_id"
    };
    let receive_id = if !chat_id.trim().is_empty() {
        chat_id.trim()
    } else {
        open_id.trim()
    };

    let content = serde_json::json!({
        "text": format!(
            "{}\n{}\n项目：{}\n任务：{}\n阶段：{}\nHost：{}\n状态：{}\n心跳：{}",
            payload.title,
            payload.body,
            if payload.project_name.is_empty() { "未识别" } else { &payload.project_name },
            if payload.task_id.is_empty() { "未识别" } else { &payload.task_id },
            payload.workflow_stage,
            if payload.active_host.is_empty() { "未识别" } else { &payload.active_host },
            payload.codex_status,
            if payload.heartbeat_at.is_empty() { "未采集" } else { &payload.heartbeat_at }
        )
    });

    let response = ureq::post(&format!(
        "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type={receive_id_type}"
    ))
    .set("Authorization", &format!("Bearer {token}"))
    .set("Content-Type", "application/json")
    .send_json(serde_json::json!({
        "receive_id": receive_id,
        "msg_type": "text",
        "content": content.to_string(),
    }))
    .map_err(|err| err.to_string())?;

    let body: FeishuApiResponse = response.into_json().map_err(|err| err.to_string())?;
    if body.code != 0 {
        return Err(format!("feishu send error: {} ({})", body.msg, body.code));
    }
    Ok(())
}

fn post_remote_alert(config: &AlertDispatchConfig, payload: &RemoteAlertPayload) -> Result<(), String> {
    match config {
        AlertDispatchConfig::Bridge { endpoint, token } => post_bridge_alert(endpoint, token, payload),
        AlertDispatchConfig::Feishu {
            app_id,
            app_secret,
            open_id,
            chat_id,
        } => post_feishu_alert(app_id, app_secret, open_id, chat_id, payload),
    }
}

fn find_auto_resume_project<'a>(projects: &'a [ProjectSnapshot], project_path: &str) -> Option<&'a ProjectSnapshot> {
    projects.iter().find(|project| {
        project.auto_resume_enabled
            && !project.is_blocked
            && !project.codex_thread_id.is_empty()
            && path_matches(&project.path, project_path)
    })
}

fn current_task_key(project: &ProjectSnapshot) -> String {
    if !project.current_task_id.is_empty() {
        project.current_task_id.clone()
    } else if !project.current_req_id.is_empty() {
        project.current_req_id.clone()
    } else {
        project.path.clone()
    }
}

fn should_attempt_auto_resume(previous: &CodexStatus, current: &CodexStatus) -> bool {
    matches!(previous, CodexStatus::Running)
        && matches!(current, CodexStatus::Stalled | CodexStatus::Idle)
}

fn should_attempt_follow_up_resume(previous: &ProjectRuntimeSignature, current: &ProjectRuntimeSignature) -> bool {
    !previous.follow_up_prompted
        && current.follow_up_prompted
        && is_follow_up_resume_candidate(&current.codex_status)
}

fn should_skip_auto_resume(
    record: Option<&AutoResumeRecord>,
    thread_id: &str,
    task_id: &str,
    now: i64,
) -> bool {
    let Some(record) = record else {
        return false;
    };

    record.thread_id == thread_id
        && record.task_id == task_id
        && now.saturating_sub(record.attempted_at) < AUTO_RESUME_COOLDOWN_SECONDS
}

fn resolve_codex_command() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    if let Some(path_os) = env::var_os("PATH") {
        candidates.extend(
            env::split_paths(&path_os)
                .map(|dir| dir.join("codex"))
                .collect::<Vec<_>>(),
        );
    }

    if let Some(home) = home_dir() {
        candidates.push(home.join(".npm-global/bin/codex"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/codex"));
    candidates.push(PathBuf::from("/usr/local/bin/codex"));

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "codex command not found".into())
}

fn trigger_auto_resume(project: &ProjectSnapshot, thread_id: &str) -> Result<(), String> {
    if thread_id.trim().is_empty() {
        return Err("missing active thread id".into());
    }
    if matches!(project.workflow_stage, WorkflowStage::Unknown) {
        return Err("workflow stage unknown".into());
    }

    let codex_command = resolve_codex_command()?;
    let prompt = "继续当前任务，请从中断处继续执行；如果最后一条回复是在询问下一步、提示“如果你要继续”、或给出可直接继续的选项，请不要等待用户确认，直接选择最符合当前任务目标的下一步继续推进。";
    Command::new(codex_command)
        .args([
            "exec",
            "--full-auto",
            "--skip-git-repo-check",
            "-C",
            &project.path,
            "resume",
            thread_id,
            prompt,
        ])
        .current_dir(&project.path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn find_project_by_path(projects: &[ProjectSnapshot], path: &str) -> Option<ProjectSnapshot> {
    projects
        .iter()
        .find(|item| path_matches(&item.path, path))
        .cloned()
}

fn rotate_project_paths(paths: &[String]) -> Option<&str> {
    if paths.is_empty() {
        return None;
    }

    let tick = (unix_now() / PROJECT_ROTATION_SECONDS) as usize;
    paths.get(tick % paths.len()).map(|value| value.as_str())
}

fn find_spotlight(
    projects: &[ProjectSnapshot],
    ide_signal: &IdeSignal,
    active_project_path: &str,
) -> Option<ProjectSnapshot> {
    if let Some(project_path) = rotate_project_paths(&ide_signal.frontmost_project_paths) {
        return find_project_by_path(projects, project_path);
    }

    if let Some(project) = find_project_by_path(projects, active_project_path) {
        if project.is_open_in_ide {
            return Some(project);
        }
    }

    if let Some(project_path) = rotate_project_paths(&ide_signal.open_project_paths) {
        return find_project_by_path(projects, project_path);
    }

    projects
        .iter()
        .filter(|item| item.is_open_in_ide)
        .find(|item| matches!(item.workflow_stage, WorkflowStage::Execution))
        .cloned()
        .or_else(|| projects.iter().find(|item| item.is_open_in_ide).cloned())
}

fn collect_runtime_state() -> RuntimeState {
    let home = match home_dir() {
        Some(home) => home,
        None => {
            return RuntimeState {
                codex: CodexState {
                    status: CodexStatus::Offline,
                    heartbeat_at: "不可用".into(),
                    active_thread_id: String::new(),
                    active_thread_name: "无法读取 ~/.codex".into(),
                    last_message_role: String::new(),
                    last_message_text: String::new(),
                    active_ide_project_name: String::new(),
                    active_project_path: String::new(),
                    source: "none".into(),
                    confidence: "low".into(),
                    process_running: false,
                    auto_resume_enabled: false,
                    monitored_project_name: String::new(),
                },
                active_host: None,
                other_host_summary: String::new(),
                hosts: Vec::new(),
                projects: Vec::new(),
                groups: Vec::new(),
                summary: Summary {
                    idle: 0,
                    bootstrap: 0,
                    requirement: 0,
                    execution: 0,
                    blocked: 0,
                    done: 0,
                },
                spotlight_project: None,
                updated_at: "不可用".into(),
            };
        }
    };

    let threads = read_recent_threads(&home);
    let mut token_usage_cache = read_token_usage_cache(&home);
    let latest_thread = threads.first().cloned();
    let latest_log_ts = read_latest_log_ts(&home).unwrap_or_default();
    let thread_log_ts = read_thread_log_ts(&home);
    let process_running = codex_process_running();
    let now = unix_now();
    let codex_status = if !home.join(".codex").exists() {
        CodexStatus::Offline
    } else {
        codex_status_from_activity(process_running, latest_log_ts, now)
    };

    let active_project_path = latest_thread
        .as_ref()
        .map(|thread| thread.cwd.clone())
        .unwrap_or_default();
    let last_message = latest_thread
        .as_ref()
        .map(|thread| read_last_thread_message(&thread.rollout_path))
        .unwrap_or_default();
    let active_thread_id = latest_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    let (claude_threads, mut claude_debug_entries, claude_probe) = read_recent_claude_threads(&home);

    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    let today = local_today();
    let mut project_threads: HashMap<PathBuf, Vec<ThreadRuntime>> = HashMap::new();

    for thread in &threads {
        let cwd = PathBuf::from(&thread.cwd);
        let Some(state_file) = lookup_state_file(&cwd) else {
            continue;
        };
        project_threads
            .entry(state_file.clone())
            .or_default()
            .push(build_thread_runtime(
                thread,
                &thread_log_ts,
                process_running,
                now,
                active_thread_id,
            ));
    }

    for (state_file, mut thread_runtimes) in project_threads {
        if !seen.insert(state_file.clone()) {
            continue;
        }
        thread_runtimes.sort_by(|left, right| {
            right
                .thread
                .updated_at
                .cmp(&left.thread.updated_at)
                .then_with(|| right.last_log_ts.cmp(&left.last_log_ts))
        });
        enrich_primary_thread_runtime(&mut thread_runtimes[0]);
        let project_runtime = ProjectRuntime {
            primary_thread: thread_runtimes[0].clone(),
            token_usage: build_project_token_usage(&thread_runtimes, today, &mut token_usage_cache),
        };
        if let Some(snapshot) = read_project_snapshot(&state_file, &active_project_path, Some(&project_runtime)) {
            projects.push(snapshot);
        }
    }

    let code_titles = read_all_ide_window_titles();
    let mut known_paths = projects
        .iter()
        .map(|project| project.path.clone())
        .collect::<Vec<_>>();
    for thread in &threads {
        if !thread.cwd.trim().is_empty() && !known_paths.iter().any(|path| path == &thread.cwd) {
            known_paths.push(thread.cwd.clone());
        }
    }
    let ide_signal = read_ide_signal(&projects, &known_paths);
    for project in &mut projects {
        project.is_open_in_ide = ide_signal
            .open_project_paths
            .iter()
            .any(|path| path_matches(&project.path, path));
    }

    for (name, pseudo_path) in infer_projects_from_titles(&code_titles) {
        if projects.iter().any(|project| project.name == name || project.path == pseudo_path) {
            continue;
        }
        let mut matched_threads = match_thread_for_placeholder(&name, &threads)
            .into_iter()
            .map(|thread| {
                build_thread_runtime(
                    &thread,
                    &thread_log_ts,
                    process_running,
                    now,
                    active_thread_id,
                )
            })
            .collect::<Vec<_>>();
        matched_threads.sort_by(|left, right| {
            right
                .thread
                .updated_at
                .cmp(&left.thread.updated_at)
                .then_with(|| right.last_log_ts.cmp(&left.last_log_ts))
        });
        let project_runtime = if matched_threads.is_empty() {
            None
        } else {
            enrich_primary_thread_runtime(&mut matched_threads[0]);
            Some(ProjectRuntime {
                primary_thread: matched_threads[0].clone(),
                token_usage: build_project_token_usage(&matched_threads, today, &mut token_usage_cache),
            })
        };
        projects.push(placeholder_project_snapshot(
            &name,
            &pseudo_path,
            &active_project_path,
            project_runtime.as_ref(),
        ));
    }

    for open_path in &ide_signal.open_project_paths {
        if projects.iter().any(|project| path_matches(&project.path, open_path) || path_matches(open_path, &project.path)) {
            continue;
        }

        let mut matched_threads = match_threads_for_path(open_path, &threads)
            .into_iter()
            .map(|thread| {
                build_thread_runtime(
                    &thread,
                    &thread_log_ts,
                    process_running,
                    now,
                    active_thread_id,
                )
            })
            .collect::<Vec<_>>();
        matched_threads.sort_by(|left, right| {
            right
                .thread
                .updated_at
                .cmp(&left.thread.updated_at)
                .then_with(|| right.last_log_ts.cmp(&left.last_log_ts))
        });
        let project_runtime = if matched_threads.is_empty() {
            None
        } else {
            enrich_primary_thread_runtime(&mut matched_threads[0]);
            Some(ProjectRuntime {
                primary_thread: matched_threads[0].clone(),
                token_usage: build_project_token_usage(&matched_threads, today, &mut token_usage_cache),
            })
        };

        let display_name = Path::new(open_path)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| open_path.to_string());

        projects.push(placeholder_project_snapshot(
            &display_name,
            open_path,
            &active_project_path,
            project_runtime.as_ref(),
        ));
    }

    save_token_usage_cache(&home, &token_usage_cache);
    enrich_projects_with_claude_host(&mut projects, &claude_threads, &mut claude_debug_entries, now);

    let spotlight_before_apply = find_spotlight(&projects, &ide_signal, &active_project_path);
    let auto_resume_project = spotlight_before_apply
        .as_ref()
        .and_then(|project| find_auto_resume_project(&projects, &project.path));

    let codex_state = build_codex_global_state(
        codex_status,
        latest_log_ts,
        latest_thread.as_ref(),
        &last_message,
        spotlight_before_apply.as_ref(),
        &ide_signal,
        &active_project_path,
        process_running,
        auto_resume_project,
    );
    let claude_process = claude_process_running();
    let claude_host = build_claude_global_host_session(claude_process, claude_threads.first(), now);
    let hosts = vec![build_codex_global_host_session(&codex_state, now), claude_host];
    let active_host = select_active_host_session(&hosts, None).map(|session| session.host.clone());
    let other_host_summary = other_host_summary_for(&hosts, active_host.as_ref(), now);

    let projects_before_apply = projects.clone();
    let mut runtime = RuntimeState {
        codex: codex_state,
        active_host,
        other_host_summary,
        hosts,
        projects,
        groups: Vec::new(),
        summary: Summary {
            idle: 0,
            bootstrap: 0,
            requirement: 0,
            execution: 0,
            blocked: 0,
            done: 0,
        },
        spotlight_project: None,
        updated_at: fmt_relative_age(now),
    };
    apply_runtime_host_compatibility(&mut runtime, now);
    runtime.spotlight_project = find_spotlight(&runtime.projects, &ide_signal, &active_project_path);
    runtime.groups = build_groups(&runtime.projects);
    runtime.summary = build_summary(&runtime.projects);

    write_runtime_debug_snapshot(
        &code_titles,
        &known_paths,
        &ide_signal,
        &claude_debug_entries,
        Some(claude_probe),
        &projects_before_apply,
        spotlight_before_apply.as_ref(),
        runtime.spotlight_project.as_ref(),
        &runtime.projects,
    );
    runtime
}

fn signature_for(state: &RuntimeState) -> RuntimeSignature {
    let focus = state.spotlight_project.clone();
    RuntimeSignature {
        codex_status: state.codex.status.clone(),
        focus_project_path: focus.as_ref().map(|project| project.path.clone()).unwrap_or_default(),
        focus_task_id: focus
            .as_ref()
            .map(|project| project.current_task_id.clone())
            .unwrap_or_default(),
        focus_task_status: focus
            .as_ref()
            .map(|project| project.current_task_status.clone())
            .unwrap_or_default(),
    }
}

fn maybe_resume_follow_up_on_startup<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    cache: &mut RuntimeCache,
    alert_settings: &SharedAlertSettings,
    current: &RuntimeState,
) {
    if cache.startup_resume_checked {
        return;
    }
    cache.startup_resume_checked = true;

    for project in &current.projects {
        if !project.is_open_in_ide
            || !project.follow_up_prompted
            || !project.auto_resume_enabled
            || project.codex_thread_id.is_empty()
            || !is_follow_up_resume_candidate(&project.codex_status)
        {
            continue;
        }

        let task_id = current_task_key(project);
        let now = unix_now();
        if should_skip_auto_resume(
            cache.last_auto_resume.as_ref(),
            &project.codex_thread_id,
            &task_id,
            now,
        ) {
            continue;
        }

        cache.last_auto_resume = Some(AutoResumeRecord {
            thread_id: project.codex_thread_id.clone(),
            task_id,
            attempted_at: now,
        });

        let body = format!("{} 启动时检测到可继续推进的收尾语气", project.name);
        match trigger_auto_resume(project, &project.codex_thread_id) {
            Ok(()) => push_alert(
                app,
                alert_settings,
                "task_interrupted",
                "项目自动续跑",
                &format!("{body}，已自动尝试续跑"),
                true,
                true,
                Some(project),
            ),
            Err(err) => push_alert(
                app,
                alert_settings,
                "auto_resume_failed",
                "项目自动续跑失败",
                &format!("{body}，自动续跑失败：{err}"),
                true,
                true,
                Some(project),
            ),
        }
    }
}

fn notify_changes<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    cache: &mut RuntimeCache,
    alert_settings: &SharedAlertSettings,
    current: &RuntimeState,
) {
    let current_signature = signature_for(current);

    maybe_resume_follow_up_on_startup(app, cache, alert_settings, current);

    if let Some(previous) = cache.signature.as_ref() {
        if previous.focus_task_status != "blocked" && current_signature.focus_task_status == "blocked" {
            let body = current
                .spotlight_project
                .as_ref()
                .map(|project| format!("{} 已进入阻塞", project.name))
                .unwrap_or_else(|| "当前项目已进入阻塞".into());
            push_alert(
                app,
                alert_settings,
                "project_blocked",
                "项目阻塞",
                &body,
                true,
                true,
                current.spotlight_project.as_ref(),
            );
        }
    }

    let previous_project_signatures = cache.project_signatures.clone();
    let mut next_project_signatures = HashMap::new();

    for project in &current.projects {
        let signature = project_signature(project);
        let previous = previous_project_signatures.get(&project.path);

        if let Some(previous) = previous {
            if previous.task_status != "done" && signature.task_status == "done" {
                let title = if !project.current_task_id.is_empty() {
                    format!("{} · {}", project.name, project.current_task_id)
                } else {
                    project.name.clone()
                };
                let body = if !project.current_task_title.is_empty() {
                    project.current_task_title.clone()
                } else if !project.current_req_title.is_empty() {
                    project.current_req_title.clone()
                } else {
                    "当前任务已完成".into()
                };
                push_alert(
                    app,
                    alert_settings,
                    "task_completed",
                    "任务完成",
                    &format!("{title} · {body}"),
                    true,
                    true,
                    Some(project),
                );
            }

            if !matches!(previous.workflow_stage, WorkflowStage::Done)
                && matches!(project.workflow_stage, WorkflowStage::Done)
            {
                push_alert(
                    app,
                    alert_settings,
                    "project_completed",
                    "项目完成",
                    &format!("{} 已进入完成阶段", project.name),
                    true,
                    true,
                    Some(project),
                );
            }

            let interrupted = should_attempt_auto_resume(&previous.codex_status, &signature.codex_status);
            let follow_up_waiting = should_attempt_follow_up_resume(previous, &signature);

            if interrupted || follow_up_waiting {
                if !project.is_open_in_ide {
                    next_project_signatures.insert(project.path.clone(), signature);
                    continue;
                }
                let stop_body = format!(
                    "{}{}",
                    project.name,
                    if interrupted {
                        format!(
                            " {} 已从执行中切换为 {}",
                            host_kind_label(project.active_host.as_ref()),
                            codex_status_label(&signature.codex_status)
                        )
                    } else {
                        " 最新回复停在可继续推进的收尾语气".into()
                    }
                );

                if signature.auto_resume_enabled {
                    let task_id = signature.task_id.clone();
                    let now = unix_now();

                    if should_skip_auto_resume(
                        cache.last_auto_resume.as_ref(),
                        &signature.thread_id,
                        &task_id,
                        now,
                    ) {
                        push_alert(
                            app,
                            alert_settings,
                            "task_interrupted",
                            "项目执行中断",
                            &format!("{stop_body}，冷却期内跳过自动续跑"),
                            true,
                            true,
                            Some(project),
                        );
                    } else {
                        cache.last_auto_resume = Some(AutoResumeRecord {
                            thread_id: signature.thread_id.clone(),
                            task_id,
                            attempted_at: now,
                        });

                        match trigger_auto_resume(project, &signature.thread_id) {
                            Ok(()) => {
                                push_alert(
                                    app,
                                    alert_settings,
                                    "task_interrupted",
                                    "项目执行中断",
                                    &format!("{stop_body}，已自动尝试续跑"),
                                    true,
                                    true,
                                    Some(project),
                                );
                            }
                            Err(err) => {
                                push_alert(
                                    app,
                                    alert_settings,
                                    "auto_resume_failed",
                                    "项目执行中断",
                                    &format!("{stop_body}，自动续跑失败：{err}"),
                                    true,
                                    true,
                                    Some(project),
                                );
                            }
                        }
                    }
                } else {
                    push_alert(
                        app,
                        alert_settings,
                        "task_interrupted",
                        "项目执行中断",
                        &format!("{stop_body}，该项目未启用自动续跑"),
                        true,
                        true,
                        Some(project),
                    );
                }
            }
        }

        next_project_signatures.insert(project.path.clone(), signature);
    }

    cache.project_signatures = next_project_signatures;
}

fn push_alert<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    alert_settings: &SharedAlertSettings,
    event_type: &str,
    title: &str,
    body: &str,
    _reveal_window: bool,
    dispatch_remote: bool,
    project: Option<&ProjectSnapshot>,
) {
    let settings = alert_settings.lock().map(|guard| guard.clone()).unwrap_or_default();
    if is_notification_enabled(&settings, event_type, false) {
        let _ = app.notification().builder().title(title).body(body).show();
    }
    if dispatch_remote && is_notification_enabled(&settings, event_type, true) {
        if let Some(config) = alert_dispatch_config(&settings) {
            let payload = RemoteAlertPayload {
                event_type: event_type.into(),
                title: title.into(),
                body: body.into(),
                project_name: project.map(|item| item.name.clone()).unwrap_or_default(),
                project_path: project.map(|item| item.path.clone()).unwrap_or_default(),
                active_host: project
                    .map(|item| host_kind_label(item.active_host.as_ref()).to_string())
                    .unwrap_or_else(|| "Codex".into()),
                thread_id: project
                    .map(|item| item.codex_thread_id.clone())
                    .unwrap_or_default(),
                task_id: project.map(current_task_key).unwrap_or_default(),
                task_title: project
                    .map(|item| {
                        if !item.current_task_title.is_empty() {
                            item.current_task_title.clone()
                        } else {
                            item.current_req_title.clone()
                        }
                    })
                    .unwrap_or_default(),
                workflow_stage: project
                    .map(|item| workflow_stage_key(&item.workflow_stage).to_string())
                    .unwrap_or_else(|| "unknown".into()),
                codex_status: project
                    .map(|item| codex_status_key(&item.codex_status).to_string())
                    .unwrap_or_else(|| "unknown".into()),
                heartbeat_at: project
                    .map(|item| item.codex_heartbeat_at.clone())
                    .unwrap_or_default(),
                occurred_at: unix_now(),
            };
            let _ = post_remote_alert(&config, &payload);
        }
    }
}

fn emit_runtime_state<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    cache: &SharedRuntimeCache,
    alert_settings: &SharedAlertSettings,
) {
    let state = collect_runtime_state();

    {
        let mut guard = cache.lock().expect("runtime cache poisoned");
        notify_changes(app, &mut guard, alert_settings, &state);
        guard.signature = Some(signature_for(&state));
        guard.latest = Some(state.clone());
    }

    let _ = app.emit("runtime-state", &state);
}

#[tauri::command]
fn get_runtime_state(state: tauri::State<'_, Arc<Mutex<RuntimeCache>>>) -> RuntimeState {
    if let Some(runtime) = state.lock().expect("runtime cache poisoned").latest.clone() {
        runtime
    } else {
        collect_runtime_state()
    }
}

#[tauri::command]
fn get_alert_settings(state: tauri::State<'_, SharedAlertSettings>) -> AlertSettings {
    state.lock().map(|guard| guard.clone()).unwrap_or_default()
}

#[tauri::command]
fn save_alert_settings_command<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, SharedAlertSettings>,
    settings: AlertSettings,
) -> Result<AlertSettings, String> {
    save_alert_settings(&app, &settings)?;
    let mut guard = state.lock().map_err(|_| "alert settings lock poisoned".to_string())?;
    *guard = settings.clone();
    Ok(settings)
}

#[tauri::command]
fn send_test_alert_command<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, SharedAlertSettings>,
) -> Result<(), String> {
    let settings = state
        .lock()
        .map(|guard| guard.clone())
        .map_err(|_| "alert settings lock poisoned".to_string())?;
    let Some(config) = alert_dispatch_config(&settings) else {
        return Err("提醒配置未启用，或缺少必要字段".into());
    };

    let payload = RemoteAlertPayload {
        event_type: "manual_test".into(),
        title: "workflow-statusbar 测试提醒".into(),
        body: "这是一条手动触发的测试消息，用来确认飞书提醒链路已经打通。".into(),
        project_name: "workflow-statusbar".into(),
        project_path: "/Users/wucongpeng/Documents/ai/skill/workflow-skills-copy/workflow-statusbar".into(),
        active_host: "Codex".into(),
        thread_id: "test-thread".into(),
        task_id: "TEST-ALERT".into(),
        task_title: "验证飞书提醒".into(),
        workflow_stage: "execution".into(),
        codex_status: "running".into(),
        heartbeat_at: "刚刚".into(),
        occurred_at: unix_now(),
    };

    if is_notification_enabled(&settings, &payload.event_type, false) {
        let _ = app
            .notification()
            .builder()
            .title(&payload.title)
            .body(&payload.body)
            .show();
    }

    if is_notification_enabled(&settings, &payload.event_type, true) {
        post_remote_alert(&config, &payload)
    } else {
        Ok(())
    }
}

#[tauri::command]
fn toggle_main_window<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    show_main_window(&app, None)
}

#[tauri::command]
fn schedule_hide_main_window<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    hide_main_window_with_delay(app, None);
    Ok(())
}

#[tauri::command]
fn open_alert_settings_window<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    show_main_window(&app, None)?;
    let _ = app.emit("open-alert-settings", true);
    Ok(())
}

#[tauri::command]
fn sync_main_window_size<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    content_height: f64,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Err("main window missing".into());
    };

    let content_height = content_height.max(180.0);
    let monitor_height = window
        .current_monitor()
        .map_err(|err| err.to_string())?
        .map(|monitor| monitor.size().height as f64)
        .unwrap_or(900.0);
    let next_height = (content_height + 12.0).ceil().min((monitor_height - 96.0).max(260.0));

    window
        .set_size(Size::Logical(tauri::LogicalSize {
            width: 392.0,
            height: next_height,
        }))
        .map_err(|err| err.to_string())?;

    if window.is_visible().map_err(|err| err.to_string())? {
        position_top_center(&window)?;
    }

    Ok(())
}

fn position_top_center<R: tauri::Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    let size = window.outer_size().map_err(|err| err.to_string())?;
    if let Some(monitor) = window.current_monitor().map_err(|err| err.to_string())? {
        let monitor_size = monitor.size();
        let x = ((monitor_size.width as i32 - size.width as i32) / 2).max(12);
        window
            .set_position(Position::Physical(PhysicalPosition { x, y: 34 }))
            .map_err(|err| err.to_string())?;
    } else {
        window.center().map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn rect_contains_cursor(rect: &Rect, cursor: &PhysicalPosition<f64>) -> bool {
    let (x, y) = match rect.position {
        Position::Physical(position) => (position.x as f64, position.y as f64),
        Position::Logical(position) => (position.x, position.y),
    };
    let (width, height) = match rect.size {
        Size::Physical(size) => (size.width as f64, size.height as f64),
        Size::Logical(size) => (size.width, size.height),
    };

    cursor.x >= x && cursor.x <= x + width && cursor.y >= y && cursor.y <= y + height
}

fn window_contains_cursor<R: tauri::Runtime>(
    window: &WebviewWindow<R>,
    cursor: &PhysicalPosition<f64>,
) -> bool {
    let Ok(position) = window.outer_position() else {
        return false;
    };
    let Ok(size) = window.outer_size() else {
        return false;
    };

    let x = position.x as f64;
    let y = position.y as f64;
    let width = size.width as f64;
    let height = size.height as f64;
    cursor.x >= x && cursor.x <= x + width && cursor.y >= y && cursor.y <= y + height
}

fn hide_main_window_with_delay<R: tauri::Runtime>(app: tauri::AppHandle<R>, tray_rect: Option<Rect>) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(TRAY_HIDE_DELAY_MS));
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        if !window.is_visible().unwrap_or(false) {
            return;
        }

        let Ok(cursor) = app.cursor_position() else {
            let _ = window.hide();
            return;
        };

        if let Some(rect) = tray_rect.as_ref() {
            if rect_contains_cursor(rect, &cursor) {
                return;
            }
        }

        if window_contains_cursor(&window, &cursor) {
            return;
        }

        let _ = window.hide();
    });
}

fn show_main_window<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    tray_rect: Option<Rect>,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Err("main window missing".into());
    };

    if let Some(rect) = tray_rect {
        let size = window.outer_size().map_err(|err| err.to_string())?;
        let (tray_x, tray_y) = match rect.position {
            Position::Physical(position) => (position.x, position.y),
            Position::Logical(position) => (position.x as i32, position.y as i32),
        };
        let (tray_w, tray_h) = match rect.size {
            Size::Physical(size) => (size.width as i32, size.height as i32),
            Size::Logical(size) => (size.width as i32, size.height as i32),
        };
        let popup_x = tray_x + (tray_w / 2) - (size.width as i32 / 2);
        let popup_y = tray_y + tray_h + 2;
        window
            .set_position(Position::Physical(PhysicalPosition {
                x: popup_x.max(12),
                y: popup_y.max(12),
            }))
            .map_err(|err| err.to_string())?;
    } else {
        position_top_center(&window)?;
    }

    window.show().map_err(|err| err.to_string())?;
    window.unminimize().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_floating_visibility<R: tauri::Runtime>(app: tauri::AppHandle<R>, visible: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window("floating") else {
        return Err("floating window missing".into());
    };

    if visible {
        window.show().map_err(|err| err.to_string())?;
    } else {
        window.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .status()
        .map_err(|err| err.to_string())
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err("open command failed".into())
            }
        })
}

fn build_tray_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<Menu<R>, String> {
    let open_dashboard = MenuItem::with_id(app, TRAY_MENU_OPEN_DASHBOARD, "打开面板", true, None::<&str>)
        .map_err(|err| err.to_string())?;
    let open_alert_settings = MenuItem::with_id(app, TRAY_MENU_OPEN_ALERT_SETTINGS, "提醒配置", true, None::<&str>)
        .map_err(|err| err.to_string())?;
    let quit = MenuItem::with_id(app, TRAY_MENU_QUIT, "退出", true, None::<&str>)
        .map_err(|err| err.to_string())?;

    Menu::with_items(app, &[&open_dashboard, &open_alert_settings, &quit]).map_err(|err| err.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime_cache: SharedRuntimeCache = Arc::new(Mutex::new(RuntimeCache::default()));

    tauri::Builder::default()
        .manage(runtime_cache.clone())
        .setup(move |app| {
            let alert_settings: SharedAlertSettings = Arc::new(Mutex::new(read_alert_settings(app.handle())));
            app.manage(alert_settings.clone());
            app.set_activation_policy(ActivationPolicy::Accessory);

            let main_window = app.get_webview_window("main").expect("main window should exist");
            let _ = position_top_center(&main_window);
            let _ = main_window.hide();
            let startup_hide_handle = app.handle().clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(400));
                if let Some(window) = startup_hide_handle.get_webview_window("main") {
                    let _ = position_top_center(&window);
                    let _ = window.hide();
                }
            });

            let app_handle = app.handle().clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(false) = event {
                    hide_main_window_with_delay(app_handle.clone(), None);
                }
            });

            let icon = app.default_window_icon().cloned();
            let tray_handle = app.handle().clone();
            let tray_menu = build_tray_menu(app.handle()).map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;
            TrayIconBuilder::new()
                .icon(icon.expect("default icon missing"))
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    TRAY_MENU_OPEN_DASHBOARD => {
                        let _ = show_main_window(app, None);
                    }
                    TRAY_MENU_OPEN_ALERT_SETTINGS => {
                        let _ = open_alert_settings_window(app.clone());
                    }
                    TRAY_MENU_QUIT => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |_tray, event| match event {
                    TrayIconEvent::Enter { rect, .. } => {
                        let _ = show_main_window(&tray_handle, Some(rect));
                    }
                    TrayIconEvent::Leave { rect, .. } => {
                        hide_main_window_with_delay(tray_handle.clone(), Some(rect));
                    }
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } => {
                        let _ = show_main_window(&tray_handle, Some(rect));
                    }
                    _ => {}
                })
                .build(app)?;

            emit_runtime_state(app.handle(), &runtime_cache, &alert_settings);

            let poller_app = app.handle().clone();
            let poller_cache = runtime_cache.clone();
            let poller_alert_settings = alert_settings.clone();
            thread::spawn(move || loop {
                emit_runtime_state(&poller_app, &poller_cache, &poller_alert_settings);
                thread::sleep(Duration::from_secs(POLL_INTERVAL_SECONDS));
            });

            Ok(())
        })
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = show_main_window(&app, None);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_runtime_state,
            get_alert_settings,
            save_alert_settings_command,
            send_test_alert_command,
            toggle_main_window,
            schedule_hide_main_window,
            open_alert_settings_window,
            sync_main_window_size,
            set_floating_visibility,
            open_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project(path: &str) -> ProjectSnapshot {
        let primary_host = HostSession {
            host: HostKind::Codex,
            status: CodexStatus::Running,
            heartbeat_at: "3 秒前".into(),
            thread_id: "thread-1".into(),
            thread_name: "测试线程".into(),
            project_path: path.into(),
            last_message_role: "assistant".into(),
            last_message_text: "继续推进中".into(),
            process_running: true,
            source: "test".into(),
            confidence: "high".into(),
            token_total: 5_300_000,
            token_input: 120_000,
            token_output: 26_800,
            token_reasoning: 9_500,
            auto_resume_enabled: true,
            follow_up_prompted: false,
            updated_at: unix_now(),
        };
        ProjectSnapshot {
            name: "erp-finance".into(),
            path: path.into(),
            workflow_stage: WorkflowStage::Execution,
            gate_status: "待验证".into(),
            health: "正常".into(),
            risk: "低".into(),
            current_req_id: "REQ-1".into(),
            current_req_title: "需求".into(),
            current_task_id: "TASK-1".into(),
            current_task_title: "修复问题".into(),
            current_task_status: "doing".into(),
            current_mode: "execute".into(),
            last_sync_at: "5 秒前".into(),
            sync_source: "test".into(),
            active_host: Some(HostKind::Codex),
            other_host_summary: String::new(),
            hosts: vec![primary_host.clone()],
            is_blocked: false,
            is_active_by_codex: true,
            is_open_in_ide: true,
            progress_label: "任务 1 / 3".into(),
            stage_label: "执行".into(),
            codex_status: CodexStatus::Running,
            codex_heartbeat_at: "3 秒前".into(),
            codex_thread_id: "thread-1".into(),
            codex_thread_name: "测试线程".into(),
            last_message_role: "assistant".into(),
            last_message_text: "继续推进中".into(),
            token_total: 5_300_000,
            token_input: 120_000,
            token_output: 26_800,
            token_reasoning: 9_500,
            auto_resume_enabled: true,
            follow_up_prompted: false,
        }
    }

    #[test]
    fn task_done_transition_is_detected() {
        let mut project = sample_project("/tmp/erp-finance");
        let previous = project_signature(&project);
        project.current_task_status = "done".into();
        let current = project_signature(&project);

        assert_ne!(previous.task_status, current.task_status);
        assert_eq!(current.task_status, "done");
    }

    #[test]
    fn project_done_stage_transition_is_detected() {
        let mut project = sample_project("/tmp/erp-finance");
        let previous = project_signature(&project);
        project.workflow_stage = WorkflowStage::Done;
        let current = project_signature(&project);

        assert!(!matches!(previous.workflow_stage, WorkflowStage::Done));
        assert!(matches!(current.workflow_stage, WorkflowStage::Done));
    }

    #[test]
    fn task_interrupt_transition_is_detected() {
        let mut project = sample_project("/tmp/erp-finance");
        let previous = project_signature(&project);
        project.codex_status = CodexStatus::Stalled;
        let current = project_signature(&project);

        assert!(should_attempt_auto_resume(
            &previous.codex_status,
            &current.codex_status
        ));
    }

    #[test]
    fn waiting_input_transition_is_not_treated_as_interrupt() {
        let mut project = sample_project("/tmp/erp-finance");
        let previous = project_signature(&project);
        project.codex_status = CodexStatus::WaitingInput;
        let current = project_signature(&project);

        assert!(!should_attempt_auto_resume(
            &previous.codex_status,
            &current.codex_status
        ));
    }

    #[test]
    fn unknown_workflow_project_cannot_auto_resume() {
        let mut project = sample_project("/tmp/skill");
        project.workflow_stage = WorkflowStage::Unknown;

        let result = trigger_auto_resume(&project, &project.codex_thread_id);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "workflow stage unknown");
    }

    #[test]
    fn placeholder_project_does_not_enable_auto_resume() {
        let project = placeholder_project_snapshot("skill", "ide://skill", "", None);

        assert!(matches!(project.workflow_stage, WorkflowStage::Unknown));
        assert!(!project.auto_resume_enabled);
    }

    #[test]
    fn idle_stage_is_treated_as_linked_but_not_auto_resumable() {
        let mut project = sample_project("/tmp/b2c");
        project.workflow_stage = WorkflowStage::Idle;
        project.auto_resume_enabled = false;

        assert!(matches!(project.workflow_stage, WorkflowStage::Idle));
        assert!(!project.auto_resume_enabled);
        assert_eq!(workflow_stage_key(&project.workflow_stage), "idle");
        assert_eq!(stage_label(&project.workflow_stage), "已接入");
    }

    #[test]
    fn follow_up_prompt_transition_is_detected() {
        let mut project = sample_project("/tmp/solo");
        let previous = project_signature(&project);
        project.follow_up_prompted = true;
        project.codex_status = CodexStatus::WaitingInput;
        let current = project_signature(&project);

        assert!(should_attempt_follow_up_resume(&previous, &current));
    }

    #[test]
    fn follow_up_prompt_while_running_is_not_treated_as_interrupt() {
        let mut project = sample_project("/tmp/solo");
        let previous = project_signature(&project);
        project.follow_up_prompted = true;
        let current = project_signature(&project);

        assert!(!should_attempt_follow_up_resume(&previous, &current));
    }

    #[test]
    fn running_status_is_not_follow_up_resume_candidate() {
        assert!(!is_follow_up_resume_candidate(&CodexStatus::Running));
        assert!(is_follow_up_resume_candidate(&CodexStatus::WaitingInput));
        assert!(is_follow_up_resume_candidate(&CodexStatus::Stalled));
        assert!(is_follow_up_resume_candidate(&CodexStatus::Idle));
    }

    #[test]
    fn codex_command_can_be_resolved_in_local_environment() {
        let command = resolve_codex_command();

        assert!(command.is_ok());
        assert!(command.unwrap().is_file());
    }

    #[test]
    fn path_matches_requires_candidate_to_be_inside_project() {
        assert!(path_matches("/tmp/solo", "/tmp/solo"));
        assert!(path_matches("/tmp/solo", "/tmp/solo/.ai/runtime/project-state.json"));
        assert!(!path_matches("/tmp/solo", "/tmp"));
        assert!(!path_matches("/tmp/solo", "/tmp/solo-backup"));
    }

    #[test]
    fn select_active_host_prefers_status_even_if_updated_at_older() {
        let hosts = vec![
            HostSession {
                host: HostKind::Codex,
                status: CodexStatus::WaitingInput,
                heartbeat_at: "30 秒前".into(),
                thread_id: "codex-1".into(),
                thread_name: "codex".into(),
                project_path: "/tmp/solo".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: true,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: 120,
            },
            HostSession {
                host: HostKind::Claude,
                status: CodexStatus::Running,
                heartbeat_at: "10 秒前".into(),
                thread_id: "claude-1".into(),
                thread_name: "claude".into(),
                project_path: "/tmp/solo".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: true,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: 100,
            },
        ];
        let selected = select_active_host_session(&hosts, None).expect("active host should exist");
        assert!(matches!(selected.host, HostKind::Claude));
    }

    #[test]
    fn select_active_host_prefers_newer_updated_at_when_status_same() {
        let hosts = vec![
            HostSession {
                host: HostKind::Codex,
                status: CodexStatus::Stalled,
                heartbeat_at: "1 分钟前".into(),
                thread_id: "codex-1".into(),
                thread_name: "codex".into(),
                project_path: "/tmp/solo".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: true,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: 80,
            },
            HostSession {
                host: HostKind::Claude,
                status: CodexStatus::Stalled,
                heartbeat_at: "30 秒前".into(),
                thread_id: "claude-1".into(),
                thread_name: "claude".into(),
                project_path: "/tmp/solo".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: true,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: 100,
            },
        ];
        let selected = select_active_host_session(&hosts, None).expect("active host should exist");
        assert!(matches!(selected.host, HostKind::Claude));
    }

    #[test]
    fn select_active_host_prefers_codex_on_full_tie() {
        let hosts = vec![
            HostSession {
                host: HostKind::Codex,
                status: CodexStatus::Idle,
                heartbeat_at: "1 分钟前".into(),
                thread_id: "codex-1".into(),
                thread_name: "codex".into(),
                project_path: "/tmp/solo".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: false,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: 50,
            },
            HostSession {
                host: HostKind::Claude,
                status: CodexStatus::Idle,
                heartbeat_at: "1 分钟前".into(),
                thread_id: "claude-1".into(),
                thread_name: "claude".into(),
                project_path: "/tmp/solo".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: false,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: 50,
            },
        ];
        let selected = select_active_host_session(&hosts, None).expect("active host should exist");
        assert!(matches!(selected.host, HostKind::Codex));
    }

    #[test]
    fn select_active_host_prefers_matching_project_path() {
        let hosts = vec![
            HostSession {
                host: HostKind::Codex,
                status: CodexStatus::Idle,
                heartbeat_at: "1 分钟前".into(),
                thread_id: "codex-1".into(),
                thread_name: "codex".into(),
                project_path: "/tmp/other".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: false,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: 50,
            },
            HostSession {
                host: HostKind::Claude,
                status: CodexStatus::Idle,
                heartbeat_at: "2 分钟前".into(),
                thread_id: "claude-1".into(),
                thread_name: "claude".into(),
                project_path: "/tmp/solo".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: false,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: 100,
            },
        ];
        let selected = select_active_host_session(&hosts, Some("/tmp/solo/subdir"))
            .expect("active host should exist");
        assert!(matches!(selected.host, HostKind::Claude));
    }

    #[test]
    fn other_host_summary_ignores_stale_sessions() {
        let now = 10_000;
        let hosts = vec![
            HostSession {
                host: HostKind::Codex,
                status: CodexStatus::Running,
                heartbeat_at: String::new(),
                thread_id: "codex-1".into(),
                thread_name: String::new(),
                project_path: "/tmp/solo".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: true,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: now - 10,
            },
            HostSession {
                host: HostKind::Claude,
                status: CodexStatus::Stalled,
                heartbeat_at: String::new(),
                thread_id: "claude-old".into(),
                thread_name: String::new(),
                project_path: "/tmp/solo".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: true,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: now - (OTHER_HOST_SUMMARY_FRESH_WINDOW_SECONDS + 1),
            },
        ];

        let summary = other_host_summary_for(&hosts, Some(&HostKind::Codex), now);
        assert!(summary.is_empty());
    }

    #[test]
    fn other_host_summary_dedups_same_host_kind() {
        let now = 10_000;
        let hosts = vec![
            HostSession {
                host: HostKind::Codex,
                status: CodexStatus::WaitingInput,
                heartbeat_at: String::new(),
                thread_id: "codex-1".into(),
                thread_name: String::new(),
                project_path: "/tmp/skill".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: true,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: now - 20,
            },
            HostSession {
                host: HostKind::Claude,
                status: CodexStatus::Stalled,
                heartbeat_at: String::new(),
                thread_id: "claude-a".into(),
                thread_name: String::new(),
                project_path: "/tmp/skill".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: true,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: now - 100,
            },
            HostSession {
                host: HostKind::Claude,
                status: CodexStatus::Stalled,
                heartbeat_at: String::new(),
                thread_id: "claude-b".into(),
                thread_name: String::new(),
                project_path: "/tmp/skill".into(),
                last_message_role: String::new(),
                last_message_text: String::new(),
                process_running: true,
                source: "test".into(),
                confidence: "high".into(),
                token_total: 0,
                token_input: 0,
                token_output: 0,
                token_reasoning: 0,
                auto_resume_enabled: false,
                follow_up_prompted: false,
                updated_at: now - 200,
            },
        ];

        let summary = other_host_summary_for(&hosts, Some(&HostKind::Codex), now);
        assert_eq!(summary, "另有 Claude 会话");
    }

    #[test]
    fn find_best_project_prefers_exact_path_over_broader_path() {
        let mut broad = sample_project("/Users/wucongpeng");
        broad.name = "broad".into();
        let mut exact = sample_project("/Users/wucongpeng/Documents/ai/skill");
        exact.name = "exact".into();
        let projects = vec![broad, exact];

        let index = find_best_project_index(&projects, "/Users/wucongpeng/Documents/ai/skill")
            .expect("project should match");
        assert_eq!(projects[index].name, "exact");
    }

    #[test]
    fn find_best_project_prefers_longer_prefix_when_both_match() {
        let mut parent = sample_project("/Users/wucongpeng/Documents/ai");
        parent.name = "parent".into();
        let mut child = sample_project("/Users/wucongpeng/Documents/ai/skill");
        child.name = "child".into();
        let projects = vec![parent, child];

        let index = find_best_project_index(&projects, "/Users/wucongpeng/Documents/ai/skill/workflow-skills-copy")
            .expect("project should match");
        assert_eq!(projects[index].name, "child");
    }

    #[test]
    fn find_best_project_does_not_match_parent_thread_path() {
        let mut broad = sample_project("/Users/wucongpeng");
        broad.name = "broad".into();
        let mut exact = sample_project("/Users/wucongpeng/Documents/ai/skill");
        exact.name = "exact".into();
        let projects = vec![broad, exact];

        let index = find_best_project_index(&projects, "/Users/wucongpeng")
            .expect("project should match root only");
        assert_eq!(projects[index].name, "broad");
        assert!(find_best_project_index(&projects, "/Users/wucongpeng/Documents").is_some());
    }
}
