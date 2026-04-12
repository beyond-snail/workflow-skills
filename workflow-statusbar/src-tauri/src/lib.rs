use dirs::home_dir;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs,
    io::{BufRead, BufReader},
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

const LOOKUP_LIMIT: usize = 12;
const MAX_GROUP_ITEMS: usize = 5;
const PROJECT_ROTATION_SECONDS: i64 = 8;
const AUTO_RESUME_COOLDOWN_SECONDS: i64 = 90;
const POLL_INTERVAL_SECONDS: u64 = 8;
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

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkflowStage {
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
    is_blocked: bool,
    is_active_by_codex: bool,
    is_open_in_ide: bool,
    progress_label: String,
    stage_label: String,
    codex_status: CodexStatus,
    codex_heartbeat_at: String,
    codex_thread_id: String,
    codex_thread_name: String,
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
    bootstrap: usize,
    requirement: usize,
    execution: usize,
    blocked: usize,
    done: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct RuntimeState {
    codex: CodexState,
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
    thread_id: String,
    task_id: String,
    task_title: String,
    workflow_stage: String,
    codex_status: String,
    heartbeat_at: String,
    occurred_at: i64,
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
}

#[derive(Clone)]
struct ThreadRuntime {
    thread: CodexThread,
    last_log_ts: i64,
    status: CodexStatus,
    follow_up_prompted: bool,
}

#[derive(Clone, Debug)]
struct IdeProcess {
    pid: i32,
}

#[derive(Default)]
struct IdeSignal {
    frontmost_project_paths: Vec<String>,
    open_project_paths: Vec<String>,
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
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

    candidate == project
        || candidate.starts_with(&(project.to_string() + "/"))
        || project.starts_with(&(candidate.to_string() + "/"))
}

fn stage_from_str(input: &str) -> WorkflowStage {
    match input {
        "bootstrap" => WorkflowStage::Bootstrap,
        "requirement" => WorkflowStage::Requirement,
        "execution" => WorkflowStage::Execution,
        "done" => WorkflowStage::Done,
        _ => WorkflowStage::Unknown,
    }
}

fn stage_label(stage: &WorkflowStage) -> String {
    match stage {
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
        "select id, title, cwd, rollout_path from threads order by updated_at desc limit ?1",
    ) {
        Ok(statement) => statement,
        Err(_) => return Vec::new(),
    };

    let rows = statement.query_map([LOOKUP_LIMIT as i64], |row| {
        Ok(CodexThread {
            id: row.get(0)?,
            title: row.get(1)?,
            cwd: row.get(2)?,
            rollout_path: row.get(3)?,
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

    let file = match fs::File::open(rollout_path) {
        Ok(file) => file,
        Err(_) => return false,
    };

    let markers = [
        "下一步可以直接做",
        "如果你要继续",
        "如果要继续",
        "继续的话",
        "你发我一个主题",
        "我直接继续",
        "直接进入",
    ];

    let mut matched = false;
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        if !line.contains("\"type\":\"event_msg\"")
            && !line.contains("\"type\":\"response_item\"")
            && !line.contains("\"last_agent_message\"")
        {
            continue;
        }

        if markers.iter().any(|marker| line.contains(marker)) {
            matched = true;
        }
    }

    matched
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
        "/Cursor.app/",
        "/Windsurf.app/",
        "/Trae.app/",
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
        "tell application \"System Events\" to tell process \"{process_name}\" to get name of every window"
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

fn project_paths_for_pid(pid: i32, projects: &[ProjectSnapshot]) -> Vec<String> {
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
        for project in projects {
            if path_matches(&project.path, path) && seen.insert(project.path.clone()) {
                matched.push(project.path.clone());
            }
        }
    }

    matched
}

fn read_ide_signal(projects: &[ProjectSnapshot]) -> IdeSignal {
    if projects.is_empty() {
        return IdeSignal::default();
    }

    let frontmost_pid = read_frontmost_pid();
    let frontmost_app_name = read_frontmost_app_name();
    let mut frontmost_project_paths = Vec::new();
    let mut open_project_paths = Vec::new();
    let mut seen = HashSet::new();

    let code_titles = read_window_titles("Code");
    let code_title_paths = project_paths_from_titles(projects, &code_titles);
    for path in &code_title_paths {
        if seen.insert(path.clone()) {
            open_project_paths.push(path.clone());
        }
    }

    for process in read_ide_processes() {
        let project_paths = project_paths_for_pid(process.pid, projects);
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

    if frontmost_project_paths.is_empty() && matches!(frontmost_app_name.as_deref(), Some("Code")) {
        frontmost_project_paths = code_title_paths.clone();
    }

    IdeSignal {
        frontmost_project_paths,
        open_project_paths,
    }
}

fn read_project_snapshot(
    state_path: &Path,
    active_project_path: &str,
    thread_runtime: Option<&ThreadRuntime>,
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
        is_blocked,
        is_active_by_codex: !active_project_path.is_empty() && path_matches(&project_path, active_project_path),
        is_open_in_ide: false,
        progress_label,
        stage_label: stage_label(&stage),
        codex_status: thread_runtime
            .map(|runtime| runtime.status.clone())
            .unwrap_or(CodexStatus::Idle),
        codex_heartbeat_at: thread_runtime
            .map(|runtime| fmt_relative_age(runtime.last_log_ts))
            .unwrap_or_else(|| "未采集".into()),
        codex_thread_id: thread_runtime
            .map(|runtime| runtime.thread.id.clone())
            .unwrap_or_default(),
        codex_thread_name: thread_runtime
            .map(|runtime| runtime.thread.title.clone())
            .unwrap_or_default(),
        auto_resume_enabled: thread_runtime.is_some() && !is_blocked,
        follow_up_prompted: thread_runtime
            .map(|runtime| runtime.follow_up_prompted)
            .unwrap_or(false),
    })
}

fn build_groups(projects: &[ProjectSnapshot]) -> Vec<ProjectGroup> {
    let specs = [
        ("execution", "执行中"),
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

fn workflow_stage_key(stage: &WorkflowStage) -> &'static str {
    match stage {
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
            "{}\n{}\n项目：{}\n任务：{}\n阶段：{}\nCodex：{}\n心跳：{}",
            payload.title,
            payload.body,
            if payload.project_name.is_empty() { "未识别" } else { &payload.project_name },
            if payload.task_id.is_empty() { "未识别" } else { &payload.task_id },
            payload.workflow_stage,
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
        && matches!(
            current,
            CodexStatus::WaitingInput | CodexStatus::Stalled | CodexStatus::Idle
        )
}

fn should_attempt_follow_up_resume(previous: &ProjectRuntimeSignature, current: &ProjectRuntimeSignature) -> bool {
    !previous.follow_up_prompted && current.follow_up_prompted
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

fn trigger_auto_resume(project: &ProjectSnapshot, thread_id: &str) -> Result<(), String> {
    if thread_id.trim().is_empty() {
        return Err("missing active thread id".into());
    }

    let prompt = "继续当前任务，请从中断处继续执行；如果最后一条回复是在询问下一步、提示“如果你要继续”、或给出可直接继续的选项，请不要等待用户确认，直接选择最符合当前任务目标的下一步继续推进。";
    Command::new("codex")
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
        return Some(project);
    }

    if let Some(project_path) = rotate_project_paths(&ide_signal.open_project_paths) {
        return find_project_by_path(projects, project_path);
    }

    projects
        .iter()
        .find(|item| matches!(item.workflow_stage, WorkflowStage::Execution))
        .cloned()
        .or_else(|| projects.first().cloned())
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
                    active_project_path: String::new(),
                    source: "none".into(),
                    confidence: "low".into(),
                    process_running: false,
                    auto_resume_enabled: false,
                    monitored_project_name: String::new(),
                },
                projects: Vec::new(),
                groups: Vec::new(),
                summary: Summary {
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

    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    let mut project_threads: HashMap<PathBuf, ThreadRuntime> = HashMap::new();

    for thread in &threads {
        let cwd = PathBuf::from(&thread.cwd);
        let Some(state_file) = lookup_state_file(&cwd) else {
            continue;
        };
        project_threads.entry(state_file.clone()).or_insert_with(|| {
            let last_log_ts = thread_log_ts.get(&thread.id).copied().unwrap_or_default();
            ThreadRuntime {
                thread: thread.clone(),
                last_log_ts,
                status: codex_status_from_activity(process_running, last_log_ts, now),
                follow_up_prompted: detect_follow_up_prompt(&thread.rollout_path),
            }
        });
    }

    for (state_file, thread_runtime) in project_threads {
        if !seen.insert(state_file.clone()) {
            continue;
        }
        if let Some(snapshot) = read_project_snapshot(&state_file, &active_project_path, Some(&thread_runtime)) {
            projects.push(snapshot);
        }
    }

    let ide_signal = read_ide_signal(&projects);
    for project in &mut projects {
        project.is_open_in_ide = ide_signal
            .open_project_paths
            .iter()
            .any(|path| path_matches(&project.path, path));
    }
    let spotlight = find_spotlight(&projects, &ide_signal, &active_project_path);
    let groups = build_groups(&projects);
    let summary = build_summary(&projects);
    let auto_resume_project = spotlight
        .as_ref()
        .and_then(|project| find_auto_resume_project(&projects, &project.path));

    RuntimeState {
        codex: CodexState {
            status: codex_status,
            heartbeat_at: fmt_relative_age(latest_log_ts),
            active_thread_id: latest_thread
                .as_ref()
                .map(|thread| thread.id.clone())
                .unwrap_or_default(),
            active_thread_name: latest_thread
                .as_ref()
                .map(|thread| thread.title.clone())
                .unwrap_or_else(|| "暂无活跃会话".into()),
            active_project_path,
            source: "state_5.sqlite + logs_2.sqlite".into(),
            confidence: if process_running { "high".into() } else { "medium".into() },
            process_running,
            auto_resume_enabled: auto_resume_project.is_some(),
            monitored_project_name: auto_resume_project
                .map(|project| project.name.clone())
                .unwrap_or_default(),
        },
        projects,
        groups,
        summary,
        spotlight_project: spotlight,
        updated_at: fmt_relative_age(now),
    }
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
        if previous.focus_task_id != current_signature.focus_task_id && !current_signature.focus_task_id.is_empty() {
            let body = current
                .spotlight_project
                .as_ref()
                .map(|project| format!("{} · {}", project.name, project.current_task_title))
                .unwrap_or_else(|| "当前任务已切换".into());
            push_alert(
                app,
                alert_settings,
                "task_switched",
                "任务已切换",
                &body,
                false,
                false,
                current.spotlight_project.as_ref(),
            );
        }

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
                        format!(" 已从执行中切换为 {}", codex_status_label(&signature.codex_status))
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
    reveal_window: bool,
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
    if reveal_window {
        let _ = show_main_window(app, None);
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
fn open_alert_settings_window<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    show_main_window(&app, None)?;
    let _ = app.emit("open-alert-settings", true);
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
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.hide();
                    }
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
                .on_tray_icon_event(move |_tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        if let Some(window) = tray_handle.get_webview_window("main") {
                            let is_visible = window.is_visible().unwrap_or(false);
                            if is_visible {
                                let _ = window.hide();
                            } else {
                                let _ = show_main_window(&tray_handle, Some(rect));
                            }
                        }
                    }
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
            open_alert_settings_window,
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
            is_blocked: false,
            is_active_by_codex: true,
            is_open_in_ide: true,
            progress_label: "任务 1 / 3".into(),
            stage_label: "执行".into(),
            codex_status: CodexStatus::Running,
            codex_heartbeat_at: "3 秒前".into(),
            codex_thread_id: "thread-1".into(),
            codex_thread_name: "测试线程".into(),
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
    fn follow_up_prompt_transition_is_detected() {
        let mut project = sample_project("/tmp/solo");
        let previous = project_signature(&project);
        project.follow_up_prompted = true;
        let current = project_signature(&project);

        assert!(should_attempt_follow_up_resume(&previous, &current));
    }
}
