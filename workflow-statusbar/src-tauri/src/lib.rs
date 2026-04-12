use dirs::home_dir;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    ActivationPolicy, Emitter, Manager, PhysicalPosition, Position, Rect, Size, WebviewWindow,
};
use tauri_plugin_notification::NotificationExt;

const LOOKUP_LIMIT: usize = 12;
const MAX_GROUP_ITEMS: usize = 5;
const PROJECT_ROTATION_SECONDS: i64 = 8;
const AUTO_RESUME_COOLDOWN_SECONDS: i64 = 90;

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
    progress_label: String,
    stage_label: String,
    codex_status: CodexStatus,
    codex_heartbeat_at: String,
    codex_thread_id: String,
    codex_thread_name: String,
    auto_resume_enabled: bool,
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
    task_id: String,
    task_status: String,
    thread_id: String,
    auto_resume_enabled: bool,
}

#[derive(Default)]
struct RuntimeCache {
    latest: Option<RuntimeState>,
    signature: Option<RuntimeSignature>,
    project_signatures: HashMap<String, ProjectRuntimeSignature>,
    last_auto_resume: Option<AutoResumeRecord>,
}

#[derive(Clone, Debug)]
struct AutoResumeRecord {
    thread_id: String,
    task_id: String,
    attempted_at: i64,
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
}

#[derive(Clone)]
struct ThreadRuntime {
    thread: CodexThread,
    last_log_ts: i64,
    status: CodexStatus,
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
        "select id, title, cwd from threads order by updated_at desc limit ?1",
    ) {
        Ok(statement) => statement,
        Err(_) => return Vec::new(),
    };

    let rows = statement.query_map([LOOKUP_LIMIT as i64], |row| {
        Ok(CodexThread {
            id: row.get(0)?,
            title: row.get(1)?,
            cwd: row.get(2)?,
        })
    });

    match rows {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => Vec::new(),
    }
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
        task_id: current_task_key(project),
        task_status: project.current_task_status.clone(),
        thread_id: project.codex_thread_id.clone(),
        auto_resume_enabled: project.auto_resume_enabled,
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

    let prompt = "继续当前任务，请从中断处继续执行；如果没有新的用户输入要求，直接按既定计划推进。";
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

fn notify_changes<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    cache: &mut RuntimeCache,
    current: &RuntimeState,
) {
    let current_signature = signature_for(current);

    if let Some(previous) = cache.signature.as_ref() {
        if previous.codex_status == CodexStatus::Running && current_signature.codex_status != CodexStatus::Running {
            let _ = app
                .notification()
                .builder()
                .title("Codex 状态变化")
                .body(format!(
                    "全局 Codex 状态已切换为 {}",
                    codex_status_label(&current_signature.codex_status)
                ))
                .show();
        }

        if previous.focus_task_id != current_signature.focus_task_id && !current_signature.focus_task_id.is_empty() {
            let body = current
                .spotlight_project
                .as_ref()
                .map(|project| format!("{} · {}", project.name, project.current_task_title))
                .unwrap_or_else(|| "当前任务已切换".into());
            let _ = app.notification().builder().title("任务已切换").body(body).show();
        }

        if previous.focus_task_status != "blocked" && current_signature.focus_task_status == "blocked" {
            let body = current
                .spotlight_project
                .as_ref()
                .map(|project| format!("{} 已进入阻塞", project.name))
                .unwrap_or_else(|| "当前项目已进入阻塞".into());
            let _ = app.notification().builder().title("项目阻塞").body(body).show();
        }
    }

    let previous_project_signatures = cache.project_signatures.clone();
    let mut next_project_signatures = HashMap::new();

    for project in &current.projects {
        let signature = project_signature(project);
        let previous = previous_project_signatures.get(&project.path);

        if let Some(previous) = previous {
            if should_attempt_auto_resume(&previous.codex_status, &signature.codex_status) {
                let stop_body = format!(
                    "{} 已从执行中切换为 {}",
                    project.name,
                    codex_status_label(&signature.codex_status)
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
                        let _ = app
                            .notification()
                            .builder()
                            .title("项目执行中断")
                            .body(format!("{stop_body}，冷却期内跳过自动续跑"))
                            .show();
                    } else {
                        cache.last_auto_resume = Some(AutoResumeRecord {
                            thread_id: signature.thread_id.clone(),
                            task_id,
                            attempted_at: now,
                        });

                        match trigger_auto_resume(project, &signature.thread_id) {
                            Ok(()) => {
                                let _ = app
                                    .notification()
                                    .builder()
                                    .title("项目执行中断")
                                    .body(format!("{stop_body}，已自动尝试续跑"))
                                    .show();
                            }
                            Err(err) => {
                                let _ = app
                                    .notification()
                                    .builder()
                                    .title("项目执行中断")
                                    .body(format!("{stop_body}，自动续跑失败：{err}"))
                                    .show();
                            }
                        }
                    }
                } else {
                    let _ = app
                        .notification()
                        .builder()
                        .title("项目执行中断")
                        .body(format!("{stop_body}，该项目未启用自动续跑"))
                        .show();
                }
            }
        }

        next_project_signatures.insert(project.path.clone(), signature);
    }

    cache.project_signatures = next_project_signatures;
}

fn emit_runtime_state<R: tauri::Runtime>(app: &tauri::AppHandle<R>, cache: &Arc<Mutex<RuntimeCache>>) {
    let state = collect_runtime_state();

    {
        let mut guard = cache.lock().expect("runtime cache poisoned");
        notify_changes(app, &mut guard, &state);
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
fn toggle_main_window<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    show_main_window(&app, None)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime_cache: Arc<Mutex<RuntimeCache>> = Arc::new(Mutex::new(RuntimeCache::default()));

    tauri::Builder::default()
        .manage(runtime_cache.clone())
        .setup(move |app| {
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
            TrayIconBuilder::new()
                .icon(icon.expect("default icon missing"))
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

            emit_runtime_state(app.handle(), &runtime_cache);

            let poller_app = app.handle().clone();
            let poller_cache = runtime_cache.clone();
            thread::spawn(move || loop {
                emit_runtime_state(&poller_app, &poller_cache);
                thread::sleep(Duration::from_secs(5));
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
            toggle_main_window,
            set_floating_visibility,
            open_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
