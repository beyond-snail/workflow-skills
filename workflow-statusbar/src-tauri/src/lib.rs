use chrono::{Datelike, Local, Utc};
use dirs::home_dir;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, Position, Rect, Size, WebviewWindow,
};
use tauri_plugin_notification::NotificationExt;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const MAX_GROUP_ITEMS: usize = 5;
const PROJECT_ROTATION_SECONDS: i64 = 8;
const AUTO_RESUME_COOLDOWN_SECONDS: i64 = 90;
const OTHER_HOST_SUMMARY_FRESH_WINDOW_SECONDS: i64 = 2 * 60 * 60;
const POLL_INTERVAL_SECONDS: u64 = 8;
const TRAY_HIDE_DELAY_MS: u64 = 260;
const MAIN_WINDOW_SHOW_GRACE_MS: u64 = 900;
const ALERT_HTTP_TIMEOUT_CONNECT_MS: u64 = 2_000;
const ALERT_HTTP_TIMEOUT_READ_MS: u64 = 4_000;
const ALERT_HTTP_TIMEOUT_WRITE_MS: u64 = 4_000;
const SQLITE_BUSY_TIMEOUT_MS: u64 = 800;
const KB_HEALTHCHECK_INTERVAL_SECONDS: u64 = 20;
const KB_HEALTHCHECK_CONSECUTIVE_FAILURES: u32 = 3;
const KB_HEALTHCHECK_ALERT_COOLDOWN_SECONDS: i64 = 5 * 60;
const KB_HEALTHCHECK_TIMEOUT_MS: u64 = 1_500;
const KB_COLLECT_MAX_FILE_BYTES: u64 = 512 * 1024;
const KB_COLLECT_MAX_CONTENT_CHARS: usize = 60_000;
const KB_AUTO_COLLECT_INTERVAL_SECONDS: i64 = 30;
const KB_AUTO_COLLECT_MAX_THREADS: usize = 40;
const KB_AUTO_CONVERSATION_TAIL_BYTES: u64 = 256 * 1024;
const TRAY_MENU_OPEN_DASHBOARD: &str = "open_dashboard";
const TRAY_MENU_OPEN_ALERT_SETTINGS: &str = "open_alert_settings";
const TRAY_MENU_OPEN_KNOWLEDGEBASE: &str = "open_knowledgebase";
const TRAY_MENU_QUIT: &str = "quit";
const KNOWLEDGEBASE_DEFAULT_WEB_URL: &str = "http://127.0.0.1:8788";
const KNOWLEDGEBASE_DEFAULT_BIND_ADDR: &str = "127.0.0.1:8788";
const KNOWLEDGEBASE_WEB_HTML: &str = include_str!("../resources/knowledgebase/index.html");
const WORKFLOW_PACK_SCHEMA_VERSION: &str = "1.0.0";

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
    knowledgebase_push: KnowledgebasePushStatus,
    projects: Vec<ProjectSnapshot>,
    groups: Vec<ProjectGroup>,
    summary: Summary,
    spotlight_project: Option<ProjectSnapshot>,
    updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KnowledgebasePushStatus {
    enabled: bool,
    endpoint: String,
    connected: bool,
    last_push_at: String,
    failure_count: u64,
    last_error: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbStats {
    projects: i64,
    items: i64,
    events: i64,
    links: i64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbSearchItem {
    item_id: String,
    project_id: String,
    item_type: String,
    title: String,
    source_path: String,
    snippet: String,
    updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbSearchResponse {
    query: String,
    items: Vec<KbSearchItem>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbItemDetail {
    item_id: String,
    project_id: String,
    item_type: String,
    title: String,
    source_path: String,
    content_text: String,
    updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbItemDetailResponse {
    item: Option<KbItemDetail>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbCollectProjectResult {
    project: String,
    events: i64,
    processed_files: i64,
    documents: i64,
    scanned_files: i64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbProjectStatus {
    project_id: String,
    name: String,
    root_path: String,
    item_count: i64,
    event_count: i64,
    document_count: i64,
    conversation_count: i64,
    memory_count: i64,
    workflow_count: i64,
    inbox_count: i64,
    last_item_at: String,
    path_exists: bool,
    has_memory_dir: bool,
    has_workflow_docs: bool,
    has_inbox_dir: bool,
    has_conversation_dir: bool,
    sync_status: String,
    sync_reason: String,
    next_action: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbTraceItem {
    item_id: String,
    item_type: String,
    title: String,
    source_path: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbTraceLink {
    from_id: String,
    to_id: String,
    relation_type: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbTraceResponse {
    item: Option<KbTraceItem>,
    links: Vec<KbTraceLink>,
    related_items: Vec<KbTraceItem>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbPromptTemplateSource {
    template_id: String,
    item_id: String,
    source_kind: String,
    source_title: String,
    source_path: String,
    source_project: String,
    source_tool: String,
    evidence_excerpt: String,
    confidence: f64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbPromptTemplateSummary {
    id: String,
    name: String,
    category: String,
    target_tools: String,
    task_goal: String,
    status: String,
    quality_score: i64,
    review_note: String,
    usage_boundary: String,
    candidate_note: String,
    source_count: i64,
    updated_at: String,
    source_project: String,
    source_tool: String,
    source_updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbPromptTemplateDetail {
    id: String,
    name: String,
    category: String,
    target_tools: String,
    role_prompt: String,
    task_goal: String,
    variables_json: String,
    context_requirements: String,
    output_format: String,
    quality_bar: String,
    donts: String,
    example_input: String,
    example_output: String,
    status: String,
    quality_score: i64,
    review_note: String,
    usage_boundary: String,
    candidate_note: String,
    created_at: String,
    updated_at: String,
    sources: Vec<KbPromptTemplateSource>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbPromptTemplateListResponse {
    templates: Vec<KbPromptTemplateSummary>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbPromptReviewStats {
    required_total: i64,
    required_dev_handoff: i64,
    total: i64,
    candidate: i64,
    reviewed: i64,
    verified: i64,
    deprecated: i64,
    approved: i64,
    approved_dev_handoff: i64,
    remaining_total: i64,
    remaining_dev_handoff: i64,
    source_count: i64,
    templates_with_sources: i64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbPromptReviewResponse {
    stats: KbPromptReviewStats,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbKnowledgeUnit {
    id: String,
    unit_type: String,
    title: String,
    summary: String,
    category: String,
    source_item_id: String,
    template_id: String,
    weight: f64,
    status: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbKnowledgeUnitLink {
    id: String,
    from_id: String,
    to_id: String,
    relation_type: String,
    summary: String,
    evidence_ref: String,
    template_id: String,
    weight: f64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbKnowledgeUnitsResponse {
    units: Vec<KbKnowledgeUnit>,
    links: Vec<KbKnowledgeUnitLink>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
struct KbTaskStarterRequest {
    input_text: Option<String>,
    session_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbTaskStarterEvidenceItem {
    evidence_type: String,
    source_table: String,
    source_id: String,
    title: String,
    excerpt: String,
    score: f64,
    reason: String,
    source_path: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
struct KbTaskStarterSections {
    similar_tasks: Vec<KbTaskStarterEvidenceItem>,
    risks: Vec<KbTaskStarterEvidenceItem>,
    templates: Vec<KbTaskStarterEvidenceItem>,
    suggested_files: Vec<KbTaskStarterEvidenceItem>,
    verify_commands: Vec<KbTaskStarterEvidenceItem>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbTaskStarterPreviewResponse {
    session_id: String,
    input_type: String,
    parsed_req_id: String,
    parsed_task_id: String,
    summary: String,
    sections: KbTaskStarterSections,
    evidence: Vec<KbTaskStarterEvidenceItem>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbTaskStarterPackageResponse {
    session_id: String,
    markdown: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbTaskStarterSessionSummary {
    session_id: String,
    input_text: String,
    input_type: String,
    parsed_req_id: String,
    parsed_task_id: String,
    summary: String,
    has_package: bool,
    evidence_count: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbTaskStarterSessionsResponse {
    sessions: Vec<KbTaskStarterSessionSummary>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbTaskStarterSessionDetailResponse {
    session: KbTaskStarterSessionSummary,
    evidence: Vec<KbTaskStarterEvidenceItem>,
    markdown: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
struct KbRetroRequest {
    session_id: Option<String>,
    input_text: Option<String>,
    starter_session_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
struct KbRetroSections {
    changes: Vec<KbTaskStarterEvidenceItem>,
    verification: Vec<KbTaskStarterEvidenceItem>,
    risks: Vec<KbTaskStarterEvidenceItem>,
    context: Vec<KbTaskStarterEvidenceItem>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbRetroSuggestionItem {
    suggestion_id: String,
    suggestion_type: String,
    target_kind: String,
    target_id: String,
    title: String,
    rationale: String,
    payload_json: String,
    status: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbRetroStarterEvaluation {
    linked: bool,
    starter_session_id: String,
    score: i64,
    summary: String,
    missing_info: Vec<String>,
    optimization_items: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbRetroPreviewResponse {
    session_id: String,
    input_type: String,
    parsed_req_id: String,
    parsed_task_id: String,
    related_starter_session_id: String,
    summary: String,
    draft_markdown: String,
    sections: KbRetroSections,
    suggestions: Vec<KbRetroSuggestionItem>,
    starter_evaluation: KbRetroStarterEvaluation,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbRetroPackageResponse {
    session_id: String,
    markdown: String,
    suggestions: Vec<KbRetroSuggestionItem>,
    starter_evaluation: KbRetroStarterEvaluation,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct KbRetroSuggestionsResponse {
    suggestions: Vec<KbRetroSuggestionItem>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbHealthAsset {
    asset_type: String,
    asset_id: String,
    title: String,
    category: String,
    status: String,
    score: i64,
    level: String,
    source_count: i64,
    reasons: Vec<String>,
    suggested_action: String,
    updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbHealthProject {
    project_id: String,
    name: String,
    root_path: String,
    score: i64,
    level: String,
    item_count: i64,
    document_count: i64,
    conversation_count: i64,
    memory_count: i64,
    workflow_count: i64,
    reasons: Vec<String>,
    suggested_action: String,
    last_item_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbHealthAction {
    target_type: String,
    target_id: String,
    title: String,
    score: i64,
    priority: String,
    reason: String,
    suggested_action: String,
    primary_route: String,
    evidence_item_id: String,
    search_query: String,
    graph_query: String,
    starter_input: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbHealthSummary {
    total_assets: i64,
    healthy_assets: i64,
    attention_assets: i64,
    noise_candidates: i64,
    total_projects: i64,
    healthy_projects: i64,
    attention_projects: i64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbHealthAssetsResponse {
    summary: KbHealthSummary,
    assets: Vec<KbHealthAsset>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbHealthProjectsResponse {
    summary: KbHealthSummary,
    projects: Vec<KbHealthProject>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbHealthActionsResponse {
    summary: KbHealthSummary,
    actions: Vec<KbHealthAction>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbProjectHealthSnapshot {
    project_id: String,
    name: String,
    root_path: String,
    health_score: i64,
    collection_coverage: i64,
    template_count: i64,
    verified_template_count: i64,
    evidence_completeness: i64,
    risk_count: i64,
    action_count: i64,
    item_count: i64,
    document_count: i64,
    conversation_count: i64,
    memory_count: i64,
    workflow_count: i64,
    test_record_count: i64,
    retrospective_count: i64,
    last_item_at: String,
    path_exists: bool,
    risks: Vec<String>,
    suggested_actions: Vec<String>,
    generated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbProjectHealthSnapshotsResponse {
    snapshots: Vec<KbProjectHealthSnapshot>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbProjectsOverviewSummary {
    total_projects: i64,
    healthy_projects: i64,
    attention_projects: i64,
    total_risks: i64,
    total_actions: i64,
    average_score: i64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbProjectOverviewItem {
    project_id: String,
    name: String,
    root_path: String,
    health_score: i64,
    collection_coverage: i64,
    template_count: i64,
    verified_template_count: i64,
    evidence_completeness: i64,
    risk_count: i64,
    action_count: i64,
    last_item_at: String,
    primary_risk: String,
    next_action: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbProjectsOverviewResponse {
    summary: KbProjectsOverviewSummary,
    projects: Vec<KbProjectOverviewItem>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbProjectHealthDetailResponse {
    project: KbProjectHealthSnapshot,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbProjectActionItem {
    project_id: String,
    action_type: String,
    title: String,
    priority: String,
    reason: String,
    suggested_action: String,
    route_hint: String,
    starter_input: String,
    status: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbProjectActionsResponse {
    project_id: String,
    name: String,
    actions: Vec<KbProjectActionItem>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbWorkflowPackTypeSchema {
    pack_type: String,
    title: String,
    description: String,
    required_sections: Vec<String>,
    required_fields: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbWorkflowPackSchemaResponse {
    schema_version: String,
    checksum_algorithm: String,
    envelope_required_fields: Vec<String>,
    item_required_fields: Vec<String>,
    supported_pack_types: Vec<KbWorkflowPackTypeSchema>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[allow(dead_code)]
struct KbWorkflowPackExportRequest {
    pack_type: Option<String>,
    input_text: Option<String>,
    req_id: Option<String>,
    task_id: Option<String>,
    project_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbWorkflowPackExportResponse {
    pack_id: String,
    pack_type: String,
    schema_version: String,
    title: String,
    source: serde_json::Value,
    item_count: usize,
    checksum: String,
    markdown: String,
    package_json: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[allow(dead_code)]
struct KbWorkflowPackValidateRequest {
    pack_id: Option<String>,
    package_json: Option<serde_json::Value>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[allow(dead_code)]
struct KbWorkflowPackImportRequest {
    package_json: Option<serde_json::Value>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbWorkflowPackValidationIssue {
    severity: String,
    code: String,
    path: String,
    message: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbWorkflowPackValidateResponse {
    valid: bool,
    importable: bool,
    pack_id: String,
    pack_type: String,
    schema_version: String,
    checksum: String,
    calculated_checksum: String,
    item_count: usize,
    issues: Vec<KbWorkflowPackValidationIssue>,
    package_json: serde_json::Value,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbWorkflowPackImportResponse {
    imported: bool,
    pack_id: String,
    status: String,
    validation: KbWorkflowPackValidateResponse,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[allow(dead_code)]
struct KbWorkflowPackDetailResponse {
    pack_id: String,
    pack_type: String,
    schema_version: String,
    title: String,
    source_ref: String,
    checksum: String,
    status: String,
    markdown: String,
    package_json: serde_json::Value,
    items: Vec<serde_json::Value>,
    updated_at: String,
    created_at: String,
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

#[derive(Default)]
struct KnowledgebasePushStateRaw {
    last_push_ts: i64,
    failure_count: u64,
    last_error: String,
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

#[derive(Clone, Serialize)]
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
    session_file_path: String,
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

    fn skipped(
        session_id: &str,
        project_path: &str,
        file_path: Option<&Path>,
        reason: &str,
    ) -> Self {
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

fn knowledgebase_push_state() -> &'static Mutex<KnowledgebasePushStateRaw> {
    static STATE: OnceLock<Mutex<KnowledgebasePushStateRaw>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(KnowledgebasePushStateRaw::default()))
}

fn knowledgebase_web_server_state() -> &'static OnceLock<Result<(), String>> {
    static STATE: OnceLock<Result<(), String>> = OnceLock::new();
    &STATE
}

fn main_window_last_shown_at() -> &'static Mutex<Option<Instant>> {
    static STATE: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
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
        spotlight_after_apply_host: spotlight_after_apply.and_then(|item| item.active_host.clone()),
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
    let file_len = file
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or_default();
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
    let file_len = file
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or_default();
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
        .query_row("select max(ts) from logs", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
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

        let entry = cache.threads.entry(runtime.thread.id.clone()).or_default();
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

            let total =
                extract_json_number_after(&line, "\"total_token_usage\"", "\"total_tokens\"");
            if total <= 0 {
                continue;
            }

            let current = TokenUsage {
                input: extract_json_number_after(
                    &line,
                    "\"total_token_usage\"",
                    "\"input_tokens\"",
                ),
                output: extract_json_number_after(
                    &line,
                    "\"total_token_usage\"",
                    "\"output_tokens\"",
                ),
                reasoning: extract_json_number_after(
                    &line,
                    "\"total_token_usage\"",
                    "\"reasoning_output_tokens\"",
                ),
            };

            if first_today.is_none() {
                first_today = Some(current.clone());
            }
            latest_today = Some(current);
        }

        if let Some(first) = first_today {
            let baseline_empty = entry.baseline.input == 0
                && entry.baseline.output == 0
                && entry.baseline.reasoning == 0;
            let first_total = first.input + first.output + first.reasoning;
            let baseline_total =
                entry.baseline.input + entry.baseline.output + entry.baseline.reasoning;
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
        let timestamp_ms = payload
            .get("timestamp")
            .and_then(|value| value.as_i64())
            .unwrap_or_default();
        let timestamp = if timestamp_ms > 0 {
            timestamp_ms / 1000
        } else {
            0
        };
        if timestamp <= 0 {
            continue;
        }

        let replace = project_by_session
            .get(session_id)
            .map(|(_, existing_ts)| timestamp >= *existing_ts)
            .unwrap_or(true);
        if replace {
            project_by_session.insert(
                session_id.to_string(),
                (project_path.to_string(), timestamp),
            );
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
                let Some(content) = message.get("content").and_then(|value| value.as_array())
                else {
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
                session_file_path: file_path.to_string_lossy().to_string(),
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
    let asn = front_stdout.trim().strip_prefix("ASN:")?.trim();
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
    let asn = front_stdout.trim().strip_prefix("ASN:")?.trim();
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
            if lower_title.contains(&project.name.to_lowercase())
                && seen.insert(project.path.clone())
            {
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
    if ignored_titles
        .iter()
        .any(|item| item.eq_ignore_ascii_case(trimmed))
    {
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

    if frontmost_project_paths.is_empty()
        && IDE_PROCESS_NAMES
            .iter()
            .any(|name| frontmost_app_name.as_deref() == Some(*name))
    {
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
            if IDE_PROCESS_NAMES.contains(&project_name.as_str())
                || !seen.insert(project_name.clone())
            {
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
        is_active_by_codex: !active_project_path.is_empty()
            && path_matches(path, active_project_path),
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
            .map(|runtime| {
                read_last_thread_message(&runtime.primary_thread.thread.rollout_path).role
            })
            .unwrap_or_default(),
        last_message_text: project_runtime
            .map(|runtime| {
                read_last_thread_message(&runtime.primary_thread.thread.rollout_path).text
            })
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
        .map(|runtime| {
            vec![build_codex_project_host_session(
                runtime,
                &project_path,
                auto_resume_enabled,
            )]
        })
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
        is_active_by_codex: !active_project_path.is_empty()
            && path_matches(&project_path, active_project_path),
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
            .map(|runtime| {
                read_last_thread_message(&runtime.primary_thread.thread.rollout_path).role
            })
            .unwrap_or_default(),
        last_message_text: project_runtime
            .map(|runtime| {
                read_last_thread_message(&runtime.primary_thread.thread.rollout_path).text
            })
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
                    .filter(|item| {
                        matches!(item.workflow_stage, WorkflowStage::Execution) && !item.is_blocked
                    })
                    .take(MAX_GROUP_ITEMS)
                    .cloned()
                    .collect(),
                "idle" => projects
                    .iter()
                    .filter(|item| {
                        matches!(item.workflow_stage, WorkflowStage::Idle) && !item.is_blocked
                    })
                    .take(MAX_GROUP_ITEMS)
                    .cloned()
                    .collect(),
                "requirement" => projects
                    .iter()
                    .filter(|item| {
                        matches!(item.workflow_stage, WorkflowStage::Requirement)
                            && !item.is_blocked
                    })
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
        confidence: if process_running {
            "high".into()
        } else {
            "medium".into()
        },
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
    let status = if home_dir()
        .map(|path| path.join(".claude").exists())
        .unwrap_or(false)
    {
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
        project_path: thread
            .map(|item| item.project_path.clone())
            .unwrap_or_default(),
        last_message_role: thread
            .map(|item| item.last_message_role.clone())
            .unwrap_or_default(),
        last_message_text: thread
            .map(|item| item.last_message_text.clone())
            .unwrap_or_default(),
        process_running,
        source: "history.jsonl + projects/*.jsonl".into(),
        confidence: if process_running {
            "medium".into()
        } else {
            "low".into()
        },
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
        process_running: !matches!(
            runtime.primary_thread.status,
            CodexStatus::Idle | CodexStatus::Offline
        ),
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
            } else if !matches!(left.host, HostKind::Codex) && matches!(right.host, HostKind::Codex)
            {
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

    hosts
        .iter()
        .max_by(|left, right| host_rank_order(left, right))
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

fn other_host_summary_for(
    hosts: &[HostSession],
    active_host: Option<&HostKind>,
    now: i64,
) -> String {
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
        state
            .hosts
            .push(build_codex_global_host_session(&state.codex, now));
    }

    let active_session = select_active_host_session(&state.hosts, None);
    state.active_host = active_session.map(|session| session.host.clone());
    state.other_host_summary =
        other_host_summary_for(&state.hosts, state.active_host.as_ref(), now);

    for project in &mut state.projects {
        let project_active_session =
            select_active_host_session(&project.hosts, Some(&project.path));
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
        project.hosts.push(build_claude_global_host_session(
            process_running,
            Some(thread),
            now,
        ));
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

fn find_best_project_index(
    projects: &[ProjectSnapshot],
    thread_project_path: &str,
) -> Option<usize> {
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

fn app_alert_settings_path<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|err| err.to_string())?;
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
            if app_id.is_empty()
                || app_secret.is_empty()
                || (open_id.is_empty() && chat_id.is_empty())
            {
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

fn is_notification_enabled(
    settings: &AlertSettings,
    event_type: &str,
    dispatch_remote: bool,
) -> bool {
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

fn post_bridge_alert(
    endpoint: &str,
    token: &str,
    payload: &RemoteAlertPayload,
) -> Result<(), String> {
    let mut request = alert_http_agent()
        .post(endpoint)
        .set("Content-Type", "application/json");
    if !token.trim().is_empty() {
        request = request.set("Authorization", &format!("Bearer {}", token.trim()));
    }
    request
        .send_json(serde_json::json!({
            "provider": "feishu",
            "payload": payload,
        }))
        .map(|_| ())
        .map_err(|err: ureq::Error| err.to_string())
}

fn request_feishu_tenant_access_token(app_id: &str, app_secret: &str) -> Result<String, String> {
    let response = alert_http_agent()
        .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "app_id": app_id,
            "app_secret": app_secret,
        }))
        .map_err(|err: ureq::Error| err.to_string())?;

    let body: FeishuTenantAccessTokenResponse = response
        .into_json()
        .map_err(|err: std::io::Error| err.to_string())?;
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

    let response = alert_http_agent()
        .post(&format!(
            "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type={receive_id_type}"
        ))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": content.to_string(),
        }))
        .map_err(|err: ureq::Error| err.to_string())?;

    let body: FeishuApiResponse = response
        .into_json()
        .map_err(|err: std::io::Error| err.to_string())?;
    if body.code != 0 {
        return Err(format!("feishu send error: {} ({})", body.msg, body.code));
    }
    Ok(())
}

fn alert_http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(ALERT_HTTP_TIMEOUT_CONNECT_MS))
        .timeout_read(Duration::from_millis(ALERT_HTTP_TIMEOUT_READ_MS))
        .timeout_write(Duration::from_millis(ALERT_HTTP_TIMEOUT_WRITE_MS))
        .build()
}

fn post_remote_alert(
    config: &AlertDispatchConfig,
    payload: &RemoteAlertPayload,
) -> Result<(), String> {
    match config {
        AlertDispatchConfig::Bridge { endpoint, token } => {
            post_bridge_alert(endpoint, token, payload)
        }
        AlertDispatchConfig::Feishu {
            app_id,
            app_secret,
            open_id,
            chat_id,
        } => post_feishu_alert(app_id, app_secret, open_id, chat_id, payload),
    }
}

fn knowledgebase_auto_push_enabled() -> bool {
    env::var("WORKFLOW_STATUSBAR_KB_PUSH")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(true)
}

fn open_url(url: &str) -> Result<(), String> {
    Command::new("open")
        .arg(url)
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

fn knowledgebase_root_dir() -> PathBuf {
    if let Some(home) = home_dir() {
        home.join("Library/Application Support/workflow-statusbar/knowledgebase")
    } else {
        PathBuf::from("./.workflow-statusbar/knowledgebase")
    }
}

fn knowledgebase_db_path() -> PathBuf {
    knowledgebase_root_dir().join("knowledge.db")
}

fn fnv1a64_hex(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in input.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn kb_item_id_for(
    project_id: &str,
    item_type: &str,
    content_hash: &str,
    source_path: &str,
    meta: &KbItemMeta,
) -> String {
    if item_type == "conversation" && !meta.session_id.trim().is_empty() {
        return format!(
            "item-{}",
            fnv1a64_hex(&format!(
                "conversation:{}:{}:{}",
                project_id,
                source_path,
                meta.session_id.trim()
            ))
        );
    }
    format!("item-{content_hash}")
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn connect_knowledgebase() -> Result<Connection, String> {
    let root = knowledgebase_root_dir();
    fs::create_dir_all(&root).map_err(|err| format!("创建知识库目录失败: {err}"))?;
    let conn = Connection::open(knowledgebase_db_path())
        .map_err(|err| format!("打开知识库数据库失败: {err}"))?;
    conn.busy_timeout(Duration::from_millis(SQLITE_BUSY_TIMEOUT_MS))
        .map_err(|err| format!("配置知识库数据库busy_timeout失败: {err}"))?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS projects (
          project_id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          root_path TEXT NOT NULL,
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS items (
          item_id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL,
          item_type TEXT NOT NULL,
          title TEXT NOT NULL,
          content_text TEXT NOT NULL,
          source_path TEXT NOT NULL,
          content_hash TEXT NOT NULL,
          source_type TEXT NOT NULL DEFAULT 'runtime_event',
          source_tool TEXT NOT NULL DEFAULT 'unknown',
          session_id TEXT NOT NULL DEFAULT '',
          speaker TEXT NOT NULL DEFAULT '',
          verified INTEGER NOT NULL DEFAULT 0,
          tags TEXT NOT NULL DEFAULT '',
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS links (
          link_id TEXT PRIMARY KEY,
          from_id TEXT NOT NULL,
          to_id TEXT NOT NULL,
          relation_type TEXT NOT NULL,
          created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS idx_items_project_type ON items(project_id, item_type);
        CREATE INDEX IF NOT EXISTS idx_items_hash ON items(content_hash);
        CREATE INDEX IF NOT EXISTS idx_links_from_to ON links(from_id, to_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_links_unique ON links(from_id, to_id, relation_type);
        CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
          item_id UNINDEXED,
          title,
          content_text,
          source_path,
          tokenize = 'unicode61'
        );
        "#,
    )
    .map_err(|err| format!("初始化知识库表失败: {err}"))?;
    ensure_knowledgebase_schema_migration(&conn)?;
    Ok(conn)
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| err.to_string())?;
    for row in rows {
        if row.map_err(|err| err.to_string())? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    if table_has_column(conn, table, column)? {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn ensure_knowledgebase_schema_migration(conn: &Connection) -> Result<(), String> {
    ensure_column(
        conn,
        "items",
        "source_type",
        "TEXT NOT NULL DEFAULT 'runtime_event'",
    )?;
    ensure_column(
        conn,
        "items",
        "source_tool",
        "TEXT NOT NULL DEFAULT 'unknown'",
    )?;
    ensure_column(conn, "items", "session_id", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(conn, "items", "speaker", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(conn, "items", "verified", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(conn, "items", "tags", "TEXT NOT NULL DEFAULT ''")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_items_project_source ON items(project_id, source_type, source_tool)",
        [],
    )
    .map_err(|err| err.to_string())?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS prompt_templates (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          category TEXT NOT NULL,
          target_tools TEXT NOT NULL DEFAULT '通用',
          role_prompt TEXT NOT NULL DEFAULT '',
          task_goal TEXT NOT NULL DEFAULT '',
          variables_json TEXT NOT NULL DEFAULT '',
          context_requirements TEXT NOT NULL DEFAULT '',
          output_format TEXT NOT NULL DEFAULT '',
          quality_bar TEXT NOT NULL DEFAULT '',
          donts TEXT NOT NULL DEFAULT '',
          example_input TEXT NOT NULL DEFAULT '',
          example_output TEXT NOT NULL DEFAULT '',
          status TEXT NOT NULL DEFAULT 'candidate',
          quality_score INTEGER NOT NULL DEFAULT 60,
          review_note TEXT NOT NULL DEFAULT '',
          usage_boundary TEXT NOT NULL DEFAULT '',
          candidate_note TEXT NOT NULL DEFAULT '',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS prompt_template_sources (
          template_id TEXT NOT NULL,
          item_id TEXT NOT NULL DEFAULT '',
          source_kind TEXT NOT NULL DEFAULT '',
          evidence_excerpt TEXT NOT NULL DEFAULT '',
          confidence REAL NOT NULL DEFAULT 0,
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          PRIMARY KEY(template_id, item_id, evidence_excerpt)
        );
        CREATE TABLE IF NOT EXISTS knowledge_units (
          id TEXT PRIMARY KEY,
          unit_type TEXT NOT NULL,
          title TEXT NOT NULL,
          summary TEXT NOT NULL DEFAULT '',
          category TEXT NOT NULL DEFAULT '',
          source_item_id TEXT NOT NULL DEFAULT '',
          template_id TEXT NOT NULL DEFAULT '',
          weight REAL NOT NULL DEFAULT 1,
          status TEXT NOT NULL DEFAULT 'active',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS task_starter_sessions (
          id TEXT PRIMARY KEY,
          input_text TEXT NOT NULL,
          input_type TEXT NOT NULL DEFAULT 'text',
          parsed_req_id TEXT NOT NULL DEFAULT '',
          parsed_task_id TEXT NOT NULL DEFAULT '',
          summary TEXT NOT NULL DEFAULT '',
          package_markdown TEXT NOT NULL DEFAULT '',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS task_starter_evidence (
          session_id TEXT NOT NULL,
          evidence_type TEXT NOT NULL,
          source_table TEXT NOT NULL,
          source_id TEXT NOT NULL DEFAULT '',
          title TEXT NOT NULL DEFAULT '',
          excerpt TEXT NOT NULL DEFAULT '',
          score REAL NOT NULL DEFAULT 0,
          reason TEXT NOT NULL DEFAULT '',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          PRIMARY KEY(session_id, evidence_type, source_table, source_id)
        );
        CREATE TABLE IF NOT EXISTS retrospective_sessions (
          id TEXT PRIMARY KEY,
          input_text TEXT NOT NULL,
          input_type TEXT NOT NULL DEFAULT 'text',
          parsed_req_id TEXT NOT NULL DEFAULT '',
          parsed_task_id TEXT NOT NULL DEFAULT '',
          related_starter_session_id TEXT NOT NULL DEFAULT '',
          source_summary TEXT NOT NULL DEFAULT '',
          draft_markdown TEXT NOT NULL DEFAULT '',
          status TEXT NOT NULL DEFAULT 'draft',
          confirmed_at TEXT NOT NULL DEFAULT '',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS retrospective_suggestions (
          id TEXT PRIMARY KEY,
          session_id TEXT NOT NULL,
          suggestion_type TEXT NOT NULL,
          target_kind TEXT NOT NULL DEFAULT '',
          target_id TEXT NOT NULL DEFAULT '',
          title TEXT NOT NULL DEFAULT '',
          rationale TEXT NOT NULL DEFAULT '',
          payload_json TEXT NOT NULL DEFAULT '',
          status TEXT NOT NULL DEFAULT 'pending',
          approved_at TEXT NOT NULL DEFAULT '',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS api_clients (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          client_type TEXT NOT NULL DEFAULT 'api',
          last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS api_call_logs (
          id TEXT PRIMARY KEY,
          client_id TEXT NOT NULL DEFAULT '',
          client_name TEXT NOT NULL DEFAULT '',
          tool_name TEXT NOT NULL DEFAULT '',
          method TEXT NOT NULL DEFAULT '',
          path TEXT NOT NULL DEFAULT '',
          params_summary TEXT NOT NULL DEFAULT '',
          duration_ms INTEGER NOT NULL DEFAULT 0,
          status_code INTEGER NOT NULL DEFAULT 0,
          error_message TEXT NOT NULL DEFAULT '',
          remote_addr TEXT NOT NULL DEFAULT '',
          user_agent TEXT NOT NULL DEFAULT '',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS project_health_snapshots (
          project_id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          root_path TEXT NOT NULL,
          health_score INTEGER NOT NULL DEFAULT 0,
          collection_coverage INTEGER NOT NULL DEFAULT 0,
          template_count INTEGER NOT NULL DEFAULT 0,
          verified_template_count INTEGER NOT NULL DEFAULT 0,
          evidence_completeness INTEGER NOT NULL DEFAULT 0,
          risk_count INTEGER NOT NULL DEFAULT 0,
          action_count INTEGER NOT NULL DEFAULT 0,
          item_count INTEGER NOT NULL DEFAULT 0,
          document_count INTEGER NOT NULL DEFAULT 0,
          conversation_count INTEGER NOT NULL DEFAULT 0,
          memory_count INTEGER NOT NULL DEFAULT 0,
          workflow_count INTEGER NOT NULL DEFAULT 0,
          test_record_count INTEGER NOT NULL DEFAULT 0,
          retrospective_count INTEGER NOT NULL DEFAULT 0,
          last_item_at TEXT NOT NULL DEFAULT '',
          path_exists INTEGER NOT NULL DEFAULT 0,
          risks_json TEXT NOT NULL DEFAULT '[]',
          actions_json TEXT NOT NULL DEFAULT '[]',
          generated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS project_action_items (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL,
          action_type TEXT NOT NULL,
          title TEXT NOT NULL,
          priority TEXT NOT NULL DEFAULT 'P1',
          reason TEXT NOT NULL DEFAULT '',
          suggested_action TEXT NOT NULL DEFAULT '',
          route_hint TEXT NOT NULL DEFAULT 'health',
          starter_input TEXT NOT NULL DEFAULT '',
          status TEXT NOT NULL DEFAULT 'open',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS workflow_packs (
          id TEXT PRIMARY KEY,
          pack_type TEXT NOT NULL,
          schema_version TEXT NOT NULL,
          title TEXT NOT NULL,
          source_ref TEXT NOT NULL DEFAULT '',
          package_json TEXT NOT NULL DEFAULT '{}',
          package_markdown TEXT NOT NULL DEFAULT '',
          checksum TEXT NOT NULL DEFAULT '',
          status TEXT NOT NULL DEFAULT 'draft',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS workflow_pack_items (
          id TEXT PRIMARY KEY,
          pack_id TEXT NOT NULL,
          item_type TEXT NOT NULL,
          source_table TEXT NOT NULL DEFAULT '',
          source_id TEXT NOT NULL DEFAULT '',
          title TEXT NOT NULL DEFAULT '',
          path TEXT NOT NULL DEFAULT '',
          content_hash TEXT NOT NULL DEFAULT '',
          required INTEGER NOT NULL DEFAULT 0,
          payload_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS idx_prompt_templates_status ON prompt_templates(status, category);
        CREATE INDEX IF NOT EXISTS idx_prompt_sources_template ON prompt_template_sources(template_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_units_status ON knowledge_units(status, unit_type, category);
        CREATE INDEX IF NOT EXISTS idx_task_starter_sessions_created ON task_starter_sessions(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_task_starter_sessions_req_task ON task_starter_sessions(parsed_req_id, parsed_task_id);
        CREATE INDEX IF NOT EXISTS idx_task_starter_evidence_session ON task_starter_evidence(session_id);
        CREATE INDEX IF NOT EXISTS idx_retro_sessions_created ON retrospective_sessions(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_retro_sessions_req_task ON retrospective_sessions(parsed_req_id, parsed_task_id);
        CREATE INDEX IF NOT EXISTS idx_retro_sessions_starter ON retrospective_sessions(related_starter_session_id);
        CREATE INDEX IF NOT EXISTS idx_retro_suggestions_session ON retrospective_suggestions(session_id, status);
        CREATE INDEX IF NOT EXISTS idx_retro_suggestions_type ON retrospective_suggestions(suggestion_type, status);
        CREATE INDEX IF NOT EXISTS idx_api_call_logs_created ON api_call_logs(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_api_call_logs_client_tool ON api_call_logs(client_id, tool_name, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_project_health_snapshots_score ON project_health_snapshots(health_score, risk_count);
        CREATE INDEX IF NOT EXISTS idx_project_action_items_project ON project_action_items(project_id, status, priority);
        CREATE INDEX IF NOT EXISTS idx_workflow_packs_type ON workflow_packs(pack_type, status, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_workflow_pack_items_pack ON workflow_pack_items(pack_id, item_type);
        "#,
    )
    .map_err(|err| err.to_string())?;
    ensure_column(
        conn,
        "prompt_templates",
        "quality_score",
        "INTEGER NOT NULL DEFAULT 60",
    )?;
    ensure_column(
        conn,
        "prompt_templates",
        "review_note",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "prompt_templates",
        "usage_boundary",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "prompt_templates",
        "candidate_note",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    seed_v3_knowledge_assets(conn)?;
    Ok(())
}

fn compact_text_chars(value: &str, limit: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = normalized.chars().take(limit).collect::<String>();
    if normalized.chars().count() > limit {
        out.push('…');
    }
    out
}

fn infer_prompt_category(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    if text.contains("UI")
        || text.contains("界面")
        || text.contains("原型")
        || text.contains("视觉")
    {
        "UI 设计"
    } else if text.contains("测试") || text.contains("验收") || text.contains("验证") {
        "测试验收"
    } else if text.contains("workflow")
        || text.contains("需求池")
        || text.contains("任务看板")
        || text.contains("治理")
    {
        "工作流治理"
    } else if text.contains("修复")
        || text.contains("根因")
        || text.contains("问题定位")
        || lower.contains("bug")
    {
        "问题修复"
    } else if text.contains("审查") || text.contains("review") || text.contains("风险") {
        "代码审查"
    } else {
        "开发实现"
    }
}

fn prompt_template_copy_text(template: &KbPromptTemplateDetail) -> String {
    let mut out = format!(
        r#"# {}

适用场景：{}
适合模型/工具：{}
质量评分：{}
审核备注：{}
适用边界：{}

角色设定：
{}

任务目标：
{}

输入变量：
{}

上下文要求：
{}

输出格式：
{}

质量标准：
{}

禁忌/不要做：
{}

示例输入：
{}

示例输出：
{}
"#,
        template.name,
        template.category,
        template.target_tools,
        template.quality_score,
        if template.review_note.trim().is_empty() {
            "暂无"
        } else {
            template.review_note.as_str()
        },
        if template.usage_boundary.trim().is_empty() {
            "按当前模板场景使用，超出场景需重新确认上下文。"
        } else {
            template.usage_boundary.as_str()
        },
        template.role_prompt,
        template.task_goal,
        template.variables_json,
        template.context_requirements,
        template.output_format,
        template.quality_bar,
        template.donts,
        template.example_input,
        template.example_output,
    );
    if !template.sources.is_empty() {
        out.push_str("\n来源证据：\n");
        for source in &template.sources {
            out.push_str(&format!(
                "- {}｜{}｜{}\n",
                source.source_kind, source.source_title, source.evidence_excerpt
            ));
        }
    }
    out
}

fn seed_prompt_template(
    conn: &Connection,
    id: &str,
    name: &str,
    category: &str,
    target_tools: &str,
    role_prompt: &str,
    task_goal: &str,
    variables_json: &str,
    context_requirements: &str,
    output_format: &str,
    quality_bar: &str,
    donts: &str,
    example_input: &str,
    example_output: &str,
    status: &str,
) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO prompt_templates(
          id, name, category, target_tools, role_prompt, task_goal, variables_json,
          context_requirements, output_format, quality_bar, donts, example_input,
          example_output, status
        ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
        params![
            id,
            name,
            category,
            target_tools,
            role_prompt,
            task_goal,
            variables_json,
            context_requirements,
            output_format,
            quality_bar,
            donts,
            example_input,
            example_output,
            status
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn seed_prompt_template_quality(
    conn: &Connection,
    id: &str,
    status: &str,
    quality_score: i64,
    review_note: &str,
    variables_json: &str,
    example_input: &str,
    output_format: &str,
    usage_boundary: &str,
) -> Result<(), String> {
    conn.execute(
        r#"
        UPDATE prompt_templates
        SET status=?2,
            quality_score=?3,
            review_note=?4,
            variables_json=?5,
            example_input=?6,
            output_format=?7,
            usage_boundary=?8,
            updated_at=CURRENT_TIMESTAMP
        WHERE id=?1
        "#,
        params![
            id,
            status,
            quality_score.clamp(0, 100),
            review_note,
            variables_json,
            example_input,
            output_format,
            usage_boundary
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn seed_knowledge_unit(
    conn: &Connection,
    id: &str,
    unit_type: &str,
    title: &str,
    summary: &str,
    category: &str,
    template_id: &str,
    weight: f64,
) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO knowledge_units(
          id, unit_type, title, summary, category, template_id, weight, status
        ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active')
        "#,
        params![id, unit_type, title, summary, category, template_id, weight],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn seed_v3_knowledge_assets(conn: &Connection) -> Result<(), String> {
    seed_prompt_template(
        conn,
        "tpl-dev-handoff",
        "需求交接给开发 AI",
        "开发实现",
        "Codex / 通用",
        "你是熟悉本仓库协作约定的高级开发执行 AI，必须先检索历史、分析影响范围，再实施改动。",
        "把已审核需求转成可执行实现，并完成验证、证据和记忆沉淀。",
        "{{repo_path}}、{{req_id}}、{{task_id}}、{{acceptance}}",
        "先读取 AGENTS.md、PROJECT_CONTEXT、需求池、任务看板和命中的 .ai/memory。",
        "按“分析结论 / 改动摘要 / 验证结果 / 未覆盖风险 / 记忆沉淀”输出。",
        "至少完成最小构建验证；不越界修改；有复用价值的结论写入 .ai/memory。",
        "不要跳过人工审核门；不要修改无关文件；不要用主观描述替代验证结果。",
        "请在 {{repo_path}} 按 {{task_id}} 开始实现，先检索历史并锁定边界。",
        "输出涉及文件、SQL/API 链路、调用链、根因结论、改动与验证。",
        "verified",
    )?;
    seed_prompt_template(
        conn,
        "tpl-ui-prototype-review",
        "知识库页面原型重整",
        "UI 设计",
        "Codex / Claude",
        "你是产品型前端设计师，熟悉深色科技工作台、信息密度控制和可复用 AI 工作流。",
        "把资料浏览器重整为知识图谱中枢、提示词工程、搜索工作台、采集中心。",
        "{{prototype_path}}、{{core_tabs}}、{{visual_direction}}",
        "读取 V3 PRD、产品设计、技术设计、旧原型和当前运行页布局。",
        "输出可交互页面或实现方案，明确默认页、三栏区、候选审核和复制动作。",
        "默认页突出语义图谱；提示词工程是核心；搜索/采集保持已有好用布局。",
        "不要做营销页；不要引入重型 3D；不要让搜索和采集抢默认首页。",
        "你先做个 V3 原型看看，搜索中心和采集中心布局参考现有页面。",
        "给出页面结构、交互闭环、视觉方向和可验证文件。",
        "reviewed",
    )?;
    seed_prompt_template(
        conn,
        "tpl-test-acceptance",
        "实现后验收话术",
        "测试验收",
        "通用",
        "你是负责验收闭环的测试与发布 AI，关注命令结果、接口结果、截图证据和未覆盖风险。",
        "按任务验收标准执行验证，并将结果回写到测试记录和任务记忆。",
        "{{build_command}}、{{api_smoke}}、{{ui_flow}}、{{risk_note}}",
        "读取任务看板验收标准、技术设计验证建议和 .ai/memory/tasks/**/verify.md。",
        "输出验证命令、实际结果、证据位置、失败定位、未覆盖风险。",
        "失败要明确失败点；未执行要说明原因；验证结论必须可追溯。",
        "不要只说“看起来正常”；不要省略失败日志；不要提前标 done。",
        "请按 {{task_id}} 验收本轮实现。",
        "输出验证表格、证据链接、剩余风险和状态建议。",
        "candidate",
    )?;
    seed_prompt_template(
        conn,
        "tpl-frontend-fix-handoff",
        "前端问题交给开发 AI 修复",
        "开发实现",
        "Codex / 通用",
        "你是负责前端问题修复的高级开发 AI，必须先复现现象、定位相关组件/样式/接口，再做最小范围改动。",
        "把页面截图、异常现象和验收口径转成可执行修复任务，并完成构建与运行态验证。",
        "{{repo_path}}、{{page_or_route}}、{{screenshot_note}}、{{expected_behavior}}、{{acceptance}}",
        "先读取项目协作约定、相关页面文件、样式规则和最近任务记忆；说明涉及文件、接口链路、调用链和根因判断。",
        "按“现象复述 / 根因定位 / 改动清单 / 验证命令 / 未覆盖风险 / 提交信息”输出。",
        "修复后至少完成前端构建；涉及交互时补运行态检查；不得扩大到无关页面或重做设计体系。",
        "不要只按截图猜测；不要改全局样式造成其他页签漂移；不要在未验证时标记完成。",
        "请在 {{repo_path}} 修复 {{page_or_route}} 的前端问题：{{screenshot_note}}。验收标准：{{acceptance}}。",
        "输出根因、改动文件、验证结果和剩余风险。",
        "reviewed",
    )?;
    seed_prompt_template(
        conn,
        "tpl-review-fix-handoff",
        "代码审查问题交给开发 AI 修复",
        "开发实现",
        "Codex / 通用",
        "你是负责处理代码审查意见的开发 AI，必须逐条确认问题是否成立，优先修复真实缺陷和回归风险。",
        "把 review 发现转成有边界的修复任务，完成代码改动、验证、记忆沉淀和提交说明。",
        "{{repo_path}}、{{review_findings}}、{{risk_level}}、{{test_scope}}、{{acceptance}}",
        "先读取 review 指向的文件、相关调用链、测试/验证材料和任务记忆；区分必须修复、可解释不改和后续跟进。",
        "按“问题判定 / 修复策略 / 文件改动 / 验证覆盖 / 不修复说明 / 提交建议”输出。",
        "每个修复都能对应一个审查发现；验证覆盖高风险路径；不成立的问题要给出代码证据。",
        "不要为了消除评论而重构无关代码；不要跳过测试；不要隐藏仍未解决的风险。",
        "请处理以下代码审查问题：{{review_findings}}。仓库：{{repo_path}}。验收标准：{{acceptance}}。",
        "输出逐条处理结果、验证命令和未覆盖风险。",
        "reviewed",
    )?;

    seed_knowledge_unit(
        conn,
        "ku-prompt-engineering",
        "theme",
        "提示词工程",
        "把历史对话、文档和 AI 回复整理为可审核、可复制、可复用的个人提示词工程资产。",
        "提示词工程",
        "",
        1.0,
    )?;
    seed_knowledge_unit(
        conn,
        "ku-dev-handoff",
        "template",
        "开发交接模板",
        "把需求、边界、调用链、验证和沉淀要求一次性交给开发 AI。",
        "开发实现",
        "tpl-dev-handoff",
        0.92,
    )?;
    seed_knowledge_unit(
        conn,
        "ku-ui-iteration",
        "theme",
        "UI 体验迭代",
        "保留现有搜索/采集布局，新增提示词工程与语义图谱工作区。",
        "UI 设计",
        "tpl-ui-prototype-review",
        0.86,
    )?;
    seed_knowledge_unit(
        conn,
        "ku-test-acceptance",
        "template",
        "测试验收模板",
        "沉淀构建、API、UI、迁移和风险验证话术。",
        "测试验收",
        "tpl-test-acceptance",
        0.82,
    )?;
    seed_knowledge_unit(
        conn,
        "ku-frontend-fix-handoff",
        "template",
        "前端修复交接",
        "把截图现象、页面路径、预期行为和验收标准整理为可执行前端修复任务。",
        "开发实现",
        "tpl-frontend-fix-handoff",
        0.84,
    )?;
    seed_knowledge_unit(
        conn,
        "ku-review-fix-handoff",
        "template",
        "审查修复交接",
        "把代码审查发现逐条转成有证据、有验证的修复任务。",
        "开发实现",
        "tpl-review-fix-handoff",
        0.83,
    )?;
    seed_knowledge_unit(
        conn,
        "ku-workflow-governance",
        "theme",
        "工作流治理",
        "以需求池、任务看板、memory 和人工审核门管理需求执行。",
        "工作流治理",
        "",
        0.78,
    )?;
    seed_knowledge_unit(
        conn,
        "ku-evidence",
        "evidence",
        "来源证据",
        "模板必须能回到原始会话、文档或任务记忆。",
        "来源证据",
        "",
        0.72,
    )?;

    extract_prompt_candidates(conn)?;
    seed_v31_prompt_quality_assets(conn)?;
    Ok(())
}

fn seed_v31_prompt_quality_assets(conn: &Connection) -> Result<(), String> {
    seed_prompt_template_quality(
        conn,
        "tpl-dev-handoff",
        "verified",
        92,
        "V3.1 精修：适合把已审核需求交给开发 AI，已补齐边界、变量和验证回写要求。",
        "{{repo_path}}、{{req_id}}、{{task_id}}、{{acceptance}}、{{known_risks}}",
        "请在 {{repo_path}} 按 {{req_id}} / {{task_id}} 开始实现，验收标准：{{acceptance}}。已知风险：{{known_risks}}。",
        "Markdown，包含：涉及文件/方法、SQL或接口链路、调用链与影响范围、根因结论、改动摘要、验证命令、未覆盖风险、记忆沉淀。",
        "用于已通过人工审核、可以进入 execution 的开发任务；需求未冻结或范围不清时先回到 requirement。",
    )?;
    seed_prompt_template_quality(
        conn,
        "tpl-frontend-fix-handoff",
        "reviewed",
        90,
        "V3.1 精修：适合截图驱动的前端修复，强调复现、局部样式影响和构建验证。",
        "{{repo_path}}、{{page_or_route}}、{{screenshot_note}}、{{expected_behavior}}、{{acceptance}}、{{viewport}}",
        "请在 {{repo_path}} 修复 {{page_or_route}}：{{screenshot_note}}。预期：{{expected_behavior}}。视口：{{viewport}}。验收：{{acceptance}}。",
        "Markdown，包含：现象复述、根因定位、涉及组件/样式、最小改动、验证命令、截图或运行态证据、剩余风险。",
        "用于已有页面的具体前端缺陷修复；不适合从零设计新页面或整体重做视觉体系。",
    )?;
    seed_prompt_template_quality(
        conn,
        "tpl-review-fix-handoff",
        "reviewed",
        89,
        "V3.1 精修：适合处理代码审查意见，要求逐条判断成立与否并保留验证证据。",
        "{{repo_path}}、{{review_findings}}、{{risk_level}}、{{test_scope}}、{{acceptance}}",
        "请处理以下 review 问题：{{review_findings}}。风险级别：{{risk_level}}。测试范围：{{test_scope}}。验收：{{acceptance}}。",
        "Markdown，包含：问题判定表、修复策略、文件改动、验证覆盖、不修复说明、提交建议。",
        "用于已有审查结论后的修复回合；不适合没有具体 finding 的泛化重构。",
    )?;
    seed_prompt_template_quality(
        conn,
        "tpl-ui-prototype-review",
        "reviewed",
        87,
        "V3.1 精修：适合把知识库类工作台原型升级为可落地的信息架构和交互方案。",
        "{{prototype_path}}、{{core_tabs}}、{{visual_direction}}、{{existing_layout}}、{{acceptance}}",
        "请基于 {{prototype_path}} 重整页面，核心页签：{{core_tabs}}，视觉方向：{{visual_direction}}，需保留：{{existing_layout}}。",
        "Markdown 或可运行页面方案，包含：页面结构、信息密度、交互闭环、状态处理、验收检查点。",
        "用于产品/原型到前端实现前的方案整理；不适合直接替代构建验证或真实 UI 回归。",
    )?;
    seed_prompt_template_quality(
        conn,
        "tpl-test-acceptance",
        "reviewed",
        88,
        "V3.1 精修：适合实现完成后的验收闭环，强调命令、接口、UI 和未覆盖风险可追溯。",
        "{{task_id}}、{{build_command}}、{{api_smoke}}、{{ui_flow}}、{{risk_note}}、{{evidence_docs}}",
        "请按 {{task_id}} 验收本轮实现。构建：{{build_command}}；接口烟测：{{api_smoke}}；UI 流程：{{ui_flow}}；风险：{{risk_note}}。",
        "Markdown 表格，包含：验证项、命令/步骤、实际结果、证据位置、结论、未覆盖风险。",
        "用于代码完成后的验收和回写；不适合需求仍未冻结或没有可运行环境的阶段。",
    )?;
    seed_prompt_template_quality(
        conn,
        "cand-462d8e5c077e720b",
        "reviewed",
        84,
        "V3.1 精修：来源是 Codex 会话中的 V3 规划落地，适合把规划结果转成任务体系同步提示。",
        "{{planning_summary}}、{{repo_path}}、{{req_id}}、{{task_board_path}}、{{memory_path}}",
        "已完成规划：{{planning_summary}}。请在 {{repo_path}} 同步 {{req_id}} 到 {{task_board_path}} 和 {{memory_path}}。",
        "Markdown，包含：规划结论、任务拆解、状态变更、需同步文档、验证与后续动作。",
        "用于规划已经明确后的治理同步；不适合替代产品确认或未冻结 PRD 的需求分析。",
    )?;
    seed_prompt_template_quality(
        conn,
        "cand-4cad283624054680",
        "reviewed",
        83,
        "V3.1 精修：来源是前端自测说明，适合要求 AI 区分 file:// 与真实服务环境的 UI 验证。",
        "{{page_url}}、{{api_mock_scope}}、{{browser_flow}}、{{known_gap}}、{{acceptance}}",
        "请验证 {{page_url}}。接口/mock 范围：{{api_mock_scope}}；浏览器流程：{{browser_flow}}；已知差异：{{known_gap}}；验收：{{acceptance}}。",
        "Markdown，包含：环境说明、验证步骤、通过项、失败项、与用户打开方式的差异、下一步修正。",
        "用于前端页面验收解释和复测；不适合代替真实用户环境下的最终验收。",
    )?;
    seed_prompt_template_quality(
        conn,
        "cand-7466094f5650fda2",
        "reviewed",
        82,
        "V3.1 精修：来源是 MCP 配置排障，适合处理工具注册、配置存在但运行态未加载的问题。",
        "{{config_path}}、{{expected_tool}}、{{runtime_check}}、{{observed_result}}、{{next_action}}",
        "请排查 {{expected_tool}} 未加载。配置文件：{{config_path}}；运行态检查：{{runtime_check}}；现象：{{observed_result}}。",
        "Markdown，包含：配置证据、运行态证据、差异判断、根因候选、修复步骤、复测命令。",
        "用于本地工具/MCP/插件加载排障；不适合没有配置证据的泛化环境问题。",
    )?;
    seed_prompt_template_quality(
        conn,
        "cand-053aa57839a3b392",
        "reviewed",
        81,
        "V3.1 精修：来源是重启后复查 MCP 暴露能力的对话，适合做环境重载后的验收清单。",
        "{{restart_action}}、{{tool_list_command}}、{{expected_resources}}、{{actual_resources}}、{{follow_up}}",
        "已执行 {{restart_action}}。请检查工具列表：{{tool_list_command}}；预期资源：{{expected_resources}}；实际：{{actual_resources}}。",
        "Markdown，包含：重启前提、检查命令、实际资源、差异、是否恢复、后续动作。",
        "用于重启/重载后的工具链验收；不适合业务功能验收或代码质量评审。",
    )?;
    seed_prompt_template_quality(
        conn,
        "cand-9792f918ea22067b",
        "reviewed",
        82,
        "V3.1 精修：来源是性能/接口复测产物整理，适合把测试结果路径和排序规则沉淀为验收证据。",
        "{{result_count}}、{{top_case}}、{{report_paths}}、{{sort_rule}}、{{memory_path}}",
        "本轮合并 {{result_count}} 条记录，Top case：{{top_case}}。报告：{{report_paths}}；排序规则：{{sort_rule}}。",
        "Markdown，包含：数据规模、Top 问题、排序/过滤规则、产物路径、复测建议、记忆沉淀。",
        "用于测试结果整理和证据沉淀；不适合直接生成修复方案，修复需另开开发任务。",
    )?;
    Ok(())
}

fn extract_prompt_candidates(conn: &Connection) -> Result<(), String> {
    let candidate_count = conn
        .query_row(
            "SELECT COUNT(*) FROM prompt_templates WHERE status='candidate'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if candidate_count >= 12 {
        return Ok(());
    }

    let mut stmt = conn
        .prepare(
            r#"
            SELECT item_id, item_type, title, content_text, source_path, source_tool
            FROM items
            WHERE item_type IN ('conversation', 'document', 'event')
              AND (
                title LIKE '%提示词%' OR content_text LIKE '%提示词%'
                OR content_text LIKE '%模板%' OR content_text LIKE '%交接%'
                OR content_text LIKE '%验收%' OR content_text LIKE '%先分析后改%'
                OR content_text LIKE '%不要%' OR content_text LIKE '%输出格式%'
                OR content_text LIKE '%workflow%' OR content_text LIKE '%知识图谱%'
              )
            ORDER BY updated_at DESC
            LIMIT 24
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3).unwrap_or_default(),
                row.get::<_, String>(4).unwrap_or_default(),
                row.get::<_, String>(5).unwrap_or_default(),
            ))
        })
        .map_err(|err| err.to_string())?;

    for row in rows {
        let (item_id, item_type, title, content, _source_path, source_tool) =
            row.map_err(|err| err.to_string())?;
        let joined = format!("{title}\n{content}");
        let category = infer_prompt_category(&joined);
        let excerpt = compact_text_chars(&joined, 180);
        if excerpt.trim().is_empty() {
            continue;
        }
        let id = format!(
            "cand-{}",
            fnv1a64_hex(&format!("{item_id}:{category}:{excerpt}"))
        );
        let display_title = compact_text_chars(&title, 26);
        let name = if display_title.trim().is_empty() {
            format!("候选：{category}话术")
        } else {
            format!("候选：{category} - {display_title}")
        };
        seed_prompt_template(
            conn,
            &id,
            &name,
            category,
            if source_tool.trim().is_empty() {
                "通用"
            } else {
                &source_tool
            },
            "请根据来源片段整理角色设定，并补充职责边界。",
            &format!("把来源中的高价值话术整理成“{category}”场景可复用模板。"),
            "{{输入材料}}、{{任务目标}}、{{验收标准}}",
            "保留来源证据；补齐上下文要求；人工审核后才进入正式库。",
            "结构化提示词，包含角色、目标、变量、上下文、输出格式、质量标准和禁忌。",
            "复制后 AI 能一次理解任务；字段不完整时保持候选状态。",
            "不要自动视为已验证；不要丢失来源证据。",
            &excerpt,
            "待人工整理。",
            "candidate",
        )?;
        conn.execute(
            r#"
            INSERT OR IGNORE INTO prompt_template_sources(
              template_id, item_id, source_kind, evidence_excerpt, confidence
            ) VALUES(?1, ?2, ?3, ?4, ?5)
            "#,
            params![id, item_id, item_type, excerpt, 0.72_f64],
        )
        .map_err(|err| err.to_string())?;
        conn.execute(
            r#"
            INSERT OR IGNORE INTO knowledge_units(
              id, unit_type, title, summary, category, source_item_id, template_id, weight, status
            ) VALUES(?1, 'template', ?2, ?3, ?4, ?5, ?6, 0.48, 'active')
            "#,
            params![
                format!("ku-{id}"),
                name,
                compact_text_chars(&excerpt, 72),
                category,
                item_id,
                id
            ],
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct KbItemMeta {
    source_type: String,
    source_tool: String,
    session_id: String,
    speaker: String,
    verified: i64,
    tags: String,
}

impl Default for KbItemMeta {
    fn default() -> Self {
        Self {
            source_type: "runtime_event".into(),
            source_tool: "unknown".into(),
            session_id: String::new(),
            speaker: String::new(),
            verified: 0,
            tags: String::new(),
        }
    }
}

#[derive(Default)]
struct KbAutoCollectCursor {
    last_run_at: i64,
    codex_file_mtime: HashMap<String, i64>,
    claude_file_mtime: HashMap<String, i64>,
}

fn kb_project_id(project_path: &str) -> String {
    format!("proj-{}", fnv1a64_hex(project_path))
}

fn kb_upsert_project(conn: &Connection, name: &str, root_path: &str) -> Result<String, String> {
    let project_id = kb_project_id(root_path);
    conn.execute(
        r#"
        INSERT INTO projects(project_id, name, root_path)
        VALUES(?1, ?2, ?3)
        ON CONFLICT(project_id) DO UPDATE SET
          name=excluded.name,
          root_path=excluded.root_path,
          updated_at=CURRENT_TIMESTAMP
        "#,
        params![project_id, name, root_path],
    )
    .map_err(|err| err.to_string())?;
    Ok(project_id)
}

fn kb_upsert_item_with_meta(
    conn: &Connection,
    project_id: &str,
    item_type: &str,
    title: &str,
    content_text: &str,
    source_path: &str,
    meta: &KbItemMeta,
) -> Result<String, String> {
    let content_hash = fnv1a64_hex(content_text);
    let item_id = kb_item_id_for(project_id, item_type, &content_hash, source_path, meta);
    conn.execute(
        r#"
        INSERT INTO items(item_id, project_id, item_type, title, content_text, source_path, content_hash, source_type, source_tool, session_id, speaker, verified, tags)
        VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(item_id) DO UPDATE SET
          title=excluded.title,
          content_text=excluded.content_text,
          source_path=excluded.source_path,
          source_type=excluded.source_type,
          source_tool=excluded.source_tool,
          session_id=excluded.session_id,
          speaker=excluded.speaker,
          verified=excluded.verified,
          tags=excluded.tags,
          updated_at=CURRENT_TIMESTAMP
        "#,
        params![
            item_id,
            project_id,
            item_type,
            title,
            content_text,
            source_path,
            content_hash,
            meta.source_type,
            meta.source_tool,
            meta.session_id,
            meta.speaker,
            meta.verified,
            meta.tags
        ],
    )
    .map_err(|err| err.to_string())?;
    conn.execute(
        "DELETE FROM items_fts WHERE item_id=?1",
        params![item_id.clone()],
    )
    .map_err(|err| err.to_string())?;
    conn.execute(
        "INSERT INTO items_fts(item_id, title, content_text, source_path) VALUES(?1, ?2, ?3, ?4)",
        params![item_id.clone(), title, content_text, source_path],
    )
    .map_err(|err| err.to_string())?;
    Ok(item_id)
}

fn kb_create_link(
    conn: &Connection,
    from_id: &str,
    to_id: &str,
    relation_type: &str,
) -> Result<(), String> {
    let link_id = format!("lnk-{}", now_nanos());
    conn.execute(
        "INSERT OR IGNORE INTO links(link_id, from_id, to_id, relation_type) VALUES(?1, ?2, ?3, ?4)",
        params![link_id, from_id, to_id, relation_type],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn kb_find_item_ids_by_token(
    conn: &Connection,
    project_id: &str,
    token: &str,
    limit: i64,
) -> Result<Vec<String>, String> {
    let pattern = format!("%{token}%");
    let mut stmt = conn
        .prepare(
            r#"
            SELECT item_id
            FROM items
            WHERE project_id=?1 AND item_type!='event' AND (title LIKE ?2 OR content_text LIKE ?2)
            LIMIT ?3
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![project_id, pattern, limit], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|err| err.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|err| err.to_string())?);
    }
    Ok(out)
}

fn extract_req_task_tokens(raw: &str) -> (Vec<String>, Vec<String>) {
    let mut req = Vec::new();
    let mut task = Vec::new();
    for token in raw.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        let item = token.trim();
        if item.len() < 12 {
            continue;
        }
        if item.starts_with("REQ-") {
            req.push(item.to_string());
        } else if item.starts_with("TASK-") {
            task.push(item.to_string());
        }
    }
    req.sort();
    req.dedup();
    task.sort();
    task.dedup();
    (req, task)
}

fn json_text(payload: &serde_json::Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

fn kb_process_event_payload(
    conn: &Connection,
    project_id: &str,
    fallback_source_path: &str,
    payload: &serde_json::Value,
) -> Result<String, String> {
    let summary = json_text(payload, "summary");
    let event_type = {
        let value = json_text(payload, "event_type");
        if value.is_empty() {
            "event".to_string()
        } else {
            value
        }
    };
    let req_id = json_text(payload, "req_id");
    let task_id = json_text(payload, "task_id");
    let source_path = {
        let value = json_text(payload, "source_path");
        if value.is_empty() {
            fallback_source_path.to_string()
        } else {
            value
        }
    };
    let mut title_parts = vec![event_type.clone()];
    if !req_id.is_empty() {
        title_parts.push(req_id.clone());
    }
    if !task_id.is_empty() {
        title_parts.push(task_id.clone());
    }
    let title = title_parts.join(" | ");
    let content = serde_json::to_string_pretty(payload).unwrap_or_else(|_| "{}".to_string());
    let full_text = format!(
        "{}\n\n{}",
        if summary.is_empty() {
            "workflow event"
        } else {
            &summary
        },
        content
    );
    let source_tool = {
        let host = json_text(payload, "host").to_ascii_lowercase();
        if host.contains("codex") {
            "codex"
        } else if host.contains("claude") {
            "claude"
        } else {
            "unknown"
        }
    };
    let event_item_id = kb_upsert_item_with_meta(
        conn,
        project_id,
        "event",
        &title,
        &full_text,
        &source_path,
        &KbItemMeta {
            source_type: "runtime_event".into(),
            source_tool: source_tool.into(),
            tags: "event,runtime".into(),
            ..KbItemMeta::default()
        },
    )?;
    let tokens_raw = format!("{content}\n{summary}\n{req_id}\n{task_id}");
    let (tokens_req, tokens_task) = extract_req_task_tokens(&tokens_raw);
    for token in tokens_req {
        for target_id in kb_find_item_ids_by_token(conn, project_id, &token, 20)? {
            let _ = kb_create_link(conn, &event_item_id, &target_id, "references_req");
        }
    }
    for token in tokens_task {
        for target_id in kb_find_item_ids_by_token(conn, project_id, &token, 20)? {
            let _ = kb_create_link(conn, &event_item_id, &target_id, "references_task");
        }
    }
    Ok(event_item_id)
}

fn ingest_inbox_for_project(
    conn: &Connection,
    project_path: &str,
    project_name: &str,
) -> Result<(i64, i64), String> {
    let project_id = kb_upsert_project(conn, project_name, project_path)?;
    let root = PathBuf::from(project_path);
    let inbox_dirs = vec![
        root.join("knowledge-store/inbox"),
        root.join(".ai/runtime/inbox"),
    ];

    let mut events = 0_i64;
    let mut processed_files = 0_i64;

    for inbox_dir in inbox_dirs {
        if !inbox_dir.exists() {
            continue;
        }
        let entries = fs::read_dir(&inbox_dir).map_err(|err| err.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|err| err.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) != Some("jsonl") {
                continue;
            }
            let content = fs::read_to_string(&path).unwrap_or_default();
            let mut moved_lines = Vec::new();
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let payload: serde_json::Value = match serde_json::from_str(trimmed) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let _ =
                    kb_process_event_payload(conn, &project_id, &path.to_string_lossy(), &payload)?;
                events += 1;
                moved_lines.push(trimmed.to_string());
            }
            if !moved_lines.is_empty() {
                processed_files += 1;
                let done_file = path.with_extension("done");
                let _ = fs::write(&done_file, format!("{}\n", moved_lines.join("\n")));
                let _ = fs::remove_file(&path);
            }
        }
    }

    Ok((events, processed_files))
}

fn should_collect_text_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase()),
        Some(ext)
            if matches!(
                ext.as_str(),
                "md" | "mdx" | "markdown" | "txt" | "json" | "jsonl" | "yml" | "yaml" | "log" | "rst"
            )
    )
}

fn kb_collect_walk(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            kb_collect_walk(&path, files)?;
            continue;
        }
        if should_collect_text_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn normalize_collected_content(raw: &str) -> String {
    let mut out = raw.replace('\0', " ");
    if out.chars().count() > KB_COLLECT_MAX_CONTENT_CHARS {
        out = out
            .chars()
            .take(KB_COLLECT_MAX_CONTENT_CHARS)
            .collect::<String>();
    }
    out
}

fn is_human_conversation_role(role: &str) -> bool {
    matches!(
        role.trim().to_ascii_lowercase().as_str(),
        "user" | "assistant"
    )
}

fn collect_text_from_content_value(content: &serde_json::Value) -> String {
    if let Some(text) = content.as_str() {
        return text.trim().to_string();
    }
    if let Some(parts) = content.as_array() {
        return parts
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|value| value.as_str())
                    .or_else(|| item.get("content").and_then(|value| value.as_str()))
                    .or_else(|| item.as_str())
            })
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let Some(parts) = content.get("parts").and_then(|item| item.as_array()) {
        return parts
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|value| value.as_str())
                    .or_else(|| item.as_str())
            })
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let Some(text) = content.get("text").and_then(|item| item.as_str()) {
        return text.trim().to_string();
    }
    String::new()
}

fn conversation_text_noise_score(text: &str) -> i32 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 100;
    }
    let ascii_len = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .count();
    let non_space_len = trimmed
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .count()
        .max(1);
    let punctuation_len = trimmed
        .chars()
        .filter(|ch| "{}[]<>|\\;".contains(*ch))
        .count();
    let mut score = 0;
    if trimmed.len() > 1200 {
        score += 2;
    }
    if trimmed.contains("\"cmd\"")
        || trimmed.contains("\"tool_uses\"")
        || trimmed.contains("\"recipient_name\"")
        || trimmed.contains("\"function_call_output\"")
        || trimmed.contains("\"function_call\"")
        || trimmed.contains("exec_command")
        || trimmed.contains("write_stdin")
        || trimmed.contains("apply_patch")
    {
        score += 5;
    }
    if trimmed.contains("workflow-statusbar/src-tauri/src/lib.rs:")
        || trimmed.contains("Chunk ID:")
        || trimmed.contains("Process exited with code")
        || trimmed.contains("Original token count:")
        || trimmed.contains("timestamp")
        || trimmed.contains("rate_limits")
        || trimmed.contains("token_count")
    {
        score += 4;
    }
    if punctuation_len * 3 > non_space_len {
        score += 2;
    }
    if ascii_len * 100 / non_space_len > 92 && trimmed.len() > 180 {
        score += 2;
    }
    score
}

fn clean_conversation_text(text: &str) -> Option<String> {
    let mut lines = Vec::new();
    let normalized_text = text.replace("\\n", "\n");
    for line in normalized_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("```") {
            continue;
        }
        if trimmed.starts_with("workflow-statusbar/")
            || trimmed.starts_with("src-tauri/")
            || trimmed.starts_with("Chunk ID:")
            || trimmed.starts_with("Original token count:")
            || trimmed.starts_with("Process exited with code")
            || trimmed.starts_with("{\"timestamp\"")
            || trimmed.starts_with("\"cmd\":")
        {
            continue;
        }
        lines.push(trimmed);
        if lines.len() >= 24 {
            break;
        }
    }
    let cleaned = lines.join("\n");
    if cleaned.trim().is_empty() || conversation_text_noise_score(&cleaned) >= 5 {
        None
    } else {
        Some(cleaned)
    }
}

fn normalize_conversation_block(block: &str) -> String {
    block
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(300)
        .collect::<String>()
}

fn format_conversation_line(role: &str, text: &str) -> Option<String> {
    if !is_human_conversation_role(role) {
        return None;
    }
    let cleaned = clean_conversation_text(text)?;
    let label = if role.eq_ignore_ascii_case("user") {
        "用户"
    } else {
        "助手"
    };
    Some(format!("[{label}] {cleaned}"))
}

fn extract_messages_from_json(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;

    fn collect_message_text(msg: &serde_json::Value) -> Option<String> {
        if let Some(text) = msg.get("text").and_then(|item| item.as_str()) {
            let t = text.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
        if let Some(content) = msg.get("content") {
            let merged = collect_text_from_content_value(content);
            if !merged.is_empty() {
                return Some(merged);
            }
        }
        if let Some(message) = msg.get("message") {
            return collect_message_text(message);
        }
        None
    }

    fn detect_role(msg: &serde_json::Value) -> String {
        msg.get("role")
            .and_then(|item| item.as_str())
            .or_else(|| {
                msg.get("author")
                    .and_then(|author| author.get("role"))
                    .and_then(|item| item.as_str())
            })
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    }

    let mut lines = Vec::new();

    if let Some(messages) = value.get("messages").and_then(|item| item.as_array()) {
        for msg in messages {
            if let Some(text) = collect_message_text(msg) {
                let role = detect_role(msg);
                if let Some(line) = format_conversation_line(&role, &text) {
                    lines.push(line);
                }
            }
        }
    } else if let Some(items) = value.as_array() {
        for msg in items {
            if let Some(text) = collect_message_text(msg) {
                let role = detect_role(msg);
                if let Some(line) = format_conversation_line(&role, &text) {
                    lines.push(line);
                }
            }
        }
    } else if let Some(mapping) = value.get("mapping").and_then(|item| item.as_object()) {
        for msg in mapping.values() {
            if let Some(text) = collect_message_text(msg) {
                let role = detect_role(msg);
                if let Some(line) = format_conversation_line(&role, &text) {
                    lines.push(line);
                }
            }
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n\n"))
    }
}

fn extract_messages_from_jsonl(raw: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(payload) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };

        let payload_type = payload
            .get("type")
            .and_then(|item| item.as_str())
            .unwrap_or_default();
        let payload_body = payload.get("payload").unwrap_or(&payload);
        let body_type = payload_body
            .get("type")
            .and_then(|item| item.as_str())
            .unwrap_or(payload_type);
        if payload_type == "response_item" {
            continue;
        }
        if payload_type == "event_msg" && !matches!(body_type, "user_message" | "agent_message") {
            continue;
        }
        if matches!(
            body_type,
            "function_call"
                | "function_call_output"
                | "exec_command"
                | "token_count"
                | "task_started"
        ) {
            continue;
        }

        let mut role = payload_body
            .get("role")
            .and_then(|item| item.as_str())
            .or_else(|| {
                payload_body
                    .get("author")
                    .and_then(|author| author.get("role"))
                    .and_then(|item| item.as_str())
            })
            .or_else(|| payload.get("role").and_then(|item| item.as_str()))
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .unwrap_or("unknown")
            .to_string();

        let mut text = payload_body
            .get("content")
            .map(collect_text_from_content_value)
            .filter(|item| !item.trim().is_empty())
            .or_else(|| {
                payload_body
                    .get("message")
                    .and_then(|item| item.as_str())
                    .map(str::to_string)
            })
            .or_else(|| {
                payload_body
                    .get("text")
                    .and_then(|item| item.as_str())
                    .map(str::to_string)
            })
            .or_else(|| {
                payload
                    .get("text")
                    .and_then(|item| item.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default();

        if role == "unknown" && payload_type == "event_msg" {
            if body_type == "user_message" {
                role = "user".into();
            } else if body_type == "agent_message" {
                role = "assistant".into();
            }
        }
        if text.trim().is_empty() {
            if let Some(message) = payload.get("message") {
                role = message
                    .get("role")
                    .and_then(|item| item.as_str())
                    .unwrap_or(role.as_str())
                    .to_string();
                text = message
                    .get("content")
                    .map(collect_text_from_content_value)
                    .filter(|item| !item.trim().is_empty())
                    .or_else(|| {
                        message
                            .get("text")
                            .and_then(|item| item.as_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
            }
        }

        let Some(line) = format_conversation_line(&role, &text) else {
            continue;
        };
        lines.push(line);
        if lines.len() >= 80 {
            break;
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n\n"))
    }
}

fn looks_like_conversation_file(path: &Path, content: &str) -> bool {
    let lower_path = path.to_string_lossy().to_ascii_lowercase();
    if lower_path.contains("conversation")
        || lower_path.contains("conversations")
        || lower_path.contains("chat")
        || lower_path.contains("thread")
        || lower_path.contains("dialog")
        || lower_path.contains("session")
    {
        return true;
    }
    let lower = content.to_ascii_lowercase();
    lower.contains("\"messages\"")
        || lower.contains("\"role\"")
        || lower.contains("assistant")
        || lower.contains("user")
        || lower.contains("anthropic")
        || lower.contains("chat.openai.com")
}

fn file_modified_unix(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn format_sqlite_time_from_unix(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| {
            dt.with_timezone(&Utc)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

fn source_file_time_text(path: &Path) -> String {
    let ts = file_modified_unix(path);
    if ts > 0 {
        format_sqlite_time_from_unix(ts)
    } else {
        String::new()
    }
}

fn kb_set_item_updated_at(
    conn: &Connection,
    item_id: &str,
    updated_at: &str,
) -> Result<(), String> {
    if updated_at.trim().is_empty() {
        return Ok(());
    }
    conn.execute(
        "UPDATE items SET updated_at=?2 WHERE item_id=?1",
        params![item_id, updated_at.trim()],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn resolve_claude_rollout_path(
    home: &Path,
    thread_id: &str,
    project_path: &str,
) -> Option<PathBuf> {
    if thread_id.trim().is_empty() || project_path.trim().is_empty() {
        return None;
    }
    let projects_root = home.join(".claude/projects");
    let escaped = project_path.trim_start_matches('/').replace('/', "-");
    let candidate = projects_root.join(format!("-{escaped}/{thread_id}.jsonl"));
    if candidate.is_file() {
        return Some(candidate);
    }

    let dirs = fs::read_dir(&projects_root).ok()?;
    for dir in dirs.flatten() {
        let dir_path = dir.path();
        if !dir_path.is_dir() {
            continue;
        }
        let path = dir_path.join(format!("{thread_id}.jsonl"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn kb_auto_collect_conversation_file(
    conn: &Connection,
    project_path: &str,
    file_path: &Path,
    source_tool: &str,
    session_id: &str,
) -> Result<bool, String> {
    if !file_path.is_file() {
        return Ok(false);
    }
    let file_path_str = file_path.to_string_lossy().to_string();
    let Some(raw_content) = read_file_tail(&file_path_str, KB_AUTO_CONVERSATION_TAIL_BYTES) else {
        return Ok(false);
    };

    let extracted = if file_path.extension().and_then(|v| v.to_str()) == Some("jsonl") {
        extract_messages_from_jsonl(&raw_content)
    } else if file_path.extension().and_then(|v| v.to_str()) == Some("json") {
        extract_messages_from_json(&raw_content)
    } else {
        None
    };
    let content = normalize_collected_content(extracted.as_deref().unwrap_or(raw_content.as_str()));
    if content.trim().is_empty() {
        return Ok(false);
    }

    let project_root = PathBuf::from(project_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(project_path));
    let project_path = project_root.to_string_lossy().to_string();
    let project_name = project_root
        .file_name()
        .map(|item| item.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    let project_id = kb_upsert_project(conn, &project_name, &project_path)?;

    let title = format!("{source_tool} 会话 {}", session_id.trim());
    let item_id = kb_upsert_item_with_meta(
        conn,
        &project_id,
        "conversation",
        &title,
        &content,
        &file_path_str,
        &KbItemMeta {
            source_type: "conversation".into(),
            source_tool: source_tool.to_string(),
            session_id: session_id.trim().to_string(),
            speaker: String::new(),
            verified: 0,
            tags: format!("conversation,auto,{source_tool}"),
        },
    )?;
    kb_set_item_updated_at(conn, &item_id, &source_file_time_text(file_path))?;
    Ok(true)
}

fn kb_auto_collect_runtime_conversations(
    home: &Path,
    cursor: &mut KbAutoCollectCursor,
) -> Result<(usize, usize), String> {
    if !knowledgebase_auto_push_enabled() {
        return Ok((0, 0));
    }
    let now = unix_now();
    if cursor.last_run_at > 0
        && now.saturating_sub(cursor.last_run_at) < KB_AUTO_COLLECT_INTERVAL_SECONDS
    {
        return Ok((0, 0));
    }
    cursor.last_run_at = now;

    let conn = connect_knowledgebase()?;
    let mut codex_count = 0_usize;
    let mut claude_count = 0_usize;

    for thread in read_recent_threads(home)
        .into_iter()
        .take(KB_AUTO_COLLECT_MAX_THREADS)
    {
        if thread.cwd.trim().is_empty() || thread.rollout_path.trim().is_empty() {
            continue;
        }
        let path = PathBuf::from(&thread.rollout_path);
        let mtime = file_modified_unix(&path);
        if mtime <= 0 {
            continue;
        }
        let key = path.to_string_lossy().to_string();
        if cursor
            .codex_file_mtime
            .get(&key)
            .copied()
            .unwrap_or_default()
            >= mtime
        {
            continue;
        }
        if kb_auto_collect_conversation_file(&conn, &thread.cwd, &path, "codex", &thread.id)? {
            codex_count += 1;
            cursor.codex_file_mtime.insert(key, mtime);
        }
    }

    let (claude_threads, _, _) = read_recent_claude_threads(home);
    for thread in claude_threads.into_iter().take(KB_AUTO_COLLECT_MAX_THREADS) {
        let path = if !thread.session_file_path.trim().is_empty() {
            PathBuf::from(&thread.session_file_path)
        } else {
            let Some(path) = resolve_claude_rollout_path(home, &thread.id, &thread.project_path)
            else {
                continue;
            };
            path
        };
        if !path.is_file() {
            continue;
        }
        let mtime = file_modified_unix(&path);
        if mtime <= 0 {
            continue;
        }
        let key = path.to_string_lossy().to_string();
        if cursor
            .claude_file_mtime
            .get(&key)
            .copied()
            .unwrap_or_default()
            >= mtime
        {
            continue;
        }
        if kb_auto_collect_conversation_file(
            &conn,
            &thread.project_path,
            &path,
            "claude",
            &thread.id,
        )? {
            claude_count += 1;
            cursor.claude_file_mtime.insert(key, mtime);
        }
    }

    if codex_count + claude_count > 0 {
        if let Ok(mut guard) = knowledgebase_push_state().lock() {
            guard.last_push_ts = now;
            guard.last_error.clear();
        }
    }

    Ok((codex_count, claude_count))
}

fn kb_detect_source_tool(path: &Path, content: &str) -> &'static str {
    let lower_path = path.to_string_lossy().to_ascii_lowercase();
    let lower_content = content.to_ascii_lowercase();
    if lower_path.contains("codex")
        || lower_content.contains("\"codex\"")
        || lower_content.contains("openai codex")
    {
        "codex"
    } else if lower_path.contains("claude")
        || lower_content.contains("\"claude\"")
        || lower_content.contains("anthropic")
    {
        "claude"
    } else if lower_path.contains("chatgpt")
        || lower_content.contains("\"chatgpt\"")
        || lower_content.contains("chat.openai.com")
        || lower_content.contains("openai")
    {
        "chatgpt"
    } else if lower_path.contains("gemini") || lower_content.contains("gemini.google.com") {
        "gemini"
    } else {
        "unknown"
    }
}

fn kb_collect_documents_for_project(
    conn: &Connection,
    project_path: &str,
    project_name: &str,
) -> Result<(i64, i64), String> {
    let project_id = kb_upsert_project(conn, project_name, project_path)?;
    let root = PathBuf::from(project_path);
    let roots = vec![
        (root.join(".ai/memory"), "memory"),
        (root.join("docs/workflow"), "workflow"),
        (root.join("knowledge-store/conversations"), "conversation"),
        (root.join("knowledge-store/chat"), "conversation"),
        (root.join(".ai/runtime/conversations"), "conversation"),
        (root.join(".ai/runtime/chat"), "conversation"),
        (root.join(".ai/memory/conversations"), "conversation"),
        (root.join(".ai/memory/chat"), "conversation"),
        (root.join(".codex/conversations"), "conversation"),
        (root.join(".claude/conversations"), "conversation"),
        (root.join(".chatgpt/conversations"), "conversation"),
        (root.join(".gemini/conversations"), "conversation"),
    ];

    let mut files = Vec::new();
    for (dir, _) in &roots {
        kb_collect_walk(dir, &mut files)?;
    }
    files.sort();
    files.dedup();

    let mut documents = 0_i64;
    let mut scanned_files = 0_i64;

    for file in files {
        let metadata = match fs::metadata(&file) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.len() > KB_COLLECT_MAX_FILE_BYTES {
            continue;
        }
        let raw_content = match fs::read_to_string(&file) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let parsed_conversation = extract_messages_from_json(&raw_content);
        let content = normalize_collected_content(
            parsed_conversation
                .as_deref()
                .unwrap_or(raw_content.as_str()),
        );
        if content.trim().is_empty() {
            continue;
        }
        scanned_files += 1;
        let lower = file.to_string_lossy().to_ascii_lowercase();
        let source_type = if lower.contains("/.ai/memory/") {
            "memory"
        } else if lower.contains("/docs/workflow/") {
            "workflow"
        } else if lower.contains("conversation") || looks_like_conversation_file(&file, &content) {
            "conversation"
        } else {
            "document"
        };
        let item_type = if source_type == "conversation" {
            "conversation"
        } else {
            "document"
        };
        let title = content
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().trim_start_matches('#').trim().to_string())
            .filter(|line| !line.is_empty())
            .unwrap_or_else(|| {
                file.file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_else(|| "document".to_string())
            });
        let tags = format!(
            "{source_type},{}",
            if item_type == "conversation" {
                "chat"
            } else {
                "doc"
            }
        );
        let _ = kb_upsert_item_with_meta(
            conn,
            &project_id,
            item_type,
            &title,
            &content,
            &file.to_string_lossy(),
            &KbItemMeta {
                source_type: source_type.to_string(),
                source_tool: kb_detect_source_tool(&file, &content).to_string(),
                verified: 0,
                tags,
                ..KbItemMeta::default()
            },
        )?;
        documents += 1;
    }

    Ok((documents, scanned_files))
}

fn kb_collect_project_internal(path: &str) -> Result<KbCollectProjectResult, String> {
    let root = PathBuf::from(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    let project_path = root.to_string_lossy().to_string();
    let project_name = root
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    let conn = connect_knowledgebase()?;
    let (events, processed_files) = ingest_inbox_for_project(&conn, &project_path, &project_name)?;
    let (documents, scanned_files) =
        kb_collect_documents_for_project(&conn, &project_path, &project_name)?;
    Ok(KbCollectProjectResult {
        project: project_path,
        events,
        processed_files,
        documents,
        scanned_files,
    })
}

fn any_dir_exists(project_root: &Path, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| project_root.join(candidate).is_dir())
}

fn kb_detect_project_capabilities(project_root: &Path) -> (bool, bool, bool, bool, bool) {
    let path_exists = project_root.exists();
    if !path_exists {
        return (false, false, false, false, false);
    }
    let has_memory_dir = any_dir_exists(project_root, &[".ai/memory"]);
    let has_workflow_docs = any_dir_exists(project_root, &["docs/workflow"]);
    let has_inbox_dir = any_dir_exists(
        project_root,
        &["knowledge-store/inbox", ".ai/runtime/inbox"],
    );
    let has_conversation_dir = any_dir_exists(
        project_root,
        &[
            "knowledge-store/conversations",
            "knowledge-store/chat",
            ".ai/runtime/conversations",
            ".ai/runtime/chat",
            ".ai/memory/conversations",
            ".ai/memory/chat",
            ".codex/conversations",
            ".claude/conversations",
            ".chatgpt/conversations",
            ".gemini/conversations",
        ],
    );
    (
        path_exists,
        has_memory_dir,
        has_workflow_docs,
        has_inbox_dir,
        has_conversation_dir,
    )
}

fn kb_project_sync_diagnosis(
    item_count: i64,
    document_count: i64,
    conversation_count: i64,
    path_exists: bool,
    has_memory_dir: bool,
    has_workflow_docs: bool,
    has_inbox_dir: bool,
    has_conversation_dir: bool,
) -> (String, String, String) {
    let mut missing_sources: Vec<&str> = Vec::new();
    if !has_memory_dir {
        missing_sources.push("记忆");
    }
    if !has_workflow_docs {
        missing_sources.push("文档");
    }
    if !has_inbox_dir {
        missing_sources.push("收件箱");
    }
    if !has_conversation_dir {
        missing_sources.push("会话");
    }
    let missing_text = if missing_sources.is_empty() {
        String::new()
    } else {
        format!("当前未接入：{}", missing_sources.join(" / "))
    };

    if !path_exists {
        return (
            "error".into(),
            "项目路径不存在或当前机器不可访问".into(),
            "检查项目路径是否迁移，或重新注册正确路径".into(),
        );
    }

    if item_count == 0 {
        if !(has_memory_dir || has_workflow_docs || has_inbox_dir || has_conversation_dir) {
            return (
                "empty".into(),
                "未发现可采集目录，当前项目尚未接入记忆/文档/会话来源".into(),
                "优先补 `.ai/memory`、`docs/workflow` 或导入会话目录后再重扫".into(),
            );
        }
        return (
            "warning".into(),
            "已发现可采集目录，但当前仍未形成有效条目".into(),
            "先手动执行一次项目采集，再检查目录内容是否为空或格式不受支持".into(),
        );
    }

    if document_count > 0 && conversation_count == 0 {
        if has_conversation_dir {
            return (
                "partial".into(),
                "文档已入库，但会话目录存在却尚未采到有效对话".into(),
                "检查会话文件格式，补一份真实样本做回放验证".into(),
            );
        }
        return (
            "partial".into(),
            "文档已入库，当前项目未发现可读会话目录".into(),
            format!(
                "追加采集只会刷新已接入来源；如需补齐多源对话，请先接入会话目录。{}",
                missing_text
            )
            .trim()
            .into(),
        );
    }

    if document_count == 0 && (has_memory_dir || has_workflow_docs) {
        return (
            "partial".into(),
            "项目存在文档来源目录，但当前文档条目仍为 0".into(),
            "检查文档目录是否为空、是否超出采集范围，必要时补日志回放".into(),
        );
    }

    let next_action = if missing_sources.is_empty() {
        "继续按需追加采集，并补做多源样本回归与验收记录".into()
    } else {
        format!(
            "当前追加采集只会刷新已接入来源；若要补齐黄色来源，请先接入对应目录。{}",
            missing_text
        )
    };

    (
        "ok".into(),
        "文档与基础知识条目已可采集，当前项目处于可检索状态".into(),
        next_action,
    )
}

fn kb_list_projects_internal() -> Result<Vec<KbProjectStatus>, String> {
    let conn = connect_knowledgebase()?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                p.project_id,
                p.name,
                p.root_path,
                COUNT(i.item_id) AS item_count,
                SUM(CASE WHEN i.item_type='event' THEN 1 ELSE 0 END) AS event_count,
                SUM(CASE WHEN i.item_type='document' THEN 1 ELSE 0 END) AS document_count,
                SUM(CASE WHEN i.item_type='conversation' THEN 1 ELSE 0 END) AS conversation_count,
                SUM(CASE WHEN i.source_type='memory' THEN 1 ELSE 0 END) AS memory_count,
                SUM(CASE WHEN i.source_type='workflow' THEN 1 ELSE 0 END) AS workflow_count,
                SUM(CASE WHEN i.source_path LIKE '%knowledge-store/inbox%' OR i.source_path LIKE '%/.ai/runtime/inbox/%' OR i.source_type='runtime_event' THEN 1 ELSE 0 END) AS inbox_count,
                COALESCE(MAX(i.updated_at), '') AS last_item_at
            FROM projects p
            LEFT JOIN items i ON i.project_id = p.project_id
            GROUP BY p.project_id, p.name, p.root_path
            ORDER BY p.updated_at DESC
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(KbProjectStatus {
                project_id: row.get::<_, String>(0)?,
                name: row.get::<_, String>(1)?,
                root_path: row.get::<_, String>(2)?,
                item_count: row.get::<_, i64>(3)?,
                event_count: row.get::<_, i64>(4).unwrap_or_default(),
                document_count: row.get::<_, i64>(5).unwrap_or_default(),
                conversation_count: row.get::<_, i64>(6).unwrap_or_default(),
                memory_count: row.get::<_, i64>(7).unwrap_or_default(),
                workflow_count: row.get::<_, i64>(8).unwrap_or_default(),
                inbox_count: row.get::<_, i64>(9).unwrap_or_default(),
                last_item_at: row.get::<_, String>(10).unwrap_or_default(),
                path_exists: false,
                has_memory_dir: false,
                has_workflow_docs: false,
                has_inbox_dir: false,
                has_conversation_dir: false,
                sync_status: String::new(),
                sync_reason: String::new(),
                next_action: String::new(),
            })
        })
        .map_err(|err| err.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let mut item = row.map_err(|err| err.to_string())?;
        let root = PathBuf::from(&item.root_path);
        let (path_exists, has_memory_dir, has_workflow_docs, has_inbox_dir, has_conversation_dir) =
            kb_detect_project_capabilities(&root);
        let (sync_status, sync_reason, next_action) = kb_project_sync_diagnosis(
            item.item_count,
            item.document_count,
            item.conversation_count,
            path_exists,
            has_memory_dir,
            has_workflow_docs,
            has_inbox_dir,
            has_conversation_dir,
        );
        item.path_exists = path_exists;
        item.has_memory_dir = has_memory_dir;
        item.has_workflow_docs = has_workflow_docs;
        item.has_inbox_dir = has_inbox_dir;
        item.has_conversation_dir = has_conversation_dir;
        item.sync_status = sync_status;
        item.sync_reason = sync_reason;
        item.next_action = next_action;
        out.push(item);
    }
    Ok(out)
}

fn kb_get_stats_internal() -> Result<KbStats, String> {
    let conn = connect_knowledgebase()?;
    let projects = conn
        .query_row("SELECT COUNT(*) FROM projects", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|err| err.to_string())?;
    let items = conn
        .query_row("SELECT COUNT(*) FROM items", [], |row| row.get::<_, i64>(0))
        .map_err(|err| err.to_string())?;
    let events = conn
        .query_row(
            "SELECT COUNT(*) FROM items WHERE item_type='event'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|err| err.to_string())?;
    let links = conn
        .query_row("SELECT COUNT(*) FROM links", [], |row| row.get::<_, i64>(0))
        .map_err(|err| err.to_string())?;
    Ok(KbStats {
        projects,
        items,
        events,
        links,
    })
}

fn kb_search_internal(query: &str) -> Result<KbSearchResponse, String> {
    let conn = connect_knowledgebase()?;
    let raw_query = query.trim();
    let tokens = raw_query
        .replace('"', " ")
        .split_whitespace()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(|item| format!("\"{item}\""))
        .collect::<Vec<_>>();
    if raw_query.is_empty() {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT item_id, project_id, item_type, title, source_path, substr(content_text, 1, 120) AS snippet, COALESCE(updated_at, '') AS updated_at
                FROM items
                ORDER BY updated_at DESC
                LIMIT 1000
                "#,
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(KbSearchItem {
                    item_id: row.get::<_, String>(0)?,
                    project_id: row.get::<_, String>(1)?,
                    item_type: row.get::<_, String>(2)?,
                    title: row.get::<_, String>(3)?,
                    source_path: row.get::<_, String>(4)?,
                    snippet: row.get::<_, String>(5).unwrap_or_default(),
                    updated_at: row.get::<_, String>(6).unwrap_or_default(),
                })
            })
            .map_err(|err| err.to_string())?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|err| err.to_string())?);
        }
        return Ok(KbSearchResponse {
            query: query.to_string(),
            items,
        });
    }
    let safe_query = if tokens.is_empty() {
        format!("\"{}\"", raw_query.replace('"', " "))
    } else {
        tokens.join(" OR ")
    };
    let mut items = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut stmt = conn
        .prepare(
            r#"
            SELECT i.item_id, i.project_id, i.item_type, i.title, i.source_path,
                   snippet(items_fts, 2, '[', ']', ' … ', 24) AS snippet, COALESCE(i.updated_at, '') AS updated_at
            FROM items_fts
            JOIN items i ON i.item_id = items_fts.item_id
            WHERE items_fts MATCH ?1
            ORDER BY bm25(items_fts), i.updated_at DESC
            LIMIT 30
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![safe_query], |row| {
            Ok(KbSearchItem {
                item_id: row.get::<_, String>(0)?,
                project_id: row.get::<_, String>(1)?,
                item_type: row.get::<_, String>(2)?,
                title: row.get::<_, String>(3)?,
                source_path: row.get::<_, String>(4)?,
                snippet: row.get::<_, String>(5).unwrap_or_default(),
                updated_at: row.get::<_, String>(6).unwrap_or_default(),
            })
        })
        .map_err(|err| err.to_string())?;
    for row in rows {
        let item = row.map_err(|err| err.to_string())?;
        if seen_ids.insert(item.item_id.clone()) {
            items.push(item);
        }
    }

    // Fallback to substring search so Chinese/partial keywords can still hit.
    let like_pattern = format!("%{}%", raw_query);
    let mut fallback_stmt = conn
        .prepare(
            r#"
            SELECT item_id, project_id, item_type, title, source_path, substr(content_text, 1, 120) AS snippet, COALESCE(updated_at, '') AS updated_at
            FROM items
            WHERE title LIKE ?1 OR content_text LIKE ?1 OR source_path LIKE ?1
            ORDER BY updated_at DESC
            LIMIT 30
            "#,
        )
        .map_err(|err| err.to_string())?;
    let fallback_rows = fallback_stmt
        .query_map(params![like_pattern], |row| {
            Ok(KbSearchItem {
                item_id: row.get::<_, String>(0)?,
                project_id: row.get::<_, String>(1)?,
                item_type: row.get::<_, String>(2)?,
                title: row.get::<_, String>(3)?,
                source_path: row.get::<_, String>(4)?,
                snippet: row.get::<_, String>(5).unwrap_or_default(),
                updated_at: row.get::<_, String>(6).unwrap_or_default(),
            })
        })
        .map_err(|err| err.to_string())?;
    for row in fallback_rows {
        let item = row.map_err(|err| err.to_string())?;
        if seen_ids.insert(item.item_id.clone()) {
            items.push(item);
        }
        if items.len() >= 30 {
            break;
        }
    }
    Ok(KbSearchResponse {
        query: query.to_string(),
        items,
    })
}

fn kb_trace_internal(item_id: &str) -> Result<KbTraceResponse, String> {
    let conn = connect_knowledgebase()?;
    let mut item_stmt = conn
        .prepare("SELECT item_id, item_type, title, source_path FROM items WHERE item_id=?1")
        .map_err(|err| err.to_string())?;
    let item = item_stmt
        .query_row(params![item_id], |row| {
            Ok(KbTraceItem {
                item_id: row.get::<_, String>(0)?,
                item_type: row.get::<_, String>(1)?,
                title: row.get::<_, String>(2)?,
                source_path: row.get::<_, String>(3)?,
            })
        })
        .ok();

    if item.is_none() {
        return Ok(KbTraceResponse {
            item: None,
            links: Vec::new(),
            related_items: Vec::new(),
        });
    }

    let mut links = Vec::new();
    let mut link_stmt = conn
        .prepare("SELECT from_id, to_id, relation_type FROM links WHERE from_id=?1 OR to_id=?1")
        .map_err(|err| err.to_string())?;
    let link_rows = link_stmt
        .query_map(params![item_id], |row| {
            Ok(KbTraceLink {
                from_id: row.get::<_, String>(0)?,
                to_id: row.get::<_, String>(1)?,
                relation_type: row.get::<_, String>(2)?,
            })
        })
        .map_err(|err| err.to_string())?;
    let mut related_ids = HashSet::new();
    for row in link_rows {
        let link = row.map_err(|err| err.to_string())?;
        if link.from_id != item_id {
            related_ids.insert(link.from_id.clone());
        }
        if link.to_id != item_id {
            related_ids.insert(link.to_id.clone());
        }
        links.push(link);
    }

    let mut related_items = Vec::new();
    if !related_ids.is_empty() {
        let mut related_stmt = conn
            .prepare("SELECT item_id, item_type, title, source_path FROM items WHERE item_id=?1")
            .map_err(|err| err.to_string())?;
        for related_id in related_ids {
            if let Ok(node) = related_stmt.query_row(params![related_id], |row| {
                Ok(KbTraceItem {
                    item_id: row.get::<_, String>(0)?,
                    item_type: row.get::<_, String>(1)?,
                    title: row.get::<_, String>(2)?,
                    source_path: row.get::<_, String>(3)?,
                })
            }) {
                related_items.push(node);
            }
        }
    }

    Ok(KbTraceResponse {
        item,
        links,
        related_items,
    })
}

fn kb_item_detail_internal(item_id: &str) -> Result<KbItemDetailResponse, String> {
    let conn = connect_knowledgebase()?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT item_id, project_id, item_type, title, source_path,
                   COALESCE(content_text, '') AS content_text,
                   COALESCE(updated_at, '') AS updated_at
            FROM items
            WHERE item_id = ?1
            LIMIT 1
            "#,
        )
        .map_err(|err| err.to_string())?;

    let item = stmt
        .query_row(params![item_id], |row| {
            Ok(KbItemDetail {
                item_id: row.get::<_, String>(0)?,
                project_id: row.get::<_, String>(1)?,
                item_type: row.get::<_, String>(2)?,
                title: row.get::<_, String>(3)?,
                source_path: row.get::<_, String>(4)?,
                content_text: row.get::<_, String>(5).unwrap_or_default(),
                updated_at: row.get::<_, String>(6).unwrap_or_default(),
            })
        })
        .ok();

    Ok(KbItemDetailResponse { item })
}

fn kb_prompt_templates_internal(
    status: Option<&str>,
) -> Result<KbPromptTemplateListResponse, String> {
    let conn = connect_knowledgebase()?;
    let mut templates = Vec::new();
    let status_filter = status.map(str::trim).filter(|item| !item.is_empty());
    let sql = if status_filter.is_some() {
        r#"
        SELECT t.id, t.name, t.category, t.target_tools, t.task_goal, t.status,
               COALESCE(t.quality_score, 60) AS quality_score,
               COALESCE(t.review_note, '') AS review_note,
               COALESCE(t.usage_boundary, '') AS usage_boundary,
               COALESCE(t.candidate_note, '') AS candidate_note,
               COUNT(s.template_id) AS source_count, COALESCE(t.updated_at, '') AS updated_at,
               COALESCE((
                 SELECT p.name
                 FROM prompt_template_sources s2
                 LEFT JOIN items i2 ON i2.item_id = s2.item_id
                 LEFT JOIN projects p ON p.project_id = i2.project_id
                 WHERE s2.template_id = t.id
                 ORDER BY COALESCE(i2.updated_at, '') DESC, s2.created_at DESC
                 LIMIT 1
               ), '') AS source_project,
               COALESCE((
                 SELECT i2.source_tool
                 FROM prompt_template_sources s2
                 LEFT JOIN items i2 ON i2.item_id = s2.item_id
                 WHERE s2.template_id = t.id
                 ORDER BY COALESCE(i2.updated_at, '') DESC, s2.created_at DESC
                 LIMIT 1
               ), '') AS source_tool,
               COALESCE((
                 SELECT i2.updated_at
                 FROM prompt_template_sources s2
                 LEFT JOIN items i2 ON i2.item_id = s2.item_id
                 WHERE s2.template_id = t.id
                 ORDER BY COALESCE(i2.updated_at, '') DESC, s2.created_at DESC
                 LIMIT 1
               ), '') AS source_updated_at
        FROM prompt_templates t
        LEFT JOIN prompt_template_sources s ON s.template_id = t.id
        WHERE t.status = ?1
        GROUP BY t.id
        ORDER BY CASE t.status
          WHEN 'verified' THEN 0
          WHEN 'reviewed' THEN 1
          WHEN 'candidate' THEN 2
          ELSE 3
        END, t.updated_at DESC, t.name
        "#
    } else {
        r#"
        SELECT t.id, t.name, t.category, t.target_tools, t.task_goal, t.status,
               COALESCE(t.quality_score, 60) AS quality_score,
               COALESCE(t.review_note, '') AS review_note,
               COALESCE(t.usage_boundary, '') AS usage_boundary,
               COALESCE(t.candidate_note, '') AS candidate_note,
               COUNT(s.template_id) AS source_count, COALESCE(t.updated_at, '') AS updated_at,
               COALESCE((
                 SELECT p.name
                 FROM prompt_template_sources s2
                 LEFT JOIN items i2 ON i2.item_id = s2.item_id
                 LEFT JOIN projects p ON p.project_id = i2.project_id
                 WHERE s2.template_id = t.id
                 ORDER BY COALESCE(i2.updated_at, '') DESC, s2.created_at DESC
                 LIMIT 1
               ), '') AS source_project,
               COALESCE((
                 SELECT i2.source_tool
                 FROM prompt_template_sources s2
                 LEFT JOIN items i2 ON i2.item_id = s2.item_id
                 WHERE s2.template_id = t.id
                 ORDER BY COALESCE(i2.updated_at, '') DESC, s2.created_at DESC
                 LIMIT 1
               ), '') AS source_tool,
               COALESCE((
                 SELECT i2.updated_at
                 FROM prompt_template_sources s2
                 LEFT JOIN items i2 ON i2.item_id = s2.item_id
                 WHERE s2.template_id = t.id
                 ORDER BY COALESCE(i2.updated_at, '') DESC, s2.created_at DESC
                 LIMIT 1
               ), '') AS source_updated_at
        FROM prompt_templates t
        LEFT JOIN prompt_template_sources s ON s.template_id = t.id
        GROUP BY t.id
        ORDER BY CASE t.status
          WHEN 'verified' THEN 0
          WHEN 'reviewed' THEN 1
          WHEN 'candidate' THEN 2
          ELSE 3
        END, t.updated_at DESC, t.name
        "#
    };
    let mut stmt = conn.prepare(sql).map_err(|err| err.to_string())?;
    let mut rows = if let Some(filter) = status_filter {
        stmt.query(params![filter]).map_err(|err| err.to_string())?
    } else {
        stmt.query([]).map_err(|err| err.to_string())?
    };
    while let Some(row) = rows.next().map_err(|err| err.to_string())? {
        templates.push(KbPromptTemplateSummary {
            id: row.get::<_, String>(0).map_err(|err| err.to_string())?,
            name: row.get::<_, String>(1).map_err(|err| err.to_string())?,
            category: row.get::<_, String>(2).map_err(|err| err.to_string())?,
            target_tools: row.get::<_, String>(3).map_err(|err| err.to_string())?,
            task_goal: row.get::<_, String>(4).map_err(|err| err.to_string())?,
            status: row.get::<_, String>(5).map_err(|err| err.to_string())?,
            quality_score: row.get::<_, i64>(6).unwrap_or(60),
            review_note: row.get::<_, String>(7).unwrap_or_default(),
            usage_boundary: row.get::<_, String>(8).unwrap_or_default(),
            candidate_note: row.get::<_, String>(9).unwrap_or_default(),
            source_count: row.get::<_, i64>(10).unwrap_or(0),
            updated_at: row.get::<_, String>(11).unwrap_or_default(),
            source_project: row.get::<_, String>(12).unwrap_or_default(),
            source_tool: row.get::<_, String>(13).unwrap_or_default(),
            source_updated_at: row.get::<_, String>(14).unwrap_or_default(),
        });
    }
    Ok(KbPromptTemplateListResponse { templates })
}

fn kb_prompt_review_internal() -> Result<KbPromptReviewResponse, String> {
    let conn = connect_knowledgebase()?;
    let (total, candidate, reviewed, verified, deprecated, approved, approved_dev_handoff): (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = conn
        .query_row(
            r#"
            SELECT
              COUNT(*),
              SUM(CASE WHEN status IN ('candidate', 'refining') THEN 1 ELSE 0 END),
              SUM(CASE WHEN status='reviewed' THEN 1 ELSE 0 END),
              SUM(CASE WHEN status='verified' THEN 1 ELSE 0 END),
              SUM(CASE WHEN status='deprecated' THEN 1 ELSE 0 END),
              SUM(CASE WHEN status IN ('reviewed', 'verified') THEN 1 ELSE 0 END),
              SUM(CASE
                WHEN status IN ('reviewed', 'verified')
                 AND (category LIKE '%开发%' OR name LIKE '%交接%' OR task_goal LIKE '%开发%')
                THEN 1 ELSE 0 END)
            FROM prompt_templates
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1).unwrap_or(0),
                    row.get::<_, i64>(2).unwrap_or(0),
                    row.get::<_, i64>(3).unwrap_or(0),
                    row.get::<_, i64>(4).unwrap_or(0),
                    row.get::<_, i64>(5).unwrap_or(0),
                    row.get::<_, i64>(6).unwrap_or(0),
                ))
            },
        )
        .map_err(|err| err.to_string())?;
    let source_count = conn
        .query_row("SELECT COUNT(*) FROM prompt_template_sources", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0);
    let templates_with_sources = conn
        .query_row(
            r#"
            SELECT COUNT(*) FROM (
              SELECT template_id FROM prompt_template_sources GROUP BY template_id
            )
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    let required_total = 10_i64;
    let required_dev_handoff = 3_i64;
    Ok(KbPromptReviewResponse {
        stats: KbPromptReviewStats {
            required_total,
            required_dev_handoff,
            total,
            candidate,
            reviewed,
            verified,
            deprecated,
            approved,
            approved_dev_handoff,
            remaining_total: (required_total - approved).max(0),
            remaining_dev_handoff: (required_dev_handoff - approved_dev_handoff).max(0),
            source_count,
            templates_with_sources,
        },
    })
}

fn kb_prompt_template_detail_internal(id: &str) -> Result<Option<KbPromptTemplateDetail>, String> {
    let conn = connect_knowledgebase()?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, name, category, target_tools, role_prompt, task_goal, variables_json,
                   context_requirements, output_format, quality_bar, donts, example_input,
                   example_output, status, COALESCE(quality_score, 60),
                   COALESCE(review_note, ''), COALESCE(usage_boundary, ''),
                   COALESCE(candidate_note, ''), COALESCE(created_at, ''), COALESCE(updated_at, '')
            FROM prompt_templates
            WHERE id=?1
            LIMIT 1
            "#,
        )
        .map_err(|err| err.to_string())?;
    let mut template = match stmt.query_row(params![id], |row| {
        Ok(KbPromptTemplateDetail {
            id: row.get::<_, String>(0)?,
            name: row.get::<_, String>(1)?,
            category: row.get::<_, String>(2)?,
            target_tools: row.get::<_, String>(3)?,
            role_prompt: row.get::<_, String>(4)?,
            task_goal: row.get::<_, String>(5)?,
            variables_json: row.get::<_, String>(6)?,
            context_requirements: row.get::<_, String>(7)?,
            output_format: row.get::<_, String>(8)?,
            quality_bar: row.get::<_, String>(9)?,
            donts: row.get::<_, String>(10)?,
            example_input: row.get::<_, String>(11)?,
            example_output: row.get::<_, String>(12)?,
            status: row.get::<_, String>(13)?,
            quality_score: row.get::<_, i64>(14).unwrap_or(60),
            review_note: row.get::<_, String>(15).unwrap_or_default(),
            usage_boundary: row.get::<_, String>(16).unwrap_or_default(),
            candidate_note: row.get::<_, String>(17).unwrap_or_default(),
            created_at: row.get::<_, String>(18).unwrap_or_default(),
            updated_at: row.get::<_, String>(19).unwrap_or_default(),
            sources: Vec::new(),
        })
    }) {
        Ok(item) => item,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(err) => return Err(err.to_string()),
    };

    let mut source_stmt = conn
        .prepare(
            r#"
            SELECT s.template_id, s.item_id, s.source_kind,
                   COALESCE(i.title, '') AS source_title,
                   COALESCE(i.source_path, '') AS source_path,
                   COALESCE(p.name, '') AS source_project,
                   COALESCE(i.source_tool, '') AS source_tool,
                   s.evidence_excerpt, s.confidence
            FROM prompt_template_sources s
            LEFT JOIN items i ON i.item_id = s.item_id
            LEFT JOIN projects p ON p.project_id = i.project_id
            WHERE s.template_id = ?1
            ORDER BY s.confidence DESC, s.created_at DESC
            LIMIT 12
            "#,
        )
        .map_err(|err| err.to_string())?;
    let source_rows = source_stmt
        .query_map(params![id], |row| {
            Ok(KbPromptTemplateSource {
                template_id: row.get::<_, String>(0)?,
                item_id: row.get::<_, String>(1)?,
                source_kind: row.get::<_, String>(2)?,
                source_title: row.get::<_, String>(3).unwrap_or_default(),
                source_path: row.get::<_, String>(4).unwrap_or_default(),
                source_project: row.get::<_, String>(5).unwrap_or_default(),
                source_tool: row.get::<_, String>(6).unwrap_or_default(),
                evidence_excerpt: row.get::<_, String>(7).unwrap_or_default(),
                confidence: row.get::<_, f64>(8).unwrap_or(0.0),
            })
        })
        .map_err(|err| err.to_string())?;
    for row in source_rows {
        template.sources.push(row.map_err(|err| err.to_string())?);
    }

    Ok(Some(template))
}

fn kb_prompt_template_copy_internal(id: &str) -> Result<serde_json::Value, String> {
    let template = kb_prompt_template_detail_internal(id)?
        .ok_or_else(|| "prompt_template_not_found".to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "text": prompt_template_copy_text(&template)
    }))
}

fn kb_prompt_template_status_internal(id: &str, status: &str) -> Result<serde_json::Value, String> {
    let allowed = [
        "candidate",
        "refining",
        "later",
        "noise",
        "reviewed",
        "verified",
        "deprecated",
    ];
    if !allowed.contains(&status) {
        return Err("invalid_status".into());
    }
    let conn = connect_knowledgebase()?;
    let changed = conn
        .execute(
            "UPDATE prompt_templates SET status=?2, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
            params![id, status],
        )
        .map_err(|err| err.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "status": status,
        "changed": changed
    }))
}

fn kb_prompt_template_quality_internal(
    id: &str,
    quality_score: i64,
    review_note: &str,
    variables_json: &str,
    example_input: &str,
    output_format: &str,
    usage_boundary: &str,
) -> Result<serde_json::Value, String> {
    let conn = connect_knowledgebase()?;
    let score = quality_score.clamp(0, 100);
    let changed = conn
        .execute(
            r#"
            UPDATE prompt_templates
            SET quality_score=?2,
                review_note=?3,
                variables_json=?4,
                example_input=?5,
                output_format=?6,
                usage_boundary=?7,
                updated_at=CURRENT_TIMESTAMP
            WHERE id=?1
            "#,
            params![
                id,
                score,
                review_note.trim(),
                variables_json.trim(),
                example_input.trim(),
                output_format.trim(),
                usage_boundary.trim()
            ],
        )
        .map_err(|err| err.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "quality_score": score,
        "changed": changed
    }))
}

fn kb_prompt_template_candidate_note_internal(
    id: &str,
    status: &str,
    candidate_note: &str,
) -> Result<serde_json::Value, String> {
    let allowed = ["candidate", "refining", "later", "noise", "deprecated"];
    if !allowed.contains(&status) {
        return Err("invalid_candidate_status".into());
    }
    let conn = connect_knowledgebase()?;
    let changed = conn
        .execute(
            "UPDATE prompt_templates SET status=?2, candidate_note=?3, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
            params![id, status, candidate_note.trim()],
        )
        .map_err(|err| err.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "status": status,
        "candidate_note": candidate_note.trim(),
        "changed": changed
    }))
}

#[allow(dead_code)]
fn kb_health_level(score: i64) -> String {
    if score >= 80 {
        "healthy".into()
    } else if score >= 60 {
        "attention".into()
    } else {
        "risk".into()
    }
}

#[allow(dead_code)]
fn kb_health_suggested_action(asset_type: &str, score: i64, reasons: &[String]) -> String {
    if score >= 80 {
        return "保持现状，后续复用时补充最新验证记录".into();
    }
    if reasons.iter().any(|item| item.contains("来源证据")) {
        return "补充来源证据或重新采集关联文档".into();
    }
    if reasons
        .iter()
        .any(|item| item.contains("示例") || item.contains("输出格式"))
    {
        return "补齐变量、示例输入和输出格式".into();
    }
    if reasons
        .iter()
        .any(|item| item.contains("候选") || item.contains("噪音"))
    {
        return "人工审核候选，明确进入精修、以后再看或标记噪音".into();
    }
    if asset_type == "project" {
        return "补采集 workflow、memory、document 和 conversation 来源".into();
    }
    "补充证据、审核状态和最近验证记录".into()
}

#[allow(dead_code)]
fn kb_health_summary(assets: &[KbHealthAsset], projects: &[KbHealthProject]) -> KbHealthSummary {
    KbHealthSummary {
        total_assets: assets.len() as i64,
        healthy_assets: assets.iter().filter(|item| item.score >= 80).count() as i64,
        attention_assets: assets.iter().filter(|item| item.score < 80).count() as i64,
        noise_candidates: assets
            .iter()
            .filter(|item| {
                item.status == "noise" || item.reasons.iter().any(|reason| reason.contains("噪音"))
            })
            .count() as i64,
        total_projects: projects.len() as i64,
        healthy_projects: projects.iter().filter(|item| item.score >= 80).count() as i64,
        attention_projects: projects.iter().filter(|item| item.score < 80).count() as i64,
    }
}

#[allow(dead_code)]
fn kb_health_template_score(
    status: &str,
    quality_score: i64,
    source_count: i64,
    variables_json: &str,
    example_input: &str,
    output_format: &str,
    usage_boundary: &str,
    review_note: &str,
) -> (i64, Vec<String>) {
    let mut score = quality_score.clamp(0, 100);
    let mut reasons = Vec::new();

    match status {
        "verified" => score += 8,
        "reviewed" => score += 4,
        "candidate" | "refining" => {
            score -= 18;
            reasons.push("仍处于候选或精修状态".into());
        }
        "later" => {
            score -= 24;
            reasons.push("已标记以后再看".into());
        }
        "noise" => {
            score -= 42;
            reasons.push("候选噪音需要清理".into());
        }
        "deprecated" => {
            score -= 50;
            reasons.push("模板已废弃".into());
        }
        _ => {}
    }

    if source_count <= 0 {
        score -= 18;
        reasons.push("缺少来源证据".into());
    }
    if variables_json.trim().is_empty() {
        score -= 8;
        reasons.push("缺少输入变量".into());
    }
    if example_input.trim().is_empty() {
        score -= 8;
        reasons.push("缺少示例输入".into());
    }
    if output_format.trim().is_empty() {
        score -= 8;
        reasons.push("缺少输出格式".into());
    }
    if usage_boundary.trim().is_empty() {
        score -= 6;
        reasons.push("缺少适用边界".into());
    }
    if review_note.trim().is_empty() && matches!(status, "reviewed" | "verified") {
        score -= 5;
        reasons.push("缺少审核备注".into());
    }
    if reasons.is_empty() {
        reasons.push("模板字段、状态和来源证据完整度良好".into());
    }
    (score.clamp(0, 100), reasons)
}

#[allow(dead_code)]
fn kb_health_assets_internal() -> Result<KbHealthAssetsResponse, String> {
    let conn = connect_knowledgebase()?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT t.id, t.name, t.category, t.status, COALESCE(t.quality_score, 60),
                   COALESCE(t.variables_json, ''), COALESCE(t.example_input, ''),
                   COALESCE(t.output_format, ''), COALESCE(t.usage_boundary, ''),
                   COALESCE(t.review_note, ''), COALESCE(t.updated_at, ''),
                   COUNT(s.template_id) AS source_count
            FROM prompt_templates t
            LEFT JOIN prompt_template_sources s ON s.template_id = t.id
            GROUP BY t.id
            ORDER BY t.updated_at DESC
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let status = row.get::<_, String>(3).unwrap_or_default();
            let source_count = row.get::<_, i64>(11).unwrap_or(0);
            let (score, reasons) = kb_health_template_score(
                &status,
                row.get::<_, i64>(4).unwrap_or(60),
                source_count,
                &row.get::<_, String>(5).unwrap_or_default(),
                &row.get::<_, String>(6).unwrap_or_default(),
                &row.get::<_, String>(7).unwrap_or_default(),
                &row.get::<_, String>(8).unwrap_or_default(),
                &row.get::<_, String>(9).unwrap_or_default(),
            );
            Ok(KbHealthAsset {
                asset_type: "prompt_template".into(),
                asset_id: row.get::<_, String>(0)?,
                title: row.get::<_, String>(1)?,
                category: row.get::<_, String>(2).unwrap_or_default(),
                status: status.clone(),
                score,
                level: kb_health_level(score),
                source_count,
                reasons: reasons.clone(),
                suggested_action: kb_health_suggested_action("prompt_template", score, &reasons),
                updated_at: row.get::<_, String>(10).unwrap_or_default(),
            })
        })
        .map_err(|err| err.to_string())?;
    let mut assets = Vec::new();
    for row in rows {
        assets.push(row.map_err(|err| err.to_string())?);
    }
    assets.sort_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    let summary = kb_health_summary(&assets, &[]);
    Ok(KbHealthAssetsResponse { summary, assets })
}

#[allow(dead_code)]
fn kb_health_project_score(project: &KbProjectStatus) -> (i64, Vec<String>) {
    let mut score = 20_i64;
    let mut reasons = Vec::new();
    if project.document_count > 0 {
        score += 18;
    } else {
        reasons.push("缺少文档采集".into());
    }
    if project.workflow_count > 0 {
        score += 18;
    } else {
        reasons.push("缺少 workflow 治理材料".into());
    }
    if project.memory_count > 0 {
        score += 14;
    } else {
        reasons.push("缺少任务记忆".into());
    }
    if project.conversation_count > 0 {
        score += 12;
    } else {
        reasons.push("缺少 AI 会话采集".into());
    }
    if project.item_count >= 20 {
        score += 12;
    } else if project.item_count > 0 {
        score += 6;
        reasons.push("知识条目数量偏少".into());
    } else {
        reasons.push("项目尚无可用知识条目".into());
    }
    if project.path_exists {
        score += 6;
    } else {
        reasons.push("项目路径不可访问".into());
    }
    if reasons.is_empty() {
        reasons.push("项目采集覆盖较完整".into());
    }
    (score.clamp(0, 100), reasons)
}

#[allow(dead_code)]
fn kb_health_projects_internal() -> Result<KbHealthProjectsResponse, String> {
    let statuses = kb_list_projects_internal()?;
    let mut projects = statuses
        .into_iter()
        .map(|project| {
            let (score, reasons) = kb_health_project_score(&project);
            KbHealthProject {
                project_id: project.project_id,
                name: project.name,
                root_path: project.root_path,
                score,
                level: kb_health_level(score),
                item_count: project.item_count,
                document_count: project.document_count,
                conversation_count: project.conversation_count,
                memory_count: project.memory_count,
                workflow_count: project.workflow_count,
                reasons: reasons.clone(),
                suggested_action: kb_health_suggested_action("project", score, &reasons),
                last_item_at: project.last_item_at,
            }
        })
        .collect::<Vec<_>>();
    projects.sort_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then_with(|| right.last_item_at.cmp(&left.last_item_at))
    });
    let summary = kb_health_summary(&[], &projects);
    Ok(KbHealthProjectsResponse { summary, projects })
}

fn kb_project_template_counts(conn: &Connection) -> Result<HashMap<String, (i64, i64)>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT i.project_id,
                   COUNT(DISTINCT t.id) AS template_count,
                   COUNT(DISTINCT CASE WHEN t.status='verified' THEN t.id END) AS verified_count
            FROM prompt_templates t
            INNER JOIN prompt_template_sources s ON s.template_id = t.id
            INNER JOIN items i ON i.item_id = s.item_id
            GROUP BY i.project_id
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (row.get::<_, i64>(1)?, row.get::<_, i64>(2)?),
            ))
        })
        .map_err(|err| err.to_string())?;
    let mut out = HashMap::new();
    for row in rows {
        let (project_id, counts) = row.map_err(|err| err.to_string())?;
        out.insert(project_id, counts);
    }
    Ok(out)
}

fn kb_project_evidence_counts(conn: &Connection) -> Result<HashMap<String, (i64, i64)>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT project_id,
                   SUM(CASE WHEN lower(source_path) LIKE '%testing%' OR lower(title) LIKE '%test%' OR content_text LIKE '%测试%' THEN 1 ELSE 0 END) AS test_count,
                   SUM(CASE WHEN lower(source_path) LIKE '%retro%' OR lower(title) LIKE '%retro%' OR content_text LIKE '%复盘%' THEN 1 ELSE 0 END) AS retro_count
            FROM items
            GROUP BY project_id
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (
                    row.get::<_, i64>(1).unwrap_or_default(),
                    row.get::<_, i64>(2).unwrap_or_default(),
                ),
            ))
        })
        .map_err(|err| err.to_string())?;
    let mut out = HashMap::new();
    for row in rows {
        let (project_id, counts) = row.map_err(|err| err.to_string())?;
        out.insert(project_id, counts);
    }
    Ok(out)
}

fn kb_percent(hit_count: i64, total: i64) -> i64 {
    if total <= 0 {
        0
    } else {
        ((hit_count * 100) / total).clamp(0, 100)
    }
}

fn kb_project_snapshot_actions(snapshot_name: &str, risks: &[String]) -> Vec<String> {
    let mut actions = Vec::new();
    if risks
        .iter()
        .any(|risk| risk.contains("路径") || risk.contains("文档") || risk.contains("会话"))
    {
        actions.push("补采集项目文档、memory 和 AI 会话".to_string());
    }
    if risks.iter().any(|risk| risk.contains("测试")) {
        actions.push("补充测试记录或验收材料".to_string());
    }
    if risks.iter().any(|risk| risk.contains("复盘")) {
        actions.push("使用复盘助手生成项目复盘".to_string());
    }
    if risks.iter().any(|risk| risk.contains("模板")) {
        actions.push("从项目证据中沉淀提示词模板".to_string());
    }
    if risks.len() >= 4 {
        actions.push("清理低价值候选和噪音片段".to_string());
    }
    actions.push(format!("为 {snapshot_name} 生成下一轮开工包"));
    actions.truncate(6);
    actions
}

fn kb_build_project_health_snapshot(
    project: KbProjectStatus,
    template_count: i64,
    verified_template_count: i64,
    test_record_count: i64,
    retrospective_count: i64,
) -> KbProjectHealthSnapshot {
    let (base_score, mut risks) = kb_health_project_score(&project);
    if template_count == 0 {
        risks.push("缺少提示词模板资产".into());
    }
    if verified_template_count == 0 {
        risks.push("缺少已验证模板".into());
    }
    if test_record_count == 0 {
        risks.push("缺少测试记录".into());
    }
    if retrospective_count == 0 {
        risks.push("缺少复盘记录".into());
    }
    let coverage_hits = [
        project.document_count > 0,
        project.workflow_count > 0,
        project.memory_count > 0,
        project.conversation_count > 0,
        test_record_count > 0,
        retrospective_count > 0,
    ]
    .iter()
    .filter(|item| **item)
    .count() as i64;
    let evidence_hits = [
        project.workflow_count > 0,
        project.memory_count > 0,
        test_record_count > 0,
        retrospective_count > 0,
    ]
    .iter()
    .filter(|item| **item)
    .count() as i64;
    let collection_coverage = kb_percent(coverage_hits, 6);
    let evidence_completeness = kb_percent(evidence_hits, 4);
    let template_score = if verified_template_count > 0 {
        100
    } else if template_count > 0 {
        70
    } else {
        0
    };
    let health_score =
        ((base_score + collection_coverage + evidence_completeness + template_score) / 4)
            .clamp(0, 100);
    let suggested_actions = kb_project_snapshot_actions(&project.name, &risks);
    KbProjectHealthSnapshot {
        project_id: project.project_id,
        name: project.name,
        root_path: project.root_path,
        health_score,
        collection_coverage,
        template_count,
        verified_template_count,
        evidence_completeness,
        risk_count: risks.len() as i64,
        action_count: suggested_actions.len() as i64,
        item_count: project.item_count,
        document_count: project.document_count,
        conversation_count: project.conversation_count,
        memory_count: project.memory_count,
        workflow_count: project.workflow_count,
        test_record_count,
        retrospective_count,
        last_item_at: project.last_item_at,
        path_exists: project.path_exists,
        risks,
        suggested_actions,
        generated_at: Utc::now().to_rfc3339(),
    }
}

fn kb_upsert_project_health_snapshot(
    conn: &Connection,
    snapshot: &KbProjectHealthSnapshot,
) -> Result<(), String> {
    let risks_json = serde_json::to_string(&snapshot.risks).unwrap_or_else(|_| "[]".into());
    let actions_json =
        serde_json::to_string(&snapshot.suggested_actions).unwrap_or_else(|_| "[]".into());
    conn.execute(
        r#"
        INSERT INTO project_health_snapshots(
          project_id, name, root_path, health_score, collection_coverage,
          template_count, verified_template_count, evidence_completeness,
          risk_count, action_count, item_count, document_count, conversation_count,
          memory_count, workflow_count, test_record_count, retrospective_count,
          last_item_at, path_exists, risks_json, actions_json, generated_at
        ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
        ON CONFLICT(project_id) DO UPDATE SET
          name=excluded.name,
          root_path=excluded.root_path,
          health_score=excluded.health_score,
          collection_coverage=excluded.collection_coverage,
          template_count=excluded.template_count,
          verified_template_count=excluded.verified_template_count,
          evidence_completeness=excluded.evidence_completeness,
          risk_count=excluded.risk_count,
          action_count=excluded.action_count,
          item_count=excluded.item_count,
          document_count=excluded.document_count,
          conversation_count=excluded.conversation_count,
          memory_count=excluded.memory_count,
          workflow_count=excluded.workflow_count,
          test_record_count=excluded.test_record_count,
          retrospective_count=excluded.retrospective_count,
          last_item_at=excluded.last_item_at,
          path_exists=excluded.path_exists,
          risks_json=excluded.risks_json,
          actions_json=excluded.actions_json,
          generated_at=excluded.generated_at
        "#,
        params![
            snapshot.project_id,
            snapshot.name,
            snapshot.root_path,
            snapshot.health_score,
            snapshot.collection_coverage,
            snapshot.template_count,
            snapshot.verified_template_count,
            snapshot.evidence_completeness,
            snapshot.risk_count,
            snapshot.action_count,
            snapshot.item_count,
            snapshot.document_count,
            snapshot.conversation_count,
            snapshot.memory_count,
            snapshot.workflow_count,
            snapshot.test_record_count,
            snapshot.retrospective_count,
            snapshot.last_item_at,
            if snapshot.path_exists { 1_i64 } else { 0_i64 },
            risks_json,
            actions_json,
            snapshot.generated_at,
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn kb_project_health_snapshots_internal() -> Result<KbProjectHealthSnapshotsResponse, String> {
    let conn = connect_knowledgebase()?;
    let projects = kb_list_projects_internal()?;
    let template_counts = kb_project_template_counts(&conn)?;
    let evidence_counts = kb_project_evidence_counts(&conn)?;
    let mut snapshots = Vec::new();
    for project in projects {
        let (template_count, verified_template_count) = template_counts
            .get(&project.project_id)
            .cloned()
            .unwrap_or((0, 0));
        let (test_record_count, retrospective_count) = evidence_counts
            .get(&project.project_id)
            .cloned()
            .unwrap_or((0, 0));
        let snapshot = kb_build_project_health_snapshot(
            project,
            template_count,
            verified_template_count,
            test_record_count,
            retrospective_count,
        );
        kb_upsert_project_health_snapshot(&conn, &snapshot)?;
        snapshots.push(snapshot);
    }
    snapshots.sort_by(|left, right| {
        left.health_score
            .cmp(&right.health_score)
            .then_with(|| right.risk_count.cmp(&left.risk_count))
            .then_with(|| right.last_item_at.cmp(&left.last_item_at))
    });
    Ok(KbProjectHealthSnapshotsResponse { snapshots })
}

fn kb_projects_overview_summary(
    snapshots: &[KbProjectHealthSnapshot],
) -> KbProjectsOverviewSummary {
    let total_projects = snapshots.len() as i64;
    let total_score: i64 = snapshots.iter().map(|item| item.health_score).sum();
    KbProjectsOverviewSummary {
        total_projects,
        healthy_projects: snapshots
            .iter()
            .filter(|item| item.health_score >= 80)
            .count() as i64,
        attention_projects: snapshots
            .iter()
            .filter(|item| item.health_score < 80)
            .count() as i64,
        total_risks: snapshots.iter().map(|item| item.risk_count).sum(),
        total_actions: snapshots.iter().map(|item| item.action_count).sum(),
        average_score: if total_projects == 0 {
            0
        } else {
            (total_score / total_projects).clamp(0, 100)
        },
    }
}

fn kb_project_overview_item(snapshot: &KbProjectHealthSnapshot) -> KbProjectOverviewItem {
    KbProjectOverviewItem {
        project_id: snapshot.project_id.clone(),
        name: snapshot.name.clone(),
        root_path: snapshot.root_path.clone(),
        health_score: snapshot.health_score,
        collection_coverage: snapshot.collection_coverage,
        template_count: snapshot.template_count,
        verified_template_count: snapshot.verified_template_count,
        evidence_completeness: snapshot.evidence_completeness,
        risk_count: snapshot.risk_count,
        action_count: snapshot.action_count,
        last_item_at: snapshot.last_item_at.clone(),
        primary_risk: snapshot.risks.first().cloned().unwrap_or_default(),
        next_action: snapshot
            .suggested_actions
            .first()
            .cloned()
            .unwrap_or_else(|| "保持当前项目知识维护节奏".into()),
    }
}

fn kb_projects_overview_internal() -> Result<KbProjectsOverviewResponse, String> {
    let mut snapshots = kb_project_health_snapshots_internal()?.snapshots;
    snapshots.sort_by(|left, right| {
        left.health_score
            .cmp(&right.health_score)
            .then_with(|| right.risk_count.cmp(&left.risk_count))
            .then_with(|| right.last_item_at.cmp(&left.last_item_at))
    });
    let summary = kb_projects_overview_summary(&snapshots);
    let projects = snapshots.iter().map(kb_project_overview_item).collect();
    Ok(KbProjectsOverviewResponse { summary, projects })
}

fn kb_project_snapshot_by_id(project_id: &str) -> Result<KbProjectHealthSnapshot, String> {
    let snapshots = kb_project_health_snapshots_internal()?.snapshots;
    snapshots
        .into_iter()
        .find(|item| item.project_id == project_id)
        .ok_or_else(|| "project_not_found".to_string())
}

fn kb_project_health_detail_internal(
    project_id: &str,
) -> Result<KbProjectHealthDetailResponse, String> {
    Ok(KbProjectHealthDetailResponse {
        project: kb_project_snapshot_by_id(project_id)?,
    })
}

fn kb_project_action_type_and_route(action: &str) -> (String, String) {
    if action.contains("采集") || action.contains("会话") || action.contains("文档") {
        ("collect".into(), "collect".into())
    } else if action.contains("测试") || action.contains("验收") {
        ("verify".into(), "search".into())
    } else if action.contains("复盘") {
        ("retro".into(), "retro".into())
    } else if action.contains("噪音") || action.contains("候选") {
        ("cleanup".into(), "prompt".into())
    } else if action.contains("开工包") {
        ("starter".into(), "starter".into())
    } else if action.contains("模板") {
        ("template".into(), "prompt".into())
    } else {
        ("health".into(), "health".into())
    }
}

fn kb_project_action_items_for_snapshot(
    snapshot: &KbProjectHealthSnapshot,
) -> Vec<KbProjectActionItem> {
    snapshot
        .suggested_actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let (action_type, route_hint) = kb_project_action_type_and_route(action);
            let reason = snapshot.risks.get(index).cloned().unwrap_or_else(|| {
                snapshot
                    .risks
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "项目知识健康度需要维护".into())
            });
            KbProjectActionItem {
                project_id: snapshot.project_id.clone(),
                action_type,
                title: action.clone(),
                priority: if snapshot.health_score < 50 || index == 0 {
                    "P0".into()
                } else {
                    "P1".into()
                },
                reason,
                suggested_action: action.clone(),
                route_hint,
                starter_input: format!(
                    "基于项目 {} 生成开工包。风险：{}",
                    snapshot.name,
                    snapshot.risks.first().cloned().unwrap_or_default()
                ),
                status: "open".into(),
            }
        })
        .collect()
}

fn kb_upsert_project_action_items(
    conn: &Connection,
    actions: &[KbProjectActionItem],
) -> Result<(), String> {
    for action in actions {
        let id = format!(
            "project-action-{}-{}",
            action.project_id,
            fnv1a64_hex(&format!("{}:{}", action.action_type, action.title))
        );
        conn.execute(
            r#"
            INSERT INTO project_action_items(
              id, project_id, action_type, title, priority, reason,
              suggested_action, route_hint, starter_input, status, updated_at
            ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
              priority=excluded.priority,
              reason=excluded.reason,
              suggested_action=excluded.suggested_action,
              route_hint=excluded.route_hint,
              starter_input=excluded.starter_input,
              status=excluded.status,
              updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                id,
                action.project_id,
                action.action_type,
                action.title,
                action.priority,
                action.reason,
                action.suggested_action,
                action.route_hint,
                action.starter_input,
                action.status,
            ],
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn kb_project_actions_internal(project_id: &str) -> Result<KbProjectActionsResponse, String> {
    let snapshot = kb_project_snapshot_by_id(project_id)?;
    let conn = connect_knowledgebase()?;
    let actions = kb_project_action_items_for_snapshot(&snapshot);
    kb_upsert_project_action_items(&conn, &actions)?;
    Ok(KbProjectActionsResponse {
        project_id: snapshot.project_id,
        name: snapshot.name,
        actions,
    })
}

fn kb_workflow_pack_type_schema(
    pack_type: &str,
    title: &str,
    description: &str,
    required_sections: &[&str],
    required_fields: &[&str],
) -> KbWorkflowPackTypeSchema {
    KbWorkflowPackTypeSchema {
        pack_type: pack_type.into(),
        title: title.into(),
        description: description.into(),
        required_sections: required_sections
            .iter()
            .map(|item| (*item).into())
            .collect(),
        required_fields: required_fields.iter().map(|item| (*item).into()).collect(),
    }
}

fn kb_workflow_pack_schema_internal() -> KbWorkflowPackSchemaResponse {
    KbWorkflowPackSchemaResponse {
        schema_version: WORKFLOW_PACK_SCHEMA_VERSION.into(),
        checksum_algorithm: "sha256".into(),
        envelope_required_fields: vec![
            "schema_version".into(),
            "pack_type".into(),
            "pack_id".into(),
            "title".into(),
            "created_at".into(),
            "source".into(),
            "items".into(),
            "markdown".into(),
            "checksum".into(),
        ],
        item_required_fields: vec![
            "item_id".into(),
            "item_type".into(),
            "title".into(),
            "source_ref".into(),
            "required".into(),
            "payload".into(),
        ],
        supported_pack_types: vec![
            kb_workflow_pack_type_schema(
                "requirement_context_pack",
                "需求上下文包",
                "围绕 REQ 聚合 PRD、任务拆解、设计材料、关键约束和验收口径。",
                &[
                    "metadata",
                    "requirement",
                    "tasks",
                    "evidence_index",
                    "acceptance",
                ],
                &["req_id", "title", "prd_ref", "task_refs"],
            ),
            kb_workflow_pack_type_schema(
                "development_handoff_pack",
                "开发交接包",
                "面向开发 AI 的任务输入，包含目标、上下文、相关文件、风险和验证命令。",
                &[
                    "metadata",
                    "task",
                    "context",
                    "files",
                    "risks",
                    "verification",
                ],
                &["task_id", "goal", "context_summary", "suggested_files"],
            ),
            kb_workflow_pack_type_schema(
                "verification_evidence_pack",
                "验证证据包",
                "沉淀构建、API、UI、联调和残余风险，供复盘或后续审计使用。",
                &["metadata", "commands", "api_smoke", "ui_checks", "risks"],
                &["target_ref", "commands", "results"],
            ),
            kb_workflow_pack_type_schema(
                "retrospective_pack",
                "复盘沉淀包",
                "保存执行结论、遗漏信息、沉淀建议和与开工包的关联评估。",
                &[
                    "metadata",
                    "summary",
                    "lessons",
                    "suggestions",
                    "starter_evaluation",
                ],
                &["input_ref", "summary", "suggestions"],
            ),
            kb_workflow_pack_type_schema(
                "project_knowledge_pack",
                "项目知识包",
                "面向单项目迁移或交接，聚合项目画像、关键证据、模板、风险和健康快照。",
                &[
                    "metadata",
                    "project",
                    "health",
                    "evidence_index",
                    "templates",
                    "actions",
                ],
                &["project_id", "project_name", "root_path", "health_snapshot"],
            ),
        ],
    }
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn canonical_json_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
        }
        serde_json::Value::Array(items) => {
            let parts = items
                .iter()
                .map(canonical_json_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{parts}]")
        }
        serde_json::Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            let parts = keys
                .into_iter()
                .map(|key| {
                    let key_json =
                        serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string());
                    let value_json = map
                        .get(key)
                        .map(canonical_json_string)
                        .unwrap_or_else(|| "null".to_string());
                    format!("{key_json}:{value_json}")
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{parts}}}")
        }
    }
}

fn kb_workflow_pack_checksum_for_stable(stable: &serde_json::Value) -> String {
    format!("sha256:{}", sha256_hex(&canonical_json_string(stable)))
}

fn kb_workflow_pack_supported_type(pack_type: &str) -> bool {
    matches!(
        pack_type,
        "requirement_context_pack"
            | "development_handoff_pack"
            | "verification_evidence_pack"
            | "retrospective_pack"
            | "project_knowledge_pack"
    )
}

fn kb_workflow_pack_issue(
    severity: &str,
    code: &str,
    path: &str,
    message: &str,
) -> KbWorkflowPackValidationIssue {
    KbWorkflowPackValidationIssue {
        severity: severity.into(),
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn kb_workflow_pack_required_string(
    package_json: &serde_json::Value,
    field: &str,
    issues: &mut Vec<KbWorkflowPackValidationIssue>,
) -> String {
    let value = package_json
        .get(field)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if value.is_empty() {
        issues.push(kb_workflow_pack_issue(
            "error",
            "missing_required_field",
            &format!("$.{field}"),
            "缺少必填字符串字段。",
        ));
    }
    value
}

fn kb_workflow_pack_calculated_checksum(package_json: &serde_json::Value) -> String {
    let stable = serde_json::json!({
        "schema_version": package_json.get("schema_version").cloned().unwrap_or_else(|| serde_json::json!("")),
        "pack_type": package_json.get("pack_type").cloned().unwrap_or_else(|| serde_json::json!("")),
        "title": package_json.get("title").cloned().unwrap_or_else(|| serde_json::json!("")),
        "source": package_json.get("source").cloned().unwrap_or_else(|| serde_json::json!({})),
        "items": package_json.get("items").cloned().unwrap_or_else(|| serde_json::json!([])),
        "markdown": package_json.get("markdown").cloned().unwrap_or_else(|| serde_json::json!("")),
    });
    kb_workflow_pack_checksum_for_stable(&stable)
}

fn kb_workflow_pack_string_field(package_json: &serde_json::Value, field: &str) -> String {
    package_json
        .get(field)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn kb_workflow_pack_load_package(
    conn: &Connection,
    pack_id: &str,
) -> Result<serde_json::Value, String> {
    let raw: String = conn
        .query_row(
            "SELECT package_json FROM workflow_packs WHERE id=?1",
            params![pack_id],
            |row| row.get(0),
        )
        .map_err(|err| {
            if matches!(err, rusqlite::Error::QueryReturnedNoRows) {
                "pack_not_found".to_string()
            } else {
                err.to_string()
            }
        })?;
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

fn kb_workflow_pack_source_exists(
    conn: &Connection,
    source_ref: &str,
) -> Result<Option<bool>, String> {
    let Some((source_table, source_id)) = source_ref.split_once(':') else {
        return Ok(None);
    };
    if source_id.trim().is_empty() {
        return Ok(Some(false));
    }
    let sql = match source_table {
        "items" => "SELECT 1 FROM items WHERE item_id=?1 LIMIT 1",
        "task_starter_sessions" => "SELECT 1 FROM task_starter_sessions WHERE id=?1 LIMIT 1",
        "task_starter_evidence" => "SELECT 1 FROM task_starter_evidence WHERE id=?1 LIMIT 1",
        "project_health_snapshots" => {
            "SELECT 1 FROM project_health_snapshots WHERE project_id=?1 LIMIT 1"
        }
        "project_action_items" => "SELECT 1 FROM project_action_items WHERE project_id=?1 LIMIT 1",
        "prompt_templates" => "SELECT 1 FROM prompt_templates WHERE id=?1 LIMIT 1",
        "knowledge_units" => "SELECT 1 FROM knowledge_units WHERE id=?1 LIMIT 1",
        "workflow_packs" => "SELECT 1 FROM workflow_packs WHERE id=?1 LIMIT 1",
        _ => return Ok(None),
    };
    let mut stmt = conn.prepare(sql).map_err(|err| err.to_string())?;
    let exists = stmt
        .exists(params![source_id])
        .map_err(|err| err.to_string())?;
    Ok(Some(exists))
}

fn kb_workflow_pack_markdown_section(title: &str, items: &[serde_json::Value]) -> String {
    if items.is_empty() {
        return format!("## {title}\n\n- 暂无。\n\n");
    }
    let mut out = format!("## {title}\n\n");
    for item in items {
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("-");
        let source_ref = item
            .get("source_ref")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        let reason = item
            .get("payload")
            .and_then(|payload| payload.get("reason"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        out.push_str(&format!("- {title} (`{source_ref}`)"));
        if !reason.is_empty() {
            out.push_str(&format!("：{reason}"));
        }
        out.push('\n');
    }
    out.push('\n');
    out
}

fn kb_workflow_pack_items_from_evidence(
    evidence: &[KbTaskStarterEvidenceItem],
) -> Vec<serde_json::Value> {
    evidence
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let source_ref = format!("{}:{}", item.source_table, item.source_id);
            serde_json::json!({
                "item_id": format!("pack-item-{}", fnv1a64_hex(&format!("{source_ref}:{}", item.title))),
                "item_type": item.evidence_type,
                "title": item.title,
                "source_ref": source_ref,
                "required": index < 3,
                "payload": {
                    "excerpt": item.excerpt,
                    "score": item.score,
                    "reason": item.reason,
                    "source_path": item.source_path
                }
            })
        })
        .collect()
}

fn kb_workflow_pack_insert(
    conn: &Connection,
    pack_id: &str,
    pack_type: &str,
    title: &str,
    source_ref: &str,
    package_json: &serde_json::Value,
    markdown: &str,
    checksum: &str,
    status: &str,
) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO workflow_packs(
          id, pack_type, schema_version, title, source_ref,
          package_json, package_markdown, checksum, status, updated_at
        ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
          title=excluded.title,
          source_ref=excluded.source_ref,
          package_json=excluded.package_json,
          package_markdown=excluded.package_markdown,
          checksum=excluded.checksum,
          status=excluded.status,
          updated_at=CURRENT_TIMESTAMP
        "#,
        params![
            pack_id,
            pack_type,
            WORKFLOW_PACK_SCHEMA_VERSION,
            title,
            source_ref,
            package_json.to_string(),
            markdown,
            checksum,
            status
        ],
    )
    .map_err(|err| err.to_string())?;
    conn.execute(
        "DELETE FROM workflow_pack_items WHERE pack_id=?1",
        params![pack_id],
    )
    .map_err(|err| err.to_string())?;
    if let Some(items) = package_json.get("items").and_then(|value| value.as_array()) {
        for item in items {
            let item_id = item
                .get("item_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let item_type = item
                .get("item_type")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let title = item
                .get("title")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let source_ref = item
                .get("source_ref")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let (source_table, source_id) = source_ref.split_once(':').unwrap_or(("", ""));
            let required = item
                .get("required")
                .and_then(|value| value.as_bool())
                .unwrap_or(false) as i64;
            let db_item_id = format!("{pack_id}:{item_id}");
            conn.execute(
                r#"
                INSERT INTO workflow_pack_items(
                  id, pack_id, item_type, source_table, source_id, title, required, payload_json
                ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    db_item_id,
                    pack_id,
                    item_type,
                    source_table,
                    source_id,
                    title,
                    required,
                    item.to_string()
                ],
            )
            .map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn kb_workflow_pack_finalize(
    conn: &Connection,
    pack_type: &str,
    title: &str,
    source: serde_json::Value,
    source_ref: &str,
    mut package_json: serde_json::Value,
    markdown: String,
) -> Result<KbWorkflowPackExportResponse, String> {
    package_json["markdown"] = serde_json::json!(markdown);
    let checksum_package_json =
        serde_json::from_str::<serde_json::Value>(&package_json.to_string())
            .unwrap_or_else(|_| package_json.clone());
    let checksum = kb_workflow_pack_calculated_checksum(&checksum_package_json);
    let pack_id = format!(
        "workflow-pack-{}-{}",
        pack_type,
        fnv1a64_hex(&format!("{title}:{source_ref}:{checksum}"))
    );
    package_json["pack_id"] = serde_json::json!(pack_id);
    package_json["checksum"] = serde_json::json!(checksum);
    kb_workflow_pack_insert(
        conn,
        &pack_id,
        pack_type,
        title,
        source_ref,
        &package_json,
        package_json
            .get("markdown")
            .and_then(|value| value.as_str())
            .unwrap_or_default(),
        package_json
            .get("checksum")
            .and_then(|value| value.as_str())
            .unwrap_or_default(),
        "exported",
    )?;
    let items = package_json
        .get("items")
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0);
    Ok(KbWorkflowPackExportResponse {
        pack_id,
        pack_type: pack_type.into(),
        schema_version: WORKFLOW_PACK_SCHEMA_VERSION.into(),
        title: title.into(),
        source,
        item_count: items,
        checksum,
        markdown: package_json
            .get("markdown")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .into(),
        package_json,
    })
}

fn kb_workflow_pack_export_development(
    conn: &Connection,
    payload: &KbWorkflowPackExportRequest,
    pack_type: &str,
) -> Result<KbWorkflowPackExportResponse, String> {
    let input = payload
        .input_text
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(payload.task_id.as_deref())
        .or(payload.req_id.as_deref())
        .unwrap_or_default()
        .trim()
        .to_string();
    if input.is_empty() {
        return Err("empty_input".into());
    }
    let preview = kb_task_context_readonly_internal(&input, payload.limit.unwrap_or(8))?;
    let items = kb_workflow_pack_items_from_evidence(&preview.evidence);
    let title = if preview.parsed_task_id.is_empty() {
        format!("工作流包：{}", compact_text_chars(&input, 40))
    } else {
        format!("工作流包：{}", preview.parsed_task_id)
    };
    let markdown = format!(
        "# {title}\n\n## Source\n\n- input: `{}`\n- req_id: `{}`\n- task_id: `{}`\n- input_type: `{}`\n\n{}",
        input,
        preview.parsed_req_id,
        preview.parsed_task_id,
        preview.input_type,
        kb_workflow_pack_markdown_section("Evidence Index", &items)
    );
    let source = serde_json::json!({
        "input_text": input,
        "req_id": preview.parsed_req_id,
        "task_id": preview.parsed_task_id,
        "input_type": preview.input_type
    });
    let package_json = serde_json::json!({
        "schema_version": WORKFLOW_PACK_SCHEMA_VERSION,
        "pack_type": pack_type,
        "pack_id": "",
        "title": title,
        "created_at": Utc::now().to_rfc3339(),
        "source": source.clone(),
        "items": items,
        "markdown": "",
        "checksum": ""
    });
    let source_ref = if !preview.parsed_task_id.is_empty() {
        preview.parsed_task_id.as_str()
    } else if !preview.parsed_req_id.is_empty() {
        preview.parsed_req_id.as_str()
    } else {
        input.as_str()
    };
    kb_workflow_pack_finalize(
        conn,
        pack_type,
        &title,
        source,
        source_ref,
        package_json,
        markdown,
    )
}

fn kb_workflow_pack_export_project(
    conn: &Connection,
    payload: &KbWorkflowPackExportRequest,
) -> Result<KbWorkflowPackExportResponse, String> {
    let project_id = payload.project_id.as_deref().unwrap_or_default().trim();
    if project_id.is_empty() {
        return Err("empty_project_id".into());
    }
    let snapshot = kb_project_snapshot_by_id(project_id)?;
    let actions = kb_project_action_items_for_snapshot(&snapshot);
    let mut items = Vec::new();
    for (index, risk) in snapshot.risks.iter().enumerate() {
        items.push(serde_json::json!({
            "item_id": format!("pack-risk-{}-{}", snapshot.project_id, index),
            "item_type": "risk",
            "title": risk,
            "source_ref": format!("project_health_snapshots:{}", snapshot.project_id),
            "required": index < 3,
            "payload": { "risk": risk }
        }));
    }
    for action in &actions {
        items.push(serde_json::json!({
            "item_id": format!("pack-action-{}-{}", snapshot.project_id, fnv1a64_hex(&action.title)),
            "item_type": "action",
            "title": action.title,
            "source_ref": format!("project_action_items:{}", snapshot.project_id),
            "required": action.priority == "P0",
            "payload": {
                "action_type": action.action_type,
                "priority": action.priority,
                "reason": action.reason,
                "route_hint": action.route_hint,
                "starter_input": action.starter_input
            }
        }));
    }
    let title = format!("项目知识包：{}", snapshot.name);
    let markdown = format!(
        "# {title}\n\n## Project\n\n- name: `{}`\n- root_path: `{}`\n- health_score: `{}`\n- collection_coverage: `{}%`\n\n{}",
        snapshot.name,
        snapshot.root_path,
        snapshot.health_score,
        snapshot.collection_coverage,
        kb_workflow_pack_markdown_section("Evidence Index", &items)
    );
    let source = serde_json::json!({
        "project_id": snapshot.project_id,
        "project_name": snapshot.name,
        "root_path": snapshot.root_path
    });
    let package_json = serde_json::json!({
        "schema_version": WORKFLOW_PACK_SCHEMA_VERSION,
        "pack_type": "project_knowledge_pack",
        "pack_id": "",
        "title": title,
        "created_at": Utc::now().to_rfc3339(),
        "source": source.clone(),
        "items": items,
        "markdown": "",
        "checksum": ""
    });
    kb_workflow_pack_finalize(
        conn,
        "project_knowledge_pack",
        &title,
        source,
        project_id,
        package_json,
        markdown,
    )
}

fn kb_workflow_pack_export_internal(
    payload: KbWorkflowPackExportRequest,
) -> Result<KbWorkflowPackExportResponse, String> {
    let conn = connect_knowledgebase()?;
    let pack_type = payload
        .pack_type
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if payload
                .project_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                "development_handoff_pack"
            } else {
                "project_knowledge_pack"
            }
        });
    match pack_type {
        "development_handoff_pack" | "requirement_context_pack" | "verification_evidence_pack" => {
            kb_workflow_pack_export_development(&conn, &payload, pack_type)
        }
        "project_knowledge_pack" => kb_workflow_pack_export_project(&conn, &payload),
        "retrospective_pack" => kb_workflow_pack_export_development(&conn, &payload, pack_type),
        _ => Err("unsupported_pack_type".into()),
    }
}

fn kb_workflow_pack_validate_package(
    conn: &Connection,
    package_json: serde_json::Value,
) -> Result<KbWorkflowPackValidateResponse, String> {
    if !package_json.is_object() {
        return Ok(KbWorkflowPackValidateResponse {
            valid: false,
            importable: false,
            pack_id: String::new(),
            pack_type: String::new(),
            schema_version: String::new(),
            checksum: String::new(),
            calculated_checksum: String::new(),
            item_count: 0,
            issues: vec![kb_workflow_pack_issue(
                "error",
                "invalid_package_json",
                "$",
                "工作流包必须是 JSON object。",
            )],
            package_json,
        });
    }

    let mut issues = Vec::new();
    let pack_id = kb_workflow_pack_required_string(&package_json, "pack_id", &mut issues);
    let pack_type = kb_workflow_pack_required_string(&package_json, "pack_type", &mut issues);
    let schema_version =
        kb_workflow_pack_required_string(&package_json, "schema_version", &mut issues);
    let title = kb_workflow_pack_required_string(&package_json, "title", &mut issues);
    let created_at = kb_workflow_pack_required_string(&package_json, "created_at", &mut issues);
    let markdown = kb_workflow_pack_required_string(&package_json, "markdown", &mut issues);
    let checksum = kb_workflow_pack_required_string(&package_json, "checksum", &mut issues);
    let _ = (title, created_at, markdown);

    if !pack_type.is_empty() && !kb_workflow_pack_supported_type(&pack_type) {
        issues.push(kb_workflow_pack_issue(
            "error",
            "unsupported_pack_type",
            "$.pack_type",
            "不支持的工作流包类型。",
        ));
    }
    if !schema_version.is_empty() && schema_version != WORKFLOW_PACK_SCHEMA_VERSION {
        issues.push(kb_workflow_pack_issue(
            "error",
            "incompatible_schema_version",
            "$.schema_version",
            "当前应用只支持 workflow pack schema 1.0.0。",
        ));
    }
    if !package_json
        .get("source")
        .map(|value| value.is_object())
        .unwrap_or(false)
    {
        issues.push(kb_workflow_pack_issue(
            "error",
            "missing_required_field",
            "$.source",
            "缺少必填对象字段。",
        ));
    }

    let items = package_json
        .get("items")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if !package_json
        .get("items")
        .map(|value| value.is_array())
        .unwrap_or(false)
    {
        issues.push(kb_workflow_pack_issue(
            "error",
            "missing_required_field",
            "$.items",
            "缺少必填数组字段。",
        ));
    }

    for (index, item) in items.iter().enumerate() {
        let path = format!("$.items[{index}]");
        if !item.is_object() {
            issues.push(kb_workflow_pack_issue(
                "error",
                "invalid_item",
                &path,
                "包内条目必须是 JSON object。",
            ));
            continue;
        }
        for field in ["item_id", "item_type", "title", "source_ref"] {
            let value = item
                .get(field)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim();
            if value.is_empty() {
                issues.push(kb_workflow_pack_issue(
                    "error",
                    "missing_item_field",
                    &format!("{path}.{field}"),
                    "缺少包内条目必填字符串字段。",
                ));
            }
        }
        if !item
            .get("required")
            .map(|value| value.is_boolean())
            .unwrap_or(false)
        {
            issues.push(kb_workflow_pack_issue(
                "error",
                "missing_item_field",
                &format!("{path}.required"),
                "缺少包内条目必填布尔字段。",
            ));
        }
        if item.get("payload").is_none() {
            issues.push(kb_workflow_pack_issue(
                "error",
                "missing_item_field",
                &format!("{path}.payload"),
                "缺少包内条目 payload 字段。",
            ));
        }

        let source_ref = item
            .get("source_ref")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim();
        if !source_ref.is_empty() {
            match kb_workflow_pack_source_exists(conn, source_ref)? {
                Some(true) => {}
                Some(false) => issues.push(kb_workflow_pack_issue(
                    "warning",
                    "missing_source_ref",
                    &format!("{path}.source_ref"),
                    "本地知识库未找到该来源引用，导入后只能作为候选证据保留。",
                )),
                None => issues.push(kb_workflow_pack_issue(
                    "warning",
                    "unknown_source_ref",
                    &format!("{path}.source_ref"),
                    "暂不支持自动检查该来源类型。",
                )),
            }
        }

        if let Some(source_path) = item
            .get("payload")
            .and_then(|payload| payload.get("source_path"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let path_obj = Path::new(source_path);
            if !path_obj.exists() {
                issues.push(kb_workflow_pack_issue(
                    "warning",
                    "missing_source_file",
                    &format!("{path}.payload.source_path"),
                    "本地文件路径不存在。",
                ));
            }
        }
    }

    let calculated_checksum = kb_workflow_pack_calculated_checksum(&package_json);
    if !checksum.is_empty() && checksum != calculated_checksum {
        issues.push(kb_workflow_pack_issue(
            "error",
            "checksum_mismatch",
            "$.checksum",
            "包内 checksum 与当前内容计算结果不一致。",
        ));
    }

    if !pack_id.is_empty() {
        let existing_checksum = conn
            .query_row(
                "SELECT checksum FROM workflow_packs WHERE id=?1",
                params![pack_id],
                |row| row.get::<_, String>(0),
            )
            .ok();
        if let Some(existing_checksum) = existing_checksum {
            if existing_checksum == checksum {
                issues.push(kb_workflow_pack_issue(
                    "warning",
                    "duplicate_pack",
                    "$.pack_id",
                    "本地已存在相同 checksum 的工作流包，导入会幂等更新候选包记录。",
                ));
            } else {
                issues.push(kb_workflow_pack_issue(
                    "error",
                    "pack_id_conflict",
                    "$.pack_id",
                    "本地已存在同 ID 但 checksum 不同的工作流包。",
                ));
            }
        }
    }

    let valid = !issues.iter().any(|issue| issue.severity == "error");
    Ok(KbWorkflowPackValidateResponse {
        valid,
        importable: valid,
        pack_id,
        pack_type,
        schema_version,
        checksum,
        calculated_checksum,
        item_count: items.len(),
        issues,
        package_json,
    })
}

fn kb_workflow_pack_validate_internal(
    payload: KbWorkflowPackValidateRequest,
) -> Result<KbWorkflowPackValidateResponse, String> {
    let conn = connect_knowledgebase()?;
    let package_json = if let Some(package_json) = payload.package_json {
        package_json
    } else if let Some(pack_id) = payload
        .pack_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        kb_workflow_pack_load_package(&conn, pack_id)?
    } else {
        return Err("empty_package".into());
    };
    kb_workflow_pack_validate_package(&conn, package_json)
}

fn kb_workflow_pack_import_internal(
    payload: KbWorkflowPackImportRequest,
) -> Result<KbWorkflowPackImportResponse, String> {
    let conn = connect_knowledgebase()?;
    let package_json = payload
        .package_json
        .ok_or_else(|| "empty_package".to_string())?;
    let validation = kb_workflow_pack_validate_package(&conn, package_json)?;
    if !validation.importable {
        return Ok(KbWorkflowPackImportResponse {
            imported: false,
            pack_id: validation.pack_id.clone(),
            status: "rejected".into(),
            validation,
        });
    }
    let title = kb_workflow_pack_string_field(&validation.package_json, "title");
    let source_ref = validation
        .package_json
        .get("source")
        .and_then(|source| {
            source
                .get("task_id")
                .or_else(|| source.get("req_id"))
                .or_else(|| source.get("project_id"))
        })
        .and_then(|value| value.as_str())
        .unwrap_or(&validation.pack_id)
        .to_string();
    let markdown = kb_workflow_pack_string_field(&validation.package_json, "markdown");
    kb_workflow_pack_insert(
        &conn,
        &validation.pack_id,
        &validation.pack_type,
        &title,
        &source_ref,
        &validation.package_json,
        &markdown,
        &validation.checksum,
        "imported",
    )?;
    Ok(KbWorkflowPackImportResponse {
        imported: true,
        pack_id: validation.pack_id.clone(),
        status: "imported".into(),
        validation,
    })
}

fn kb_workflow_pack_detail_internal(pack_id: &str) -> Result<KbWorkflowPackDetailResponse, String> {
    let conn = connect_knowledgebase()?;
    let (
        pack_id,
        pack_type,
        schema_version,
        title,
        source_ref,
        package_json_raw,
        markdown,
        checksum,
        status,
        updated_at,
        created_at,
    ): (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT id, pack_type, schema_version, title, source_ref, package_json,
                    package_markdown, checksum, status, updated_at, created_at
             FROM workflow_packs
             WHERE id=?1",
            params![pack_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                ))
            },
        )
        .map_err(|err| {
            if matches!(err, rusqlite::Error::QueryReturnedNoRows) {
                "pack_not_found".to_string()
            } else {
                err.to_string()
            }
        })?;
    let package_json =
        serde_json::from_str(&package_json_raw).unwrap_or_else(|_| serde_json::json!({}));
    let mut stmt = conn
        .prepare(
            "SELECT payload_json FROM workflow_pack_items WHERE pack_id=?1 ORDER BY required DESC, item_type, title",
        )
        .map_err(|err| err.to_string())?;
    let items = stmt
        .query_map(params![pack_id], |row| row.get::<_, String>(0))
        .map_err(|err| err.to_string())?
        .filter_map(|row| row.ok())
        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .collect::<Vec<_>>();
    Ok(KbWorkflowPackDetailResponse {
        pack_id,
        pack_type,
        schema_version,
        title,
        source_ref,
        checksum,
        status,
        markdown,
        package_json,
        items,
        updated_at,
        created_at,
    })
}

#[allow(dead_code)]
fn kb_health_template_primary_source(conn: &Connection, template_id: &str) -> String {
    conn.query_row(
        r#"
        SELECT item_id
        FROM prompt_template_sources
        WHERE template_id = ?1
        ORDER BY confidence DESC, created_at DESC
        LIMIT 1
        "#,
        params![template_id],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_default()
}

#[allow(dead_code)]
fn kb_health_actions_internal() -> Result<KbHealthActionsResponse, String> {
    let conn = connect_knowledgebase()?;
    let assets = kb_health_assets_internal()?.assets;
    let projects = kb_health_projects_internal()?.projects;
    let mut actions = Vec::new();
    for asset in assets.iter().filter(|item| item.score < 80).take(8) {
        let evidence_item_id = kb_health_template_primary_source(&conn, &asset.asset_id);
        actions.push(KbHealthAction {
            target_type: asset.asset_type.clone(),
            target_id: asset.asset_id.clone(),
            title: asset.title.clone(),
            score: asset.score,
            priority: if asset.score < 50 {
                "P0".into()
            } else {
                "P1".into()
            },
            reason: asset.reasons.first().cloned().unwrap_or_default(),
            suggested_action: asset.suggested_action.clone(),
            primary_route: "prompt".into(),
            evidence_item_id,
            search_query: asset.title.clone(),
            graph_query: asset.category.clone(),
            starter_input: format!(
                "整理健康度模板：{}。问题：{}",
                asset.title,
                asset.reasons.first().cloned().unwrap_or_default()
            ),
        });
    }
    for project in projects.iter().filter(|item| item.score < 80).take(5) {
        actions.push(KbHealthAction {
            target_type: "project".into(),
            target_id: project.project_id.clone(),
            title: project.name.clone(),
            score: project.score,
            priority: if project.score < 50 {
                "P0".into()
            } else {
                "P1".into()
            },
            reason: project.reasons.first().cloned().unwrap_or_default(),
            suggested_action: project.suggested_action.clone(),
            primary_route: "project".into(),
            evidence_item_id: String::new(),
            search_query: project.name.clone(),
            graph_query: project.name.clone(),
            starter_input: format!(
                "整理项目知识健康度：{}。问题：{}",
                project.name,
                project.reasons.first().cloned().unwrap_or_default()
            ),
        });
    }
    actions.sort_by(|left, right| left.score.cmp(&right.score));
    let summary = kb_health_summary(&assets, &projects);
    Ok(KbHealthActionsResponse { summary, actions })
}

fn kb_task_starter_extract_identifier(input: &str, prefix: &str) -> String {
    let upper = input.to_ascii_uppercase();
    let Some(start) = upper.find(prefix) else {
        return String::new();
    };
    upper[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect::<String>()
}

fn kb_task_starter_parse_input(input: &str) -> (String, String, String) {
    let task_id = kb_task_starter_extract_identifier(input, "TASK-");
    let req_id = kb_task_starter_extract_identifier(input, "REQ-");
    let input_type = if !task_id.is_empty() {
        "task"
    } else if !req_id.is_empty() {
        "req"
    } else {
        "text"
    };
    (input_type.into(), req_id, task_id)
}

fn kb_task_starter_like_pattern(input: &str, req_id: &str, task_id: &str) -> String {
    if !task_id.is_empty() {
        format!("%{}%", task_id)
    } else if !req_id.is_empty() {
        format!("%{}%", req_id)
    } else {
        let compact = input
            .split_whitespace()
            .take(8)
            .collect::<Vec<_>>()
            .join(" ");
        format!("%{}%", compact.trim())
    }
}

fn kb_task_starter_primary_identifier(req_id: &str, task_id: &str) -> String {
    if !task_id.trim().is_empty() {
        task_id.trim().to_string()
    } else {
        req_id.trim().to_string()
    }
}

fn kb_task_starter_push_unique(
    items: &mut Vec<KbTaskStarterEvidenceItem>,
    seen: &mut HashSet<String>,
    item: KbTaskStarterEvidenceItem,
    limit: usize,
) {
    if items.len() >= limit {
        return;
    }
    let key = format!(
        "{}:{}:{}",
        item.evidence_type, item.source_table, item.source_id
    );
    if seen.insert(key) {
        items.push(item);
    }
}

fn kb_task_starter_collect_item_rows(
    conn: &Connection,
    sql: &str,
    params_value: &str,
    evidence_type: &str,
    reason: &str,
    base_score: f64,
    limit: usize,
) -> Result<Vec<KbTaskStarterEvidenceItem>, String> {
    let mut stmt = conn.prepare(sql).map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![params_value, limit as i64], |row| {
            Ok(KbTaskStarterEvidenceItem {
                evidence_type: evidence_type.into(),
                source_table: "items".into(),
                source_id: row.get::<_, String>(0)?,
                title: row.get::<_, String>(1)?,
                excerpt: compact_text_chars(&row.get::<_, String>(2).unwrap_or_default(), 180),
                score: base_score,
                reason: reason.into(),
                source_path: row.get::<_, String>(3).unwrap_or_default(),
            })
        })
        .map_err(|err| err.to_string())?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|err| err.to_string())?);
    }
    Ok(items)
}

fn kb_task_starter_workflow_root() -> Option<PathBuf> {
    let current = env::current_dir().ok()?;
    for ancestor in current.ancestors() {
        let candidate = ancestor.join("docs/workflow");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn kb_task_starter_file_score(path: &Path) -> f64 {
    let value = path.to_string_lossy();
    if value.contains("任务看板") {
        118.0
    } else if value.contains("需求池") {
        116.0
    } else if value.contains("PRD") {
        112.0
    } else if value.contains("design") || value.contains("设计") {
        108.0
    } else if value.contains("testing") || value.contains("测试") {
        104.0
    } else {
        100.0
    }
}

fn kb_task_starter_collect_workflow_docs(
    identifier: &str,
    limit: usize,
) -> Vec<KbTaskStarterEvidenceItem> {
    if identifier.trim().is_empty() {
        return Vec::new();
    }
    let Some(root) = kb_task_starter_workflow_root() else {
        return Vec::new();
    };
    let mut stack = vec![root];
    let mut matches = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|item| item.to_str()) != Some("md") {
                continue;
            }
            let path_text = path.to_string_lossy().to_string();
            let content = fs::read_to_string(&path).unwrap_or_default();
            if !path_text.contains(identifier) && !content.contains(identifier) {
                continue;
            }
            let title = path
                .file_name()
                .and_then(|item| item.to_str())
                .unwrap_or("workflow-doc")
                .to_string();
            matches.push(KbTaskStarterEvidenceItem {
                evidence_type: "similar_task".into(),
                source_table: "workflow_docs".into(),
                source_id: path_text.clone(),
                title,
                excerpt: compact_text_chars(&content, 220),
                score: kb_task_starter_file_score(&path),
                reason: format!("本地 workflow 文档精确命中 {identifier}"),
                source_path: path_text,
            });
        }
    }
    matches.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches.truncate(limit);
    matches
}

fn kb_task_starter_collect_exact_governance(
    conn: &Connection,
    req_id: &str,
    task_id: &str,
    limit: usize,
) -> Result<Vec<KbTaskStarterEvidenceItem>, String> {
    let identifier = kb_task_starter_primary_identifier(req_id, task_id);
    if identifier.is_empty() {
        return Ok(Vec::new());
    }
    let like_pattern = format!("%{}%", identifier);
    let mut stmt = conn
        .prepare(
            r#"
            SELECT item_id, title, content_text, source_path,
                   CASE
                     WHEN source_path LIKE '%任务看板%' THEN 120
                     WHEN source_path LIKE '%需求池%' THEN 116
                     WHEN source_path LIKE '%PRD%' THEN 112
                     WHEN source_path LIKE '%design%' OR source_path LIKE '%设计%' THEN 108
                     WHEN source_path LIKE '%testing%' OR source_path LIKE '%测试%' THEN 104
                     ELSE 100
                   END AS score
            FROM items
            WHERE title LIKE ?1 OR content_text LIKE ?1 OR source_path LIKE ?1
            ORDER BY score DESC, updated_at DESC
            LIMIT ?2
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![like_pattern, limit as i64], |row| {
            let score = row.get::<_, i64>(4).unwrap_or(100) as f64;
            Ok(KbTaskStarterEvidenceItem {
                evidence_type: "similar_task".into(),
                source_table: "items".into(),
                source_id: row.get::<_, String>(0)?,
                title: row.get::<_, String>(1)?,
                excerpt: compact_text_chars(&row.get::<_, String>(2).unwrap_or_default(), 220),
                score,
                reason: format!("精确命中 {identifier}，优先作为治理上下文"),
                source_path: row.get::<_, String>(3).unwrap_or_default(),
            })
        })
        .map_err(|err| err.to_string())?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|err| err.to_string())?);
    }
    if items.len() < limit {
        for item in kb_task_starter_collect_workflow_docs(&identifier, limit - items.len()) {
            items.push(item);
        }
    }
    Ok(items)
}

fn kb_task_starter_collect_templates(
    conn: &Connection,
    like_pattern: &str,
    input: &str,
    limit: usize,
) -> Result<Vec<KbTaskStarterEvidenceItem>, String> {
    let normalized_input = input.to_ascii_lowercase();
    let wants_dev = input.contains("开发")
        || input.contains("开工")
        || input.contains("交接")
        || normalized_input.contains("task")
        || normalized_input.contains("req");
    let wants_verify = input.contains("验证") || input.contains("测试") || input.contains("验收");
    let wants_governance = input.contains("治理")
        || input.contains("需求")
        || input.contains("任务")
        || normalized_input.contains("workflow");
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, name,
                   COALESCE(NULLIF(task_goal, ''), NULLIF(review_note, ''), category) AS excerpt,
                   status, COALESCE(quality_score, 60) AS quality_score,
                   CASE
                     WHEN ?2 = 1 AND (name LIKE '%开发%' OR name LIKE '%交接%' OR task_goal LIKE '%开发%' OR category LIKE '%开发%') THEN 18
                     WHEN ?3 = 1 AND (name LIKE '%验证%' OR name LIKE '%测试%' OR name LIKE '%验收%' OR task_goal LIKE '%验证%' OR category LIKE '%测试%') THEN 16
                     WHEN ?4 = 1 AND (name LIKE '%治理%' OR name LIKE '%需求%' OR name LIKE '%任务%' OR task_goal LIKE '%workflow%' OR category LIKE '%工作流%') THEN 14
                     WHEN name LIKE ?1 OR category LIKE ?1 OR task_goal LIKE ?1 OR review_note LIKE ?1 THEN 10
                     ELSE 0
                   END AS relevance
            FROM prompt_templates
            WHERE status IN ('verified', 'reviewed', 'candidate')
              AND (
                name LIKE ?1 OR category LIKE ?1 OR task_goal LIKE ?1 OR review_note LIKE ?1
                OR ?2 = 1 OR ?3 = 1 OR ?4 = 1 OR ?1 = '%%'
              )
            ORDER BY CASE status
              WHEN 'verified' THEN 0
              WHEN 'reviewed' THEN 1
              ELSE 2
            END, relevance DESC, quality_score DESC, updated_at DESC
            LIMIT ?5
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(
            params![
                like_pattern,
                if wants_dev { 1_i64 } else { 0_i64 },
                if wants_verify { 1_i64 } else { 0_i64 },
                if wants_governance { 1_i64 } else { 0_i64 },
                limit as i64
            ],
            |row| {
                let status = row.get::<_, String>(3).unwrap_or_default();
                let quality_score = row.get::<_, i64>(4).unwrap_or(60) as f64;
                let relevance = row.get::<_, i64>(5).unwrap_or(0) as f64;
                Ok(KbTaskStarterEvidenceItem {
                    evidence_type: "template".into(),
                    source_table: "prompt_templates".into(),
                    source_id: row.get::<_, String>(0)?,
                    title: row.get::<_, String>(1)?,
                    excerpt: compact_text_chars(&row.get::<_, String>(2).unwrap_or_default(), 180),
                    score: 80.0 + relevance + quality_score / 5.0,
                    reason: format!("按状态、质量和任务相关性排序；当前状态为 {status}"),
                    source_path: String::new(),
                })
            },
        )
        .map_err(|err| err.to_string())?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|err| err.to_string())?);
    }
    Ok(items)
}

fn kb_task_starter_collect_evidence(
    conn: &Connection,
    input: &str,
    req_id: &str,
    task_id: &str,
    limit: usize,
) -> Result<KbTaskStarterSections, String> {
    let limit = limit.clamp(3, 12);
    let like_pattern = kb_task_starter_like_pattern(input, req_id, task_id);
    let broad_pattern = if like_pattern.trim() == "%%" {
        "%".to_string()
    } else {
        like_pattern.clone()
    };
    let mut seen = HashSet::new();
    let mut sections = KbTaskStarterSections::default();

    for item in kb_task_starter_collect_exact_governance(conn, req_id, task_id, limit)? {
        kb_task_starter_push_unique(&mut sections.similar_tasks, &mut seen, item, limit);
    }

    for item in kb_task_starter_collect_item_rows(
        conn,
        r#"
        SELECT item_id, title, content_text, source_path
        FROM items
        WHERE title LIKE ?1 OR content_text LIKE ?1 OR source_path LIKE ?1
        ORDER BY updated_at DESC
        LIMIT ?2
        "#,
        &broad_pattern,
        "similar_task",
        "命中输入关键词，可作为历史相似任务或上下文证据",
        70.0,
        limit,
    )? {
        kb_task_starter_push_unique(&mut sections.similar_tasks, &mut seen, item, limit);
    }

    for item in kb_task_starter_collect_item_rows(
        conn,
        r#"
        SELECT item_id, title, content_text, source_path
        FROM items
        WHERE (title LIKE ?1 OR content_text LIKE ?1 OR source_path LIKE ?1)
          AND (
            title LIKE '%风险%' OR content_text LIKE '%风险%'
            OR title LIKE '%问题%' OR content_text LIKE '%问题%'
            OR title LIKE '%阻塞%' OR content_text LIKE '%阻塞%'
            OR title LIKE '%失败%' OR content_text LIKE '%失败%'
            OR title LIKE '%未覆盖%' OR content_text LIKE '%未覆盖%'
          )
        ORDER BY updated_at DESC
        LIMIT ?2
        "#,
        &broad_pattern,
        "risk",
        "命中风险、问题、阻塞或未覆盖关键词",
        85.0,
        limit,
    )? {
        kb_task_starter_push_unique(&mut sections.risks, &mut seen, item, limit);
    }

    sections.templates = kb_task_starter_collect_templates(conn, &broad_pattern, input, limit)?;

    for item in kb_task_starter_collect_item_rows(
        conn,
        r#"
        SELECT item_id, title, content_text, source_path
        FROM items
        WHERE (title LIKE ?1 OR content_text LIKE ?1 OR source_path LIKE ?1)
          AND (
            source_path LIKE '%docs/workflow%'
            OR source_path LIKE '%.md'
            OR title LIKE '%设计%'
            OR title LIKE '%任务拆解%'
            OR title LIKE '%PRD%'
          )
        ORDER BY updated_at DESC
        LIMIT ?2
        "#,
        &broad_pattern,
        "file",
        "建议开工前阅读的治理、设计或文档证据",
        75.0,
        limit,
    )? {
        kb_task_starter_push_unique(&mut sections.suggested_files, &mut seen, item, limit);
    }

    for item in kb_task_starter_collect_item_rows(
        conn,
        r#"
        SELECT item_id, title, content_text, source_path
        FROM items
        WHERE (title LIKE ?1 OR content_text LIKE ?1 OR source_path LIKE ?1)
          AND (
            content_text LIKE '%cargo check%'
            OR content_text LIKE '%npm run build%'
            OR content_text LIKE '%pytest%'
            OR content_text LIKE '%验证命令%'
            OR content_text LIKE '%测试%'
          )
        ORDER BY updated_at DESC
        LIMIT ?2
        "#,
        &broad_pattern,
        "verify",
        "包含验证命令、测试记录或回归证据",
        80.0,
        limit,
    )? {
        kb_task_starter_push_unique(&mut sections.verify_commands, &mut seen, item, limit);
    }

    Ok(sections)
}

fn kb_task_starter_flatten_sections(
    sections: &KbTaskStarterSections,
) -> Vec<KbTaskStarterEvidenceItem> {
    sections
        .similar_tasks
        .iter()
        .chain(sections.risks.iter())
        .chain(sections.templates.iter())
        .chain(sections.suggested_files.iter())
        .chain(sections.verify_commands.iter())
        .cloned()
        .collect()
}

fn kb_task_starter_summary(input_type: &str, req_id: &str, task_id: &str, input: &str) -> String {
    if input_type == "task" {
        format!("开工助手上下文包：{task_id}")
    } else if input_type == "req" {
        format!("开工助手上下文包：{req_id}")
    } else {
        format!("开工助手上下文包：{}", compact_text_chars(input, 32))
    }
}

fn kb_task_starter_insert_session(
    conn: &Connection,
    input: &str,
    input_type: &str,
    req_id: &str,
    task_id: &str,
    summary: &str,
) -> Result<String, String> {
    let session_id = format!(
        "starter-{}-{}",
        now_nanos(),
        fnv1a64_hex(&format!("{input}:{input_type}:{req_id}:{task_id}"))
    );
    conn.execute(
        r#"
        INSERT INTO task_starter_sessions(
          id, input_text, input_type, parsed_req_id, parsed_task_id, summary
        )
        VALUES(?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![session_id, input, input_type, req_id, task_id, summary],
    )
    .map_err(|err| err.to_string())?;
    Ok(session_id)
}

fn kb_task_starter_insert_evidence(
    conn: &Connection,
    session_id: &str,
    evidence: &[KbTaskStarterEvidenceItem],
) -> Result<(), String> {
    for item in evidence {
        conn.execute(
            r#"
            INSERT OR IGNORE INTO task_starter_evidence(
              session_id, evidence_type, source_table, source_id, title, excerpt, score, reason
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                session_id,
                item.evidence_type,
                item.source_table,
                item.source_id,
                item.title,
                item.excerpt,
                item.score,
                item.reason
            ],
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn kb_task_starter_preview_internal(
    input_text: &str,
    limit: usize,
) -> Result<KbTaskStarterPreviewResponse, String> {
    let input = input_text.trim();
    if input.is_empty() {
        return Err("empty_input".into());
    }
    let conn = connect_knowledgebase()?;
    let (input_type, req_id, task_id) = kb_task_starter_parse_input(input);
    let summary = kb_task_starter_summary(&input_type, &req_id, &task_id, input);
    let sections = kb_task_starter_collect_evidence(&conn, input, &req_id, &task_id, limit)?;
    let evidence = kb_task_starter_flatten_sections(&sections);
    let session_id =
        kb_task_starter_insert_session(&conn, input, &input_type, &req_id, &task_id, &summary)?;
    kb_task_starter_insert_evidence(&conn, &session_id, &evidence)?;
    Ok(KbTaskStarterPreviewResponse {
        session_id,
        input_type,
        parsed_req_id: req_id,
        parsed_task_id: task_id,
        summary,
        sections,
        evidence,
    })
}

fn kb_task_context_readonly_internal(
    input_text: &str,
    limit: usize,
) -> Result<KbTaskStarterPreviewResponse, String> {
    let input = input_text.trim();
    if input.is_empty() {
        return Err("empty_input".into());
    }
    let conn = connect_knowledgebase()?;
    let (input_type, req_id, task_id) = kb_task_starter_parse_input(input);
    let summary = kb_task_starter_summary(&input_type, &req_id, &task_id, input);
    let sections = kb_task_starter_collect_evidence(&conn, input, &req_id, &task_id, limit)?;
    let evidence = kb_task_starter_flatten_sections(&sections);
    Ok(KbTaskStarterPreviewResponse {
        session_id: String::new(),
        input_type,
        parsed_req_id: req_id,
        parsed_task_id: task_id,
        summary,
        sections,
        evidence,
    })
}

fn kb_task_starter_load_session(
    conn: &Connection,
    session_id: &str,
) -> Result<(String, String, String, String, String), String> {
    conn.query_row(
        r#"
        SELECT input_text, input_type, parsed_req_id, parsed_task_id, summary
        FROM task_starter_sessions
        WHERE id=?1
        LIMIT 1
        "#,
        params![session_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )
    .map_err(|err| err.to_string())
}

fn kb_task_starter_load_evidence(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<KbTaskStarterEvidenceItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT evidence_type, source_table, source_id, title, excerpt, score, reason
            FROM task_starter_evidence
            WHERE session_id=?1
            ORDER BY score DESC, created_at ASC
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(KbTaskStarterEvidenceItem {
                evidence_type: row.get::<_, String>(0)?,
                source_table: row.get::<_, String>(1)?,
                source_id: row.get::<_, String>(2)?,
                title: row.get::<_, String>(3)?,
                excerpt: row.get::<_, String>(4).unwrap_or_default(),
                score: row.get::<_, f64>(5).unwrap_or(0.0),
                reason: row.get::<_, String>(6).unwrap_or_default(),
                source_path: String::new(),
            })
        })
        .map_err(|err| err.to_string())?;
    let mut evidence = Vec::new();
    for row in rows {
        evidence.push(row.map_err(|err| err.to_string())?);
    }
    Ok(evidence)
}

fn kb_task_starter_markdown_section(title: &str, items: &[KbTaskStarterEvidenceItem]) -> String {
    if items.is_empty() {
        return format!("## {title}\n\n- 暂无命中，建议先采集项目或换一组关键词。\n");
    }
    let mut out = format!("## {title}\n\n");
    for item in items.iter().take(8) {
        let source = if item.source_id.trim().is_empty() {
            item.source_table.clone()
        } else {
            format!("{}:{}", item.source_table, item.source_id)
        };
        out.push_str(&format!(
            "- {}：{}（{}，score {:.1}）\n",
            item.title, item.reason, source, item.score
        ));
        if !item.excerpt.trim().is_empty() {
            out.push_str(&format!("  摘要：{}\n", item.excerpt));
        }
    }
    out
}

fn kb_task_starter_build_markdown(
    input_text: &str,
    input_type: &str,
    req_id: &str,
    task_id: &str,
    evidence: &[KbTaskStarterEvidenceItem],
) -> String {
    let mut sections = KbTaskStarterSections::default();
    for item in evidence {
        match item.evidence_type.as_str() {
            "similar_task" => sections.similar_tasks.push(item.clone()),
            "risk" => sections.risks.push(item.clone()),
            "template" => sections.templates.push(item.clone()),
            "file" => sections.suggested_files.push(item.clone()),
            "verify" => sections.verify_commands.push(item.clone()),
            _ => {}
        }
    }
    let target = if !task_id.trim().is_empty() {
        task_id
    } else if !req_id.trim().is_empty() {
        req_id
    } else {
        input_text
    };
    let mut out = String::new();
    out.push_str("# 任务开工上下文包\n\n");
    out.push_str("## 任务目标\n\n");
    out.push_str(&format!(
        "- 输入类型：`{input_type}`\n- 开工目标：{target}\n"
    ));
    out.push_str("\n## 边界约束\n\n");
    out.push_str("- 先检索历史、再分析和改动。\n- 仅修改任务明确范围，跨边界需要先确认。\n- 改动后必须完成编译、关键自测、治理材料和 memory 回写。\n");
    out.push('\n');
    out.push_str(&kb_task_starter_markdown_section(
        "历史相似任务",
        &sections.similar_tasks,
    ));
    out.push('\n');
    out.push_str(&kb_task_starter_markdown_section(
        "相关风险",
        &sections.risks,
    ));
    out.push('\n');
    out.push_str(&kb_task_starter_markdown_section(
        "推荐提示词模板",
        &sections.templates,
    ));
    out.push('\n');
    out.push_str(&kb_task_starter_markdown_section(
        "建议读取文件",
        &sections.suggested_files,
    ));
    out.push('\n');
    out.push_str(&kb_task_starter_markdown_section(
        "建议验证命令",
        &sections.verify_commands,
    ));
    out.push_str("\n## 证据来源\n\n");
    for item in evidence.iter().take(24) {
        out.push_str(&format!(
            "- `{}` / `{}` / `{}`：{}\n",
            item.evidence_type, item.source_table, item.source_id, item.title
        ));
    }
    out.push_str("\n## 给 AI 的执行提示词\n\n");
    out.push_str("请基于以上上下文执行任务：先说明涉及文件、SQL或接口链路、调用链与影响范围、根因或实现结论；再按边界实现、验证、沉淀，并输出未覆盖风险。\n");
    out
}

fn kb_task_starter_package_internal(
    session_id: Option<&str>,
    input_text: Option<&str>,
    limit: usize,
) -> Result<KbTaskStarterPackageResponse, String> {
    let conn = connect_knowledgebase()?;
    let (session_id, input, input_type, req_id, task_id, evidence) =
        if let Some(existing_id) = session_id.map(str::trim).filter(|item| !item.is_empty()) {
            let (input, input_type, req_id, task_id, _summary) =
                kb_task_starter_load_session(&conn, existing_id)?;
            let evidence = kb_task_starter_load_evidence(&conn, existing_id)?;
            (
                existing_id.to_string(),
                input,
                input_type,
                req_id,
                task_id,
                evidence,
            )
        } else {
            let preview = kb_task_starter_preview_internal(input_text.unwrap_or_default(), limit)?;
            (
                preview.session_id,
                input_text.unwrap_or_default().trim().to_string(),
                preview.input_type,
                preview.parsed_req_id,
                preview.parsed_task_id,
                preview.evidence,
            )
        };
    let markdown =
        kb_task_starter_build_markdown(&input, &input_type, &req_id, &task_id, &evidence);
    conn.execute(
        "UPDATE task_starter_sessions SET package_markdown=?2, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
        params![session_id, markdown],
    )
    .map_err(|err| err.to_string())?;
    Ok(KbTaskStarterPackageResponse {
        session_id,
        markdown,
    })
}

fn kb_retro_summary(input_type: &str, req_id: &str, task_id: &str, input: &str) -> String {
    if input_type == "task" {
        format!("复盘草稿：{task_id}")
    } else if input_type == "req" {
        format!("复盘草稿：{req_id}")
    } else {
        format!("复盘草稿：{}", compact_text_chars(input, 32))
    }
}

fn kb_retro_latest_starter_session(conn: &Connection, req_id: &str, task_id: &str) -> String {
    conn.query_row(
        r#"
        SELECT id
        FROM task_starter_sessions
        WHERE (?1 = '' OR parsed_req_id = ?1) AND (?2 = '' OR parsed_task_id = ?2)
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        params![req_id, task_id],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_default()
}

fn kb_retro_build_sections(sections: &KbTaskStarterSections) -> KbRetroSections {
    KbRetroSections {
        changes: sections
            .similar_tasks
            .iter()
            .chain(sections.suggested_files.iter())
            .take(8)
            .cloned()
            .collect(),
        verification: sections.verify_commands.iter().take(8).cloned().collect(),
        risks: sections.risks.iter().take(8).cloned().collect(),
        context: sections
            .templates
            .iter()
            .chain(sections.similar_tasks.iter())
            .take(8)
            .cloned()
            .collect(),
    }
}

fn kb_retro_build_suggestions(
    sections: &KbRetroSections,
    req_id: &str,
    task_id: &str,
) -> Vec<KbRetroSuggestionItem> {
    let target_id = if !task_id.trim().is_empty() {
        task_id
    } else {
        req_id
    };
    let mut suggestions = Vec::new();
    suggestions.push(KbRetroSuggestionItem {
        suggestion_id: String::new(),
        suggestion_type: "task_memory".into(),
        target_kind: "memory".into(),
        target_id: target_id.into(),
        title: "写入任务复盘记忆".into(),
        rationale: "将本轮改动、验证、风险和遗留问题沉淀到 .ai/memory/tasks。".into(),
        payload_json: serde_json::json!({ "req_id": req_id, "task_id": task_id }).to_string(),
        status: "pending".into(),
    });
    suggestions.push(KbRetroSuggestionItem {
        suggestion_id: String::new(),
        suggestion_type: "project_knowledge".into(),
        target_kind: "knowledge_unit".into(),
        target_id: target_id.into(),
        title: "沉淀项目知识结论".into(),
        rationale: "将复盘结论转为后续搜索和开工助手可召回的项目知识。".into(),
        payload_json:
            serde_json::json!({ "req_id": req_id, "task_id": task_id, "source": "retro" })
                .to_string(),
        status: "pending".into(),
    });
    suggestions.push(KbRetroSuggestionItem {
        suggestion_id: String::new(),
        suggestion_type: "prompt_template_candidate".into(),
        target_kind: "prompt_template".into(),
        target_id: target_id.into(),
        title: "评估是否沉淀为提示词模板".into(),
        rationale: "若本轮复盘形成了稳定做法，可转为候选模板，后续人工精修。".into(),
        payload_json:
            serde_json::json!({ "req_id": req_id, "task_id": task_id, "source": "retro" })
                .to_string(),
        status: "pending".into(),
    });
    if let Some(item) = sections.verification.first() {
        suggestions.push(KbRetroSuggestionItem {
            suggestion_id: String::new(),
            suggestion_type: "verify_command".into(),
            target_kind: item.source_table.clone(),
            target_id: item.source_id.clone(),
            title: "沉淀验证命令".into(),
            rationale: item.reason.clone(),
            payload_json: serde_json::json!({ "title": item.title, "excerpt": item.excerpt })
                .to_string(),
            status: "pending".into(),
        });
    }
    if let Some(item) = sections.risks.first() {
        suggestions.push(KbRetroSuggestionItem {
            suggestion_id: String::new(),
            suggestion_type: "risk_rule".into(),
            target_kind: item.source_table.clone(),
            target_id: item.source_id.clone(),
            title: "沉淀风险规则".into(),
            rationale: item.reason.clone(),
            payload_json: serde_json::json!({ "title": item.title, "excerpt": item.excerpt })
                .to_string(),
            status: "pending".into(),
        });
    }
    suggestions
}

fn kb_retro_markdown_section(title: &str, items: &[KbTaskStarterEvidenceItem]) -> String {
    if items.is_empty() {
        return format!("## {title}\n\n- 暂无命中。\n");
    }
    let mut out = format!("## {title}\n\n");
    for item in items.iter().take(8) {
        out.push_str(&format!("- {}：{}\n", item.title, item.reason));
        if !item.excerpt.trim().is_empty() {
            out.push_str(&format!("  摘要：{}\n", item.excerpt));
        }
    }
    out
}

fn kb_retro_evaluate_starter(
    conn: &Connection,
    starter_session_id: &str,
    sections: &KbRetroSections,
) -> KbRetroStarterEvaluation {
    let starter_session_id = starter_session_id.trim();
    if starter_session_id.is_empty() {
        return KbRetroStarterEvaluation {
            linked: false,
            starter_session_id: String::new(),
            score: 0,
            summary: "未找到关联开工包，无法评估开工召回质量。".into(),
            missing_info: vec!["缺少可关联的开工包会话".into()],
            optimization_items: vec![
                "下次从开工助手进入任务，或在复盘时显式传入 starter_session_id。".into(),
            ],
        };
    }
    let starter_evidence =
        kb_task_starter_load_evidence(conn, starter_session_id).unwrap_or_default();
    let retro_ids = sections
        .changes
        .iter()
        .chain(sections.verification.iter())
        .chain(sections.risks.iter())
        .chain(sections.context.iter())
        .filter_map(|item| {
            let id = item.source_id.trim();
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        })
        .collect::<HashSet<_>>();
    let overlap = starter_evidence
        .iter()
        .filter(|item| {
            !item.source_id.trim().is_empty() && retro_ids.contains(item.source_id.trim())
        })
        .count() as i64;
    let has_starter_verify = starter_evidence
        .iter()
        .any(|item| item.evidence_type == "verify");
    let has_starter_risk = starter_evidence
        .iter()
        .any(|item| item.evidence_type == "risk");
    let mut score = 45 + overlap * 12;
    if has_starter_verify {
        score += 15;
    }
    if has_starter_risk {
        score += 10;
    }
    let mut missing_info = Vec::new();
    let mut optimization_items = Vec::new();
    if !has_starter_verify && !sections.verification.is_empty() {
        missing_info.push("开工包未提前召回验证命令或测试记录".into());
        optimization_items
            .push("增强开工助手对 testing 文档、验证命令和 smoke 记录的召回权重。".into());
    }
    if !has_starter_risk && !sections.risks.is_empty() {
        missing_info.push("开工包未提前召回风险或遗留问题".into());
        optimization_items.push("将复盘中的风险规则沉淀为后续开工助手的风险证据。".into());
    }
    if overlap == 0 && !retro_ids.is_empty() {
        missing_info.push("复盘证据与开工包证据重合度较低".into());
        optimization_items.push("将本轮关键证据补入任务记忆，提升下次 REQ/TASK 精准召回。".into());
    }
    if missing_info.is_empty() {
        missing_info.push("未发现明显遗漏，开工包与复盘证据存在有效衔接。".into());
    }
    if optimization_items.is_empty() {
        optimization_items.push("保持当前开工包召回策略，复盘后补充最新验证结论。".into());
    }
    let score = score.clamp(0, 100);
    KbRetroStarterEvaluation {
        linked: true,
        starter_session_id: starter_session_id.into(),
        score,
        summary: format!(
            "关联开工包 {starter_session_id}；证据重合 {overlap} 条，召回质量评分 {score}/100。"
        ),
        missing_info,
        optimization_items,
    }
}

fn kb_retro_build_markdown(
    input_text: &str,
    input_type: &str,
    req_id: &str,
    task_id: &str,
    sections: &KbRetroSections,
    suggestions: &[KbRetroSuggestionItem],
    starter_evaluation: &KbRetroStarterEvaluation,
) -> String {
    let target = if !task_id.trim().is_empty() {
        task_id
    } else if !req_id.trim().is_empty() {
        req_id
    } else {
        input_text
    };
    let mut out = String::new();
    out.push_str("# 任务复盘草稿\n\n");
    out.push_str(&format!(
        "- 输入类型：`{input_type}`\n- 复盘对象：{target}\n"
    ));
    out.push_str("\n## 本轮结论\n\n- 请根据最终提交、验证结果和遗留风险补齐一句话结论。\n\n");
    out.push_str(&kb_retro_markdown_section(
        "关键改动与上下文",
        &sections.changes,
    ));
    out.push('\n');
    out.push_str(&kb_retro_markdown_section(
        "验证证据",
        &sections.verification,
    ));
    out.push('\n');
    out.push_str(&kb_retro_markdown_section(
        "风险与遗留问题",
        &sections.risks,
    ));
    out.push('\n');
    out.push_str(&kb_retro_markdown_section("关联上下文", &sections.context));
    out.push_str("\n## 沉淀建议\n\n");
    for item in suggestions {
        out.push_str(&format!(
            "- `{}`：{}。{}\n",
            item.suggestion_type, item.title, item.rationale
        ));
    }
    out.push_str("\n## 开工包关联评估\n\n");
    out.push_str(&format!("- {}\n", starter_evaluation.summary));
    out.push_str("- 遗漏信息：\n");
    for item in &starter_evaluation.missing_info {
        out.push_str(&format!("  - {item}\n"));
    }
    out.push_str("- 优化建议：\n");
    for item in &starter_evaluation.optimization_items {
        out.push_str(&format!("  - {item}\n"));
    }
    out
}

fn kb_retro_insert_session(
    conn: &Connection,
    input: &str,
    input_type: &str,
    req_id: &str,
    task_id: &str,
    starter_session_id: &str,
    summary: &str,
    draft_markdown: &str,
) -> Result<String, String> {
    let session_id = format!(
        "retro-{}-{}",
        now_nanos(),
        fnv1a64_hex(&format!("{input}:{req_id}:{task_id}"))
    );
    conn.execute(
        r#"
        INSERT INTO retrospective_sessions(
          id, input_text, input_type, parsed_req_id, parsed_task_id,
          related_starter_session_id, source_summary, draft_markdown
        )
        VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            session_id,
            input,
            input_type,
            req_id,
            task_id,
            starter_session_id,
            summary,
            draft_markdown
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(session_id)
}

fn kb_retro_insert_suggestions(
    conn: &Connection,
    session_id: &str,
    suggestions: &mut [KbRetroSuggestionItem],
) -> Result<(), String> {
    for item in suggestions {
        let id = format!(
            "retro-sug-{}-{}",
            now_nanos(),
            fnv1a64_hex(&format!(
                "{session_id}:{}:{}",
                item.suggestion_type, item.title
            ))
        );
        item.suggestion_id = id.clone();
        conn.execute(
            r#"
            INSERT INTO retrospective_suggestions(
              id, session_id, suggestion_type, target_kind, target_id, title, rationale, payload_json, status
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![id, session_id, item.suggestion_type, item.target_kind, item.target_id, item.title, item.rationale, item.payload_json, item.status],
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn kb_retro_preview_internal(
    input_text: &str,
    starter_session_id: Option<&str>,
    limit: usize,
) -> Result<KbRetroPreviewResponse, String> {
    let input = input_text.trim();
    if input.is_empty() {
        return Err("empty_input".into());
    }
    let conn = connect_knowledgebase()?;
    let (input_type, req_id, task_id) = kb_task_starter_parse_input(input);
    let related_starter_session_id = starter_session_id
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| kb_retro_latest_starter_session(&conn, &req_id, &task_id));
    let summary = kb_retro_summary(&input_type, &req_id, &task_id, input);
    let starter_sections =
        kb_task_starter_collect_evidence(&conn, input, &req_id, &task_id, limit)?;
    let sections = kb_retro_build_sections(&starter_sections);
    let mut suggestions = kb_retro_build_suggestions(&sections, &req_id, &task_id);
    let starter_evaluation =
        kb_retro_evaluate_starter(&conn, &related_starter_session_id, &sections);
    let draft_markdown = kb_retro_build_markdown(
        input,
        &input_type,
        &req_id,
        &task_id,
        &sections,
        &suggestions,
        &starter_evaluation,
    );
    let session_id = kb_retro_insert_session(
        &conn,
        input,
        &input_type,
        &req_id,
        &task_id,
        &related_starter_session_id,
        &summary,
        &draft_markdown,
    )?;
    kb_retro_insert_suggestions(&conn, &session_id, &mut suggestions)?;
    Ok(KbRetroPreviewResponse {
        session_id,
        input_type,
        parsed_req_id: req_id,
        parsed_task_id: task_id,
        related_starter_session_id,
        summary,
        draft_markdown,
        sections,
        suggestions,
        starter_evaluation,
    })
}

fn kb_retro_load_suggestions(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<KbRetroSuggestionItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, suggestion_type, target_kind, target_id, title, rationale, payload_json, status
            FROM retrospective_suggestions
            WHERE session_id=?1
            ORDER BY created_at ASC
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(KbRetroSuggestionItem {
                suggestion_id: row.get::<_, String>(0)?,
                suggestion_type: row.get::<_, String>(1)?,
                target_kind: row.get::<_, String>(2).unwrap_or_default(),
                target_id: row.get::<_, String>(3).unwrap_or_default(),
                title: row.get::<_, String>(4).unwrap_or_default(),
                rationale: row.get::<_, String>(5).unwrap_or_default(),
                payload_json: row.get::<_, String>(6).unwrap_or_default(),
                status: row.get::<_, String>(7).unwrap_or_default(),
            })
        })
        .map_err(|err| err.to_string())?;
    let mut suggestions = Vec::new();
    for row in rows {
        suggestions.push(row.map_err(|err| err.to_string())?);
    }
    Ok(suggestions)
}

fn kb_retro_package_internal(
    session_id: Option<&str>,
    input_text: Option<&str>,
    starter_session_id: Option<&str>,
    limit: usize,
) -> Result<KbRetroPackageResponse, String> {
    let conn = connect_knowledgebase()?;
    if let Some(existing_id) = session_id.map(str::trim).filter(|item| !item.is_empty()) {
        let (markdown, starter_session_id) = conn
            .query_row(
                "SELECT draft_markdown, related_starter_session_id FROM retrospective_sessions WHERE id=?1 LIMIT 1",
                params![existing_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1).unwrap_or_default())),
            )
            .map_err(|err| err.to_string())?;
        let suggestions = kb_retro_load_suggestions(&conn, existing_id)?;
        let starter_evaluation =
            kb_retro_evaluate_starter(&conn, &starter_session_id, &KbRetroSections::default());
        return Ok(KbRetroPackageResponse {
            session_id: existing_id.into(),
            markdown,
            suggestions,
            starter_evaluation,
        });
    }
    let preview =
        kb_retro_preview_internal(input_text.unwrap_or_default(), starter_session_id, limit)?;
    Ok(KbRetroPackageResponse {
        session_id: preview.session_id,
        markdown: preview.draft_markdown,
        suggestions: preview.suggestions,
        starter_evaluation: preview.starter_evaluation,
    })
}

fn kb_retro_suggestions_internal(session_id: &str) -> Result<KbRetroSuggestionsResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("missing_session_id".into());
    }
    let conn = connect_knowledgebase()?;
    let suggestions = kb_retro_load_suggestions(&conn, session_id)?;
    Ok(KbRetroSuggestionsResponse { suggestions })
}

fn kb_retro_approve_suggestion_internal(suggestion_id: &str) -> Result<serde_json::Value, String> {
    let suggestion_id = suggestion_id.trim();
    if suggestion_id.is_empty() {
        return Err("missing_suggestion_id".into());
    }
    let conn = connect_knowledgebase()?;
    let changed = conn
        .execute(
            r#"
            UPDATE retrospective_suggestions
            SET status='approved', approved_at=CURRENT_TIMESTAMP, updated_at=CURRENT_TIMESTAMP
            WHERE id=?1
            "#,
            params![suggestion_id],
        )
        .map_err(|err| err.to_string())?;
    if changed == 0 {
        return Err("suggestion_not_found".into());
    }
    Ok(serde_json::json!({
        "ok": true,
        "suggestion_id": suggestion_id,
        "status": "approved"
    }))
}

fn kb_task_starter_session_summary_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<KbTaskStarterSessionSummary> {
    let package_markdown = row.get::<_, String>(6).unwrap_or_default();
    Ok(KbTaskStarterSessionSummary {
        session_id: row.get::<_, String>(0)?,
        input_text: row.get::<_, String>(1)?,
        input_type: row.get::<_, String>(2)?,
        parsed_req_id: row.get::<_, String>(3).unwrap_or_default(),
        parsed_task_id: row.get::<_, String>(4).unwrap_or_default(),
        summary: row.get::<_, String>(5).unwrap_or_default(),
        has_package: !package_markdown.trim().is_empty(),
        evidence_count: row.get::<_, i64>(7).unwrap_or(0),
        created_at: row.get::<_, String>(8).unwrap_or_default(),
        updated_at: row.get::<_, String>(9).unwrap_or_default(),
    })
}

fn kb_task_starter_sessions_internal(
    limit: usize,
) -> Result<KbTaskStarterSessionsResponse, String> {
    let conn = connect_knowledgebase()?;
    let limit = limit.clamp(5, 50);
    let mut stmt = conn
        .prepare(
            r#"
            SELECT s.id, s.input_text, s.input_type, s.parsed_req_id, s.parsed_task_id,
                   s.summary, s.package_markdown, COUNT(e.source_id) AS evidence_count,
                   COALESCE(s.created_at, ''), COALESCE(s.updated_at, '')
            FROM task_starter_sessions s
            LEFT JOIN task_starter_evidence e ON e.session_id = s.id
            GROUP BY s.id
            ORDER BY s.updated_at DESC, s.created_at DESC
            LIMIT ?1
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(
            params![limit as i64],
            kb_task_starter_session_summary_from_row,
        )
        .map_err(|err| err.to_string())?;
    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row.map_err(|err| err.to_string())?);
    }
    Ok(KbTaskStarterSessionsResponse { sessions })
}

fn kb_task_starter_session_detail_internal(
    session_id: &str,
) -> Result<KbTaskStarterSessionDetailResponse, String> {
    let conn = connect_knowledgebase()?;
    let session = conn
        .query_row(
            r#"
            SELECT s.id, s.input_text, s.input_type, s.parsed_req_id, s.parsed_task_id,
                   s.summary, s.package_markdown, COUNT(e.source_id) AS evidence_count,
                   COALESCE(s.created_at, ''), COALESCE(s.updated_at, '')
            FROM task_starter_sessions s
            LEFT JOIN task_starter_evidence e ON e.session_id = s.id
            WHERE s.id=?1
            GROUP BY s.id
            LIMIT 1
            "#,
            params![session_id],
            kb_task_starter_session_summary_from_row,
        )
        .map_err(|err| err.to_string())?;
    let evidence = kb_task_starter_load_evidence(&conn, session_id)?;
    let markdown = conn
        .query_row(
            "SELECT package_markdown FROM task_starter_sessions WHERE id=?1 LIMIT 1",
            params![session_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default();
    Ok(KbTaskStarterSessionDetailResponse {
        session,
        evidence,
        markdown,
    })
}

fn kb_knowledge_units_internal() -> Result<KbKnowledgeUnitsResponse, String> {
    let conn = connect_knowledgebase()?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, unit_type, title, summary, category, source_item_id, template_id, weight, status
            FROM knowledge_units
            WHERE status='active'
            ORDER BY weight DESC, updated_at DESC
            LIMIT 36
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(KbKnowledgeUnit {
                id: row.get::<_, String>(0)?,
                unit_type: row.get::<_, String>(1)?,
                title: row.get::<_, String>(2)?,
                summary: row.get::<_, String>(3).unwrap_or_default(),
                category: row.get::<_, String>(4).unwrap_or_default(),
                source_item_id: row.get::<_, String>(5).unwrap_or_default(),
                template_id: row.get::<_, String>(6).unwrap_or_default(),
                weight: row.get::<_, f64>(7).unwrap_or(1.0),
                status: row.get::<_, String>(8).unwrap_or_else(|_| "active".into()),
            })
        })
        .map_err(|err| err.to_string())?;
    let mut units = Vec::new();
    for row in rows {
        units.push(row.map_err(|err| err.to_string())?);
    }
    let hub_id = units
        .iter()
        .find(|unit| unit.id == "ku-prompt-engineering")
        .map(|unit| unit.id.clone());
    let mut links = Vec::new();
    if let Some(hub) = hub_id {
        for unit in &units {
            if unit.id != hub {
                let relation_type = if unit.unit_type == "template" {
                    "contains_template"
                } else if unit.unit_type == "evidence" {
                    "supports"
                } else {
                    "relates_to"
                };
                links.push(KbKnowledgeUnitLink {
                    id: format!("link-{}-{}", hub, unit.id),
                    from_id: hub.clone(),
                    to_id: unit.id.clone(),
                    relation_type: relation_type.into(),
                    summary: if unit.template_id.trim().is_empty() {
                        format!(
                            "{} 与提示词工程同属 {} 主题，可继续补充模板或证据。",
                            unit.title, unit.category
                        )
                    } else {
                        format!(
                            "{} 已绑定模板，可从图谱跳到提示词工程查看和复制。",
                            unit.title
                        )
                    },
                    evidence_ref: if unit.source_item_id.trim().is_empty() {
                        "knowledge_units".into()
                    } else {
                        unit.source_item_id.clone()
                    },
                    template_id: unit.template_id.clone(),
                    weight: unit.weight,
                });
            }
        }
    }
    Ok(KbKnowledgeUnitsResponse { units, links })
}

fn rebuild_knowledgebase_fts(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM items_fts", [])
        .map_err(|err| err.to_string())?;
    conn.execute(
        "INSERT INTO items_fts(item_id, title, content_text, source_path) SELECT item_id, title, content_text, source_path FROM items",
        [],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn kb_compact_conversations_internal() -> Result<serde_json::Value, String> {
    let conn = connect_knowledgebase()?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT item_id, project_id, title, content_text, source_path, source_tool, session_id, updated_at
            FROM items
            WHERE item_type='conversation'
            ORDER BY source_path, session_id, updated_at DESC
            "#,
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3).unwrap_or_default(),
                row.get::<_, String>(4).unwrap_or_default(),
                row.get::<_, String>(5).unwrap_or_default(),
                row.get::<_, String>(6).unwrap_or_default(),
                row.get::<_, String>(7).unwrap_or_default(),
            ))
        })
        .map_err(|err| err.to_string())?;

    let mut groups: HashMap<
        (String, String, String),
        Vec<(String, String, String, String, String)>,
    > = HashMap::new();
    for row in rows {
        let (item_id, project_id, title, content, source_path, source_tool, session_id, updated_at) =
            row.map_err(|err| err.to_string())?;
        let key = (project_id, source_path, session_id);
        groups
            .entry(key)
            .or_default()
            .push((item_id, title, content, source_tool, updated_at));
    }

    let mut compacted = 0_i64;
    let mut removed = 0_i64;
    for ((project_id, source_path, session_id), entries) in groups {
        if entries.is_empty() {
            continue;
        }
        let mut seen = HashSet::new();
        let mut blocks = Vec::new();
        let mut source_tool = String::new();
        let mut title = String::new();
        if Path::new(&source_path).is_file() {
            if let Ok(raw_content) = fs::read_to_string(&source_path) {
                if let Some(extracted) = if source_path.ends_with(".jsonl") {
                    extract_messages_from_jsonl(&raw_content)
                } else if source_path.ends_with(".json") {
                    extract_messages_from_json(&raw_content)
                } else {
                    None
                } {
                    for block in extracted.split("\n\n") {
                        let normalized = block.trim();
                        if normalized.is_empty() || conversation_text_noise_score(normalized) >= 5 {
                            continue;
                        }
                        let key = fnv1a64_hex(&normalize_conversation_block(normalized));
                        if seen.insert(key) {
                            blocks.push(normalized.to_string());
                        }
                        if blocks.len() >= 80 {
                            break;
                        }
                    }
                }
            }
        }
        for (_, entry_title, content, entry_tool, _) in &entries {
            if title.is_empty() {
                title = entry_title.clone();
            }
            if source_tool.is_empty() {
                source_tool = entry_tool.clone();
            }
            if !blocks.is_empty() {
                continue;
            }
            for block in content.split("\n\n") {
                let normalized = block.trim();
                if normalized.is_empty() || conversation_text_noise_score(normalized) >= 5 {
                    continue;
                }
                let key = fnv1a64_hex(&normalize_conversation_block(normalized));
                if seen.insert(key) {
                    blocks.push(normalized.to_string());
                }
                if blocks.len() >= 80 {
                    break;
                }
            }
            if blocks.len() >= 80 {
                break;
            }
        }
        let merged = blocks.join("\n\n");
        if merged.trim().is_empty() {
            for (item_id, _, _, _, _) in entries {
                conn.execute("DELETE FROM items WHERE item_id=?1", params![item_id])
                    .map_err(|err| err.to_string())?;
                removed += 1;
            }
            continue;
        }
        let canonical_meta = KbItemMeta {
            source_type: "conversation".into(),
            source_tool: if source_tool.trim().is_empty() {
                "unknown".into()
            } else {
                source_tool
            },
            session_id: session_id.clone(),
            speaker: String::new(),
            verified: 0,
            tags: "conversation,auto,compacted".into(),
        };
        let canonical_id = kb_upsert_item_with_meta(
            &conn,
            &project_id,
            "conversation",
            if title.trim().is_empty() {
                "会话记录"
            } else {
                &title
            },
            &merged,
            &source_path,
            &canonical_meta,
        )?;
        kb_set_item_updated_at(
            &conn,
            &canonical_id,
            &source_file_time_text(Path::new(&source_path)),
        )?;
        compacted += 1;
        let mut removed_canonical = false;
        if conversation_text_noise_score(&merged) >= 5 {
            conn.execute("DELETE FROM items WHERE item_id=?1", params![canonical_id])
                .map_err(|err| err.to_string())?;
            removed += 1;
            removed_canonical = true;
        }
        for (item_id, _, _, _, _) in entries {
            if item_id != canonical_id || removed_canonical {
                conn.execute("DELETE FROM items WHERE item_id=?1", params![item_id])
                    .map_err(|err| err.to_string())?;
                removed += 1;
            }
        }
    }
    rebuild_knowledgebase_fts(&conn)?;
    Ok(serde_json::json!({
        "compacted": compacted,
        "removed": removed
    }))
}

fn kb_register_project_internal(path: &str, name: Option<String>) -> Result<String, String> {
    let root = PathBuf::from(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    let display_name = name.unwrap_or_else(|| {
        root.file_name()
            .map(|value| value.to_string_lossy().to_string())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "project".to_string())
    });
    let conn = connect_knowledgebase()?;
    kb_upsert_project(&conn, &display_name, &root.to_string_lossy())
}

fn kb_ingest_inbox_internal(path: &str) -> Result<(i64, i64), String> {
    let root = PathBuf::from(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    let project_path = root.to_string_lossy().to_string();
    let project_name = root
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    let conn = connect_knowledgebase()?;
    ingest_inbox_for_project(&conn, &project_path, &project_name)
}

fn kb_push_event_internal(
    path: &str,
    event: &serde_json::Value,
    process_now: bool,
) -> Result<(), String> {
    let root = PathBuf::from(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    let project_path = root.to_string_lossy().to_string();
    let project_name = root
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    let conn = connect_knowledgebase()?;
    let project_id = kb_upsert_project(&conn, &project_name, &project_path)?;
    let _ = kb_process_event_payload(&conn, &project_id, "workflow-statusbar/manual", event)?;
    if process_now {
        let _ = ingest_inbox_for_project(&conn, &project_path, &project_name)?;
    }
    Ok(())
}

fn knowledgebase_web_url() -> String {
    env::var("WORKFLOW_STATUSBAR_KB_WEB_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| KNOWLEDGEBASE_DEFAULT_WEB_URL.to_string())
}

fn knowledgebase_bind_addr() -> String {
    if let Ok(value) = env::var("WORKFLOW_STATUSBAR_KB_BIND") {
        let bind = value.trim().to_string();
        if !bind.is_empty() {
            return bind;
        }
    }
    let url = knowledgebase_web_url();
    if let Some(rest) = url.strip_prefix("http://") {
        let host_port = rest.split('/').next().unwrap_or_default().trim();
        if !host_port.is_empty() {
            return host_port.to_string();
        }
    }
    KNOWLEDGEBASE_DEFAULT_BIND_ADDR.to_string()
}

fn http_header(name: &[u8], value: &[u8]) -> Option<Header> {
    Header::from_bytes(name, value).ok()
}

fn http_respond_json(request: Request, status_code: u16, body: String) {
    let mut response = Response::from_string(body).with_status_code(StatusCode(status_code));
    if let Some(header) = http_header(b"Content-Type", b"application/json; charset=utf-8") {
        response.add_header(header);
    }
    if let Some(header) = http_header(b"Cache-Control", b"no-store") {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

fn http_respond_html(request: Request, status_code: u16, body: String) {
    let mut response = Response::from_string(body).with_status_code(StatusCode(status_code));
    if let Some(header) = http_header(b"Content-Type", b"text/html; charset=utf-8") {
        response.add_header(header);
    }
    if let Some(header) = http_header(b"Cache-Control", b"no-store") {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

fn url_decode(value: &str) -> String {
    let normalized = value.replace('+', " ");
    urlencoding::decode(&normalized)
        .map(|item| item.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn query_param(url_query: &str, key: &str) -> Option<String> {
    for pair in url_query.split('&') {
        let (raw_key, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
        if raw_key == key {
            return Some(url_decode(raw_value));
        }
    }
    None
}

#[derive(Clone)]
struct KbApiCallContext {
    client_id: String,
    client_name: String,
    tool_name: String,
    method: String,
    path: String,
    params_summary: String,
    remote_addr: String,
    user_agent: String,
    started_at: Instant,
}

#[derive(Serialize)]
struct KbApiCallLog {
    id: String,
    client_id: String,
    client_name: String,
    tool_name: String,
    method: String,
    path: String,
    params_summary: String,
    duration_ms: i64,
    status_code: i64,
    error_message: String,
    remote_addr: String,
    user_agent: String,
    created_at: String,
}

#[derive(Serialize)]
struct KbApiCallLogsResponse {
    readonly: bool,
    logs: Vec<KbApiCallLog>,
}

fn truncate_for_log(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for ch in value.chars().take(max_chars) {
        output.push(ch);
    }
    output
}

fn request_header_value(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.to_string().eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn api_tool_name_for_path(path: &str) -> String {
    if path == "/api/v1/search" {
        "search_memory".into()
    } else if path == "/api/v1/templates" {
        "get_prompt_template".into()
    } else if path == "/api/v1/task-context" {
        "build_task_context".into()
    } else if path.starts_with("/api/v1/evidence/") {
        "get_evidence_trace".into()
    } else if path == "/api/v1/health" {
        "list_asset_health".into()
    } else if path == "/api/v1/call-logs" {
        "list_api_call_logs".into()
    } else {
        "unknown".into()
    }
}

fn summarize_query_params(query: &str) -> String {
    if query.trim().is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    for pair in query.split('&').take(6) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        parts.push(format!(
            "{}={}",
            key,
            truncate_for_log(&url_decode(value), 80)
        ));
    }
    truncate_for_log(&parts.join("&"), 240)
}

fn kb_api_call_context(request: &Request, path: &str, query: &str) -> KbApiCallContext {
    let client_name =
        request_header_value(request, "x-kb-client").unwrap_or_else(|| "direct-api".to_string());
    let client_id = request_header_value(request, "x-kb-client-id")
        .unwrap_or_else(|| fnv1a64_hex(&client_name));
    let tool_name =
        request_header_value(request, "x-kb-tool").unwrap_or_else(|| api_tool_name_for_path(path));
    let params_summary = request_header_value(request, "x-kb-params")
        .unwrap_or_else(|| summarize_query_params(query));
    KbApiCallContext {
        client_id: truncate_for_log(&client_id, 80),
        client_name: truncate_for_log(&client_name, 120),
        tool_name: truncate_for_log(&tool_name, 120),
        method: request.method().as_str().to_string(),
        path: truncate_for_log(path, 200),
        params_summary: truncate_for_log(&params_summary, 300),
        remote_addr: request
            .remote_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_default(),
        user_agent: request_header_value(request, "user-agent").unwrap_or_default(),
        started_at: Instant::now(),
    }
}

fn kb_record_api_call_log(
    context: &KbApiCallContext,
    status_code: u16,
    error_message: Option<&str>,
) -> Result<(), String> {
    let conn = connect_knowledgebase()?;
    conn.execute(
        "INSERT OR IGNORE INTO api_clients(id, name, client_type, last_seen_at, updated_at)
         VALUES(?1, ?2, ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        params![context.client_id, context.client_name, "api"],
    )
    .map_err(|err| err.to_string())?;
    conn.execute(
        "UPDATE api_clients SET name=?2, last_seen_at=CURRENT_TIMESTAMP, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
        params![context.client_id, context.client_name],
    )
    .map_err(|err| err.to_string())?;
    let duration_ms = context
        .started_at
        .elapsed()
        .as_millis()
        .min(i64::MAX as u128) as i64;
    conn.execute(
        "INSERT INTO api_call_logs(
          id, client_id, client_name, tool_name, method, path, params_summary,
          duration_ms, status_code, error_message, remote_addr, user_agent
        ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            format!("api-log-{}", now_nanos()),
            context.client_id,
            context.client_name,
            context.tool_name,
            context.method,
            context.path,
            context.params_summary,
            duration_ms,
            status_code as i64,
            error_message.unwrap_or_default(),
            context.remote_addr,
            context.user_agent,
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn http_respond_v1_json(
    request: Request,
    context: Option<&KbApiCallContext>,
    status_code: u16,
    body: String,
    error_message: Option<&str>,
) {
    if let Some(context) = context {
        let _ = kb_record_api_call_log(context, status_code, error_message);
    }
    http_respond_json(request, status_code, body);
}

fn kb_api_call_logs_internal(limit: usize) -> Result<KbApiCallLogsResponse, String> {
    let conn = connect_knowledgebase()?;
    let limit = limit.clamp(1, 200) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT id, client_id, client_name, tool_name, method, path, params_summary,
                    duration_ms, status_code, error_message, remote_addr, user_agent, created_at
             FROM api_call_logs
             ORDER BY created_at DESC
             LIMIT ?1",
        )
        .map_err(|err| err.to_string())?;
    let logs = stmt
        .query_map(params![limit], |row| {
            Ok(KbApiCallLog {
                id: row.get(0)?,
                client_id: row.get(1)?,
                client_name: row.get(2)?,
                tool_name: row.get(3)?,
                method: row.get(4)?,
                path: row.get(5)?,
                params_summary: row.get(6)?,
                duration_ms: row.get(7)?,
                status_code: row.get(8)?,
                error_message: row.get(9)?,
                remote_addr: row.get(10)?,
                user_agent: row.get(11)?,
                created_at: row.get(12)?,
            })
        })
        .map_err(|err| err.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    Ok(KbApiCallLogsResponse {
        readonly: true,
        logs,
    })
}

fn read_task_starter_request(request: &mut Request) -> KbTaskStarterRequest {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_ok() {
        serde_json::from_str::<KbTaskStarterRequest>(&body).unwrap_or_default()
    } else {
        KbTaskStarterRequest::default()
    }
}

fn read_retro_request(request: &mut Request) -> KbRetroRequest {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_ok() {
        serde_json::from_str::<KbRetroRequest>(&body).unwrap_or_default()
    } else {
        KbRetroRequest::default()
    }
}

fn read_workflow_pack_export_request(request: &mut Request) -> KbWorkflowPackExportRequest {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_ok() {
        serde_json::from_str::<KbWorkflowPackExportRequest>(&body).unwrap_or_default()
    } else {
        KbWorkflowPackExportRequest::default()
    }
}

fn read_workflow_pack_validate_request(request: &mut Request) -> KbWorkflowPackValidateRequest {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_ok() {
        serde_json::from_str::<KbWorkflowPackValidateRequest>(&body).unwrap_or_default()
    } else {
        KbWorkflowPackValidateRequest::default()
    }
}

fn read_workflow_pack_import_request(request: &mut Request) -> KbWorkflowPackImportRequest {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_ok() {
        serde_json::from_str::<KbWorkflowPackImportRequest>(&body).unwrap_or_default()
    } else {
        KbWorkflowPackImportRequest::default()
    }
}

fn handle_knowledgebase_http_request(mut request: Request) {
    if request.method() != &Method::Get && request.method() != &Method::Post {
        http_respond_json(
            request,
            405,
            serde_json::json!({ "error": "method_not_allowed" }).to_string(),
        );
        return;
    }

    let raw_url = request.url().to_string();
    let (path, query) = raw_url
        .split_once('?')
        .map(|(left, right)| (left, right))
        .unwrap_or((raw_url.as_str(), ""));
    let api_call_context = if path.starts_with("/api/v1/") {
        Some(kb_api_call_context(&request, path, query))
    } else {
        None
    };

    match path {
        "/" | "/index.html" => {
            http_respond_html(request, 200, KNOWLEDGEBASE_WEB_HTML.to_string());
        }
        "/api/stats" => match kb_get_stats_internal() {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/search" => {
            let query_text = query_param(query, "q").unwrap_or_default();
            match kb_search_internal(&query_text) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/projects/overview" => match kb_projects_overview_internal() {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/projects/snapshots" => match kb_project_health_snapshots_internal() {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/workflow-packs/schema" => {
            if request.method() != &Method::Get {
                http_respond_json(
                    request,
                    405,
                    serde_json::json!({ "error": "method_not_allowed" }).to_string(),
                );
                return;
            }
            let data = kb_workflow_pack_schema_internal();
            http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            );
        }
        "/api/workflow-packs/export" => {
            if request.method() != &Method::Post {
                http_respond_json(
                    request,
                    405,
                    serde_json::json!({ "error": "method_not_allowed" }).to_string(),
                );
                return;
            }
            let mut payload = read_workflow_pack_export_request(&mut request);
            if payload
                .pack_type
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.pack_type = query_param(query, "pack_type");
            }
            if payload
                .input_text
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.input_text =
                    query_param(query, "input_text").or_else(|| query_param(query, "q"));
            }
            if payload
                .req_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.req_id = query_param(query, "req_id");
            }
            if payload
                .task_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.task_id = query_param(query, "task_id");
            }
            if payload
                .project_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.project_id = query_param(query, "project_id");
            }
            if payload.limit.is_none() {
                payload.limit =
                    query_param(query, "limit").and_then(|value| value.parse::<usize>().ok());
            }
            match kb_workflow_pack_export_internal(payload) {
                Ok(mut data) => {
                    let checksum_package_json =
                        serde_json::from_str::<serde_json::Value>(&data.package_json.to_string())
                            .unwrap_or_else(|_| data.package_json.clone());
                    let checksum = kb_workflow_pack_calculated_checksum(&checksum_package_json);
                    data.checksum = checksum.clone();
                    data.package_json["checksum"] = serde_json::json!(checksum);
                    data.markdown = data
                        .package_json
                        .get("markdown")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .into();
                    http_respond_json(
                        request,
                        200,
                        serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                    )
                }
                Err(err) => http_respond_json(
                    request,
                    if err == "empty_input"
                        || err == "empty_project_id"
                        || err == "unsupported_pack_type"
                    {
                        400
                    } else if err == "project_not_found" {
                        404
                    } else {
                        500
                    },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/workflow-packs/validate" => {
            if request.method() != &Method::Post {
                http_respond_json(
                    request,
                    405,
                    serde_json::json!({ "error": "method_not_allowed" }).to_string(),
                );
                return;
            }
            let mut payload = read_workflow_pack_validate_request(&mut request);
            if payload
                .pack_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.pack_id = query_param(query, "pack_id");
            }
            match kb_workflow_pack_validate_internal(payload) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "empty_package" {
                        400
                    } else if err == "pack_not_found" {
                        404
                    } else {
                        500
                    },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/workflow-packs/import" => {
            if request.method() != &Method::Post {
                http_respond_json(
                    request,
                    405,
                    serde_json::json!({ "error": "method_not_allowed" }).to_string(),
                );
                return;
            }
            let payload = read_workflow_pack_import_request(&mut request);
            match kb_workflow_pack_import_internal(payload) {
                Ok(data) => http_respond_json(
                    request,
                    if data.imported { 200 } else { 422 },
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "empty_package" { 400 } else { 500 },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/workflow-packs/") => {
            if request.method() != &Method::Get {
                http_respond_json(
                    request,
                    405,
                    serde_json::json!({ "error": "method_not_allowed" }).to_string(),
                );
                return;
            }
            let pack_id = url_decode(path.trim_start_matches("/api/workflow-packs/"));
            match kb_workflow_pack_detail_internal(&pack_id) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "pack_not_found" { 404 } else { 500 },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/projects/") && path.ends_with("/health") => {
            let project_id = url_decode(
                path.trim_start_matches("/api/projects/")
                    .trim_end_matches("/health"),
            );
            match kb_project_health_detail_internal(&project_id) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "project_not_found" { 404 } else { 500 },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/projects/") && path.ends_with("/actions") => {
            let project_id = url_decode(
                path.trim_start_matches("/api/projects/")
                    .trim_end_matches("/actions"),
            );
            match kb_project_actions_internal(&project_id) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "project_not_found" { 404 } else { 500 },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/projects" => match kb_list_projects_internal() {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "[]".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/collect" => {
            let path = query_param(query, "path").unwrap_or_default();
            if path.trim().is_empty() {
                http_respond_json(
                    request,
                    400,
                    serde_json::json!({ "error": "missing_path" }).to_string(),
                );
                return;
            }
            match kb_collect_project_internal(path.trim()) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/compact-conversations" => match kb_compact_conversations_internal() {
            Ok(data) => http_respond_json(request, 200, data.to_string()),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/prompt-templates" => {
            let status = query_param(query, "status");
            match kb_prompt_templates_internal(status.as_deref()) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/prompt-candidates" => match kb_prompt_templates_internal(Some("candidate")) {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/prompt-review" => match kb_prompt_review_internal() {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/knowledge-units" => match kb_knowledge_units_internal() {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/v1/search" => {
            if request.method() != &Method::Get {
                http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    403,
                    serde_json::json!({ "error": "write_protected", "message": "This V1 endpoint is read-only and only accepts GET." }).to_string(),
                    Some("write_protected"),
                );
                return;
            }
            let query_text = query_param(query, "q").unwrap_or_default();
            match kb_search_internal(&query_text) {
                Ok(data) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    200,
                    serde_json::to_string(&serde_json::json!({ "readonly": true, "data": data }))
                        .unwrap_or_else(|_| "{}".to_string()),
                    None,
                ),
                Err(err) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                    Some(&err),
                ),
            }
        }
        "/api/v1/templates" => {
            if request.method() != &Method::Get {
                http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    403,
                    serde_json::json!({ "error": "write_protected", "message": "This V1 endpoint is read-only and only accepts GET." }).to_string(),
                    Some("write_protected"),
                );
                return;
            }
            let status = query_param(query, "status");
            match kb_prompt_templates_internal(status.as_deref()) {
                Ok(data) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    200,
                    serde_json::to_string(&serde_json::json!({ "readonly": true, "data": data }))
                        .unwrap_or_else(|_| "{}".to_string()),
                    None,
                ),
                Err(err) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                    Some(&err),
                ),
            }
        }
        "/api/v1/task-context" => {
            let mut payload = if request.method() == &Method::Post {
                read_task_starter_request(&mut request)
            } else {
                KbTaskStarterRequest::default()
            };
            if payload
                .input_text
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.input_text =
                    query_param(query, "input_text").or_else(|| query_param(query, "q"));
            }
            let limit = payload
                .limit
                .or_else(|| {
                    query_param(query, "limit").and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(8);
            match kb_task_context_readonly_internal(
                payload.input_text.as_deref().unwrap_or_default(),
                limit,
            ) {
                Ok(data) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    200,
                    serde_json::to_string(&serde_json::json!({ "readonly": true, "data": data }))
                        .unwrap_or_else(|_| "{}".to_string()),
                    None,
                ),
                Err(err) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    if err == "empty_input" { 400 } else { 500 },
                    serde_json::json!({ "error": err }).to_string(),
                    Some(&err),
                ),
            }
        }
        _ if path.starts_with("/api/v1/evidence/") => {
            if request.method() != &Method::Get {
                http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    403,
                    serde_json::json!({ "error": "write_protected", "message": "This V1 endpoint is read-only and only accepts GET." }).to_string(),
                    Some("write_protected"),
                );
                return;
            }
            let item_id = url_decode(path.trim_start_matches("/api/v1/evidence/"));
            match (
                kb_item_detail_internal(&item_id),
                kb_trace_internal(&item_id),
            ) {
                (Ok(item), Ok(trace)) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    200,
                    serde_json::to_string(
                        &serde_json::json!({ "readonly": true, "item": item.item, "trace": trace }),
                    )
                    .unwrap_or_else(|_| "{}".to_string()),
                    None,
                ),
                (Err(err), _) | (_, Err(err)) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                    Some(&err),
                ),
            }
        }
        "/api/v1/call-logs" => {
            if request.method() != &Method::Get {
                http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    403,
                    serde_json::json!({ "error": "write_protected", "message": "This V1 endpoint is read-only and only accepts GET." }).to_string(),
                    Some("write_protected"),
                );
                return;
            }
            let limit = query_param(query, "limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(50);
            match kb_api_call_logs_internal(limit) {
                Ok(data) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                    None,
                ),
                Err(err) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                    Some(&err),
                ),
            }
        }
        "/api/v1/health" => {
            if request.method() != &Method::Get {
                http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    403,
                    serde_json::json!({ "error": "write_protected", "message": "This V1 endpoint is read-only and only accepts GET." }).to_string(),
                    Some("write_protected"),
                );
                return;
            }
            match (
                kb_health_assets_internal(),
                kb_health_projects_internal(),
                kb_health_actions_internal(),
            ) {
                (Ok(assets), Ok(projects), Ok(actions)) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    200,
                    serde_json::to_string(&serde_json::json!({
                        "readonly": true,
                        "summary": kb_health_summary(&assets.assets, &projects.projects),
                        "assets": assets.assets,
                        "projects": projects.projects,
                        "actions": actions.actions
                    }))
                    .unwrap_or_else(|_| "{}".to_string()),
                    None,
                ),
                (Err(err), _, _) | (_, Err(err), _) | (_, _, Err(err)) => http_respond_v1_json(
                    request,
                    api_call_context.as_ref(),
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                    Some(&err),
                ),
            }
        }
        _ if path.starts_with("/api/v1/") => {
            let (status_code, error_code, message) = if request.method() == &Method::Post {
                (
                    403,
                    "write_protected",
                    "Unknown V1 write-like requests are rejected; use the Web UI or a future confirmation queue.",
                )
            } else {
                (404, "not_found", "Unknown V1 endpoint.")
            };
            http_respond_v1_json(
                request,
                api_call_context.as_ref(),
                status_code,
                serde_json::json!({ "error": error_code, "message": message, "readonly": true })
                    .to_string(),
                Some(error_code),
            );
        }
        "/api/health/assets" => match kb_health_assets_internal() {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/health/projects" => match kb_health_projects_internal() {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/health/actions" => match kb_health_actions_internal() {
            Ok(data) => http_respond_json(
                request,
                200,
                serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
            ),
            Err(err) => http_respond_json(
                request,
                500,
                serde_json::json!({ "error": err }).to_string(),
            ),
        },
        "/api/health/refresh" => {
            if request.method() != &Method::Post {
                http_respond_json(
                    request,
                    405,
                    serde_json::json!({ "error": "method_not_allowed" }).to_string(),
                );
                return;
            }
            match (
                kb_health_assets_internal(),
                kb_health_projects_internal(),
                kb_health_actions_internal(),
            ) {
                (Ok(assets), Ok(projects), Ok(actions)) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&serde_json::json!({
                        "summary": kb_health_summary(&assets.assets, &projects.projects),
                        "assets": assets.assets,
                        "projects": projects.projects,
                        "actions": actions.actions,
                        "refreshed": true
                    }))
                    .unwrap_or_else(|_| "{}".to_string()),
                ),
                (Err(err), _, _) | (_, Err(err), _) | (_, _, Err(err)) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/retro/preview" => {
            let mut payload = if request.method() == &Method::Post {
                read_retro_request(&mut request)
            } else {
                KbRetroRequest::default()
            };
            if payload
                .input_text
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.input_text =
                    query_param(query, "input_text").or_else(|| query_param(query, "q"));
            }
            if payload
                .starter_session_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.starter_session_id = query_param(query, "starter_session_id");
            }
            let limit = payload
                .limit
                .or_else(|| {
                    query_param(query, "limit").and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(8);
            match kb_retro_preview_internal(
                payload.input_text.as_deref().unwrap_or_default(),
                payload.starter_session_id.as_deref(),
                limit,
            ) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "empty_input" { 400 } else { 500 },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/retro/package" => {
            let mut payload = if request.method() == &Method::Post {
                read_retro_request(&mut request)
            } else {
                KbRetroRequest::default()
            };
            if payload
                .session_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.session_id = query_param(query, "session_id");
            }
            if payload
                .input_text
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.input_text =
                    query_param(query, "input_text").or_else(|| query_param(query, "q"));
            }
            if payload
                .starter_session_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.starter_session_id = query_param(query, "starter_session_id");
            }
            let limit = payload
                .limit
                .or_else(|| {
                    query_param(query, "limit").and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(8);
            match kb_retro_package_internal(
                payload.session_id.as_deref(),
                payload.input_text.as_deref(),
                payload.starter_session_id.as_deref(),
                limit,
            ) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "empty_input" { 400 } else { 500 },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/retro/suggestions" => {
            let session_id = query_param(query, "session_id").unwrap_or_default();
            match kb_retro_suggestions_internal(&session_id) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "missing_session_id" {
                        400
                    } else {
                        500
                    },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/retro/suggestion/") && path.ends_with("/approve") => {
            if request.method() != &Method::Post {
                http_respond_json(
                    request,
                    405,
                    serde_json::json!({ "error": "method_not_allowed" }).to_string(),
                );
                return;
            }
            let suggestion_id = url_decode(
                path.trim_start_matches("/api/retro/suggestion/")
                    .trim_end_matches("/approve"),
            );
            match kb_retro_approve_suggestion_internal(&suggestion_id) {
                Ok(data) => http_respond_json(request, 200, data.to_string()),
                Err(err) => http_respond_json(
                    request,
                    if err == "suggestion_not_found" || err == "missing_suggestion_id" {
                        404
                    } else {
                        500
                    },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/task-starter/preview" => {
            let mut payload = if request.method() == &Method::Post {
                read_task_starter_request(&mut request)
            } else {
                KbTaskStarterRequest::default()
            };
            if payload
                .input_text
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.input_text =
                    query_param(query, "input_text").or_else(|| query_param(query, "q"));
            }
            let limit = payload
                .limit
                .or_else(|| {
                    query_param(query, "limit").and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(8);
            match kb_task_starter_preview_internal(
                payload.input_text.as_deref().unwrap_or_default(),
                limit,
            ) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "empty_input" { 400 } else { 500 },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/task-starter/package" => {
            let mut payload = if request.method() == &Method::Post {
                read_task_starter_request(&mut request)
            } else {
                KbTaskStarterRequest::default()
            };
            if payload
                .session_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.session_id = query_param(query, "session_id");
            }
            if payload
                .input_text
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                payload.input_text =
                    query_param(query, "input_text").or_else(|| query_param(query, "q"));
            }
            let limit = payload
                .limit
                .or_else(|| {
                    query_param(query, "limit").and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(8);
            match kb_task_starter_package_internal(
                payload.session_id.as_deref(),
                payload.input_text.as_deref(),
                limit,
            ) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    if err == "empty_input" { 400 } else { 500 },
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        "/api/task-starter/sessions" => {
            let limit = query_param(query, "limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(20);
            match kb_task_starter_sessions_internal(limit) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/task-starter/session/") => {
            let session_id = url_decode(path.trim_start_matches("/api/task-starter/session/"));
            match kb_task_starter_session_detail_internal(&session_id) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/prompt-template/") && path.ends_with("/quality") => {
            let id = url_decode(
                path.trim_start_matches("/api/prompt-template/")
                    .trim_end_matches("/quality"),
            );
            let quality_score = query_param(query, "quality_score")
                .and_then(|value| value.trim().parse::<i64>().ok())
                .unwrap_or(60);
            let review_note = query_param(query, "review_note").unwrap_or_default();
            let variables_json = query_param(query, "variables_json").unwrap_or_default();
            let example_input = query_param(query, "example_input").unwrap_or_default();
            let output_format = query_param(query, "output_format").unwrap_or_default();
            let usage_boundary = query_param(query, "usage_boundary").unwrap_or_default();
            match kb_prompt_template_quality_internal(
                &id,
                quality_score,
                &review_note,
                &variables_json,
                &example_input,
                &output_format,
                &usage_boundary,
            ) {
                Ok(data) => http_respond_json(request, 200, data.to_string()),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/prompt-template/") && path.ends_with("/candidate-status") => {
            let id = url_decode(
                path.trim_start_matches("/api/prompt-template/")
                    .trim_end_matches("/candidate-status"),
            );
            let status = query_param(query, "status").unwrap_or_else(|| "refining".into());
            let note = query_param(query, "note").unwrap_or_default();
            match kb_prompt_template_candidate_note_internal(&id, status.trim(), &note) {
                Ok(data) => http_respond_json(request, 200, data.to_string()),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/prompt-template/") && path.ends_with("/copy") => {
            let id = url_decode(
                path.trim_start_matches("/api/prompt-template/")
                    .trim_end_matches("/copy"),
            );
            match kb_prompt_template_copy_internal(&id) {
                Ok(data) => http_respond_json(request, 200, data.to_string()),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/prompt-template/") && path.ends_with("/status") => {
            let id = url_decode(
                path.trim_start_matches("/api/prompt-template/")
                    .trim_end_matches("/status"),
            );
            let status = query_param(query, "status").unwrap_or_else(|| "reviewed".into());
            match kb_prompt_template_status_internal(&id, status.trim()) {
                Ok(data) => http_respond_json(request, 200, data.to_string()),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/prompt-template/") => {
            let id = url_decode(path.trim_start_matches("/api/prompt-template/"));
            match kb_prompt_template_detail_internal(&id) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&serde_json::json!({ "template": data }))
                        .unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/item/") => {
            let item_id = url_decode(path.trim_start_matches("/api/item/"));
            match kb_item_detail_internal(&item_id) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ if path.starts_with("/api/trace/") => {
            let item_id = url_decode(path.trim_start_matches("/api/trace/"));
            match kb_trace_internal(&item_id) {
                Ok(data) => http_respond_json(
                    request,
                    200,
                    serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                ),
                Err(err) => http_respond_json(
                    request,
                    500,
                    serde_json::json!({ "error": err }).to_string(),
                ),
            }
        }
        _ => {
            http_respond_html(request, 404, "<h1>Not Found</h1>".to_string());
        }
    }
}

fn is_knowledgebase_http_healthy() -> bool {
    let bind_addr = knowledgebase_bind_addr();
    let mut addrs = match bind_addr.to_socket_addrs() {
        Ok(iter) => iter,
        Err(_) => return false,
    };
    let Some(addr) = addrs.next() else {
        return false;
    };
    let timeout = Duration::from_millis(KB_HEALTHCHECK_TIMEOUT_MS);
    let mut stream = match TcpStream::connect_timeout(&addr, timeout) {
        Ok(stream) => stream,
        Err(_) => return false,
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let req = format!("GET /api/stats HTTP/1.1\r\nHost: {bind_addr}\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0_u8; 128];
    match stream.read(&mut buf) {
        Ok(size) if size > 0 => String::from_utf8_lossy(&buf[..size]).contains("200"),
        _ => false,
    }
}

fn spawn_knowledgebase_health_watchdog<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    alert_settings: SharedAlertSettings,
) {
    thread::spawn(move || {
        let mut consecutive_failures = 0_u32;
        let mut last_alert_at = 0_i64;
        loop {
            thread::sleep(Duration::from_secs(KB_HEALTHCHECK_INTERVAL_SECONDS));
            if !knowledgebase_auto_push_enabled() {
                consecutive_failures = 0;
                continue;
            }
            if is_knowledgebase_http_healthy() {
                consecutive_failures = 0;
                if let Ok(mut guard) = knowledgebase_push_state().lock() {
                    if guard.last_error == "knowledgebase_http_unhealthy" {
                        guard.last_error.clear();
                    }
                }
                continue;
            }

            consecutive_failures = consecutive_failures.saturating_add(1);
            if let Ok(mut guard) = knowledgebase_push_state().lock() {
                guard.failure_count = guard.failure_count.saturating_add(1);
                guard.last_error = "knowledgebase_http_unhealthy".into();
            }
            if consecutive_failures < KB_HEALTHCHECK_CONSECUTIVE_FAILURES {
                continue;
            }
            let now = unix_now();
            if now - last_alert_at < KB_HEALTHCHECK_ALERT_COOLDOWN_SECONDS {
                continue;
            }
            last_alert_at = now;
            push_alert(
                &app,
                &alert_settings,
                "manual_test",
                "知识库服务异常",
                "内置知识库接口连续超时，已触发健康告警。建议重启应用并关注网络/磁盘占用。",
                true,
                false,
                None,
            );
        }
    });
}

fn start_knowledgebase_web_server() -> Result<(), String> {
    let bind_addr = knowledgebase_bind_addr();
    let server = Server::http(&bind_addr)
        .map_err(|err| format!("知识库 Web 服务启动失败({bind_addr}): {err}"))?;
    thread::spawn(move || {
        for request in server.incoming_requests() {
            // Isolate slow handlers to avoid blocking the accept loop.
            thread::spawn(move || {
                handle_knowledgebase_http_request(request);
            });
        }
    });
    Ok(())
}

fn ensure_knowledgebase_web_server() -> Result<(), String> {
    let state = knowledgebase_web_server_state();
    let result = state.get_or_init(start_knowledgebase_web_server);
    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            if let Ok(mut guard) = knowledgebase_push_state().lock() {
                if guard.last_error.is_empty() {
                    guard.failure_count = guard.failure_count.saturating_add(1);
                }
                guard.last_error = err.clone();
            }
            Err(err.clone())
        }
    }
}

fn open_knowledgebase_internal() -> Result<(), String> {
    ensure_knowledgebase_web_server()?;
    let base = knowledgebase_web_url();
    let sep = if base.contains('?') { "&" } else { "?" };
    let url = format!("{base}{sep}t={}", unix_now());
    open_url(&url)
}

fn post_knowledgebase_event(
    project: &ProjectSnapshot,
    event_type: &str,
    title: &str,
    body: &str,
) -> Result<(), String> {
    if !knowledgebase_auto_push_enabled() {
        return Ok(());
    }
    let mut conn = connect_knowledgebase()?;
    let tx = conn.transaction().map_err(|err| err.to_string())?;
    let project_id = kb_upsert_project(&tx, &project.name, &project.path)?;
    let payload = serde_json::json!({
        "event_type": format!("statusbar.{event_type}"),
        "summary": body,
        "title": title,
        "project_name": project.name,
        "project_path": project.path,
        "workflow_stage": workflow_stage_key(&project.workflow_stage),
        "codex_status": codex_status_key(&project.codex_status),
        "thread_id": project.codex_thread_id,
        "host": host_kind_label(project.active_host.as_ref()),
        "req_id": if project.current_req_id.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(project.current_req_id.clone()) },
        "task_id": if project.current_task_id.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(project.current_task_id.clone()) },
        "source_path": "workflow-statusbar/runtime",
        "occurred_at": unix_now(),
    });
    let result = kb_process_event_payload(&tx, &project_id, "workflow-statusbar/runtime", &payload)
        .and_then(|_| tx.commit().map_err(|err| err.to_string()));

    if let Ok(mut guard) = knowledgebase_push_state().lock() {
        match &result {
            Ok(()) => {
                guard.last_push_ts = unix_now();
                guard.last_error.clear();
            }
            Err(err) => {
                guard.failure_count = guard.failure_count.saturating_add(1);
                guard.last_error = err.clone();
            }
        }
    }

    result
}

fn snapshot_knowledgebase_push_status() -> KnowledgebasePushStatus {
    let enabled = knowledgebase_auto_push_enabled();
    let _ = ensure_knowledgebase_web_server();
    let endpoint = knowledgebase_web_url();
    let mut last_push_at = "未上报".to_string();
    let mut failure_count = 0_u64;
    let mut last_error = String::new();

    if let Ok(guard) = knowledgebase_push_state().lock() {
        if guard.last_push_ts > 0 {
            last_push_at = fmt_relative_age(guard.last_push_ts);
        }
        failure_count = guard.failure_count;
        last_error = guard.last_error.clone();
    }

    let db_connected = connect_knowledgebase().is_ok();
    let web_connected = matches!(knowledgebase_web_server_state().get(), Some(Ok(())));
    let connected = enabled && db_connected && web_connected;
    if enabled && !connected && last_error.is_empty() {
        last_error = "内置知识库初始化失败".into();
    }

    KnowledgebasePushStatus {
        enabled,
        endpoint,
        connected,
        last_push_at,
        failure_count,
        last_error,
    }
}

fn find_auto_resume_project<'a>(
    projects: &'a [ProjectSnapshot],
    project_path: &str,
) -> Option<&'a ProjectSnapshot> {
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

fn should_attempt_follow_up_resume(
    previous: &ProjectRuntimeSignature,
    current: &ProjectRuntimeSignature,
) -> bool {
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
                knowledgebase_push: snapshot_knowledgebase_push_status(),
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
    let (claude_threads, mut claude_debug_entries, claude_probe) =
        read_recent_claude_threads(&home);

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
        if let Some(snapshot) =
            read_project_snapshot(&state_file, &active_project_path, Some(&project_runtime))
        {
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
        if projects
            .iter()
            .any(|project| project.name == name || project.path == pseudo_path)
        {
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
                token_usage: build_project_token_usage(
                    &matched_threads,
                    today,
                    &mut token_usage_cache,
                ),
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
        if projects.iter().any(|project| {
            path_matches(&project.path, open_path) || path_matches(open_path, &project.path)
        }) {
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
                token_usage: build_project_token_usage(
                    &matched_threads,
                    today,
                    &mut token_usage_cache,
                ),
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
    enrich_projects_with_claude_host(
        &mut projects,
        &claude_threads,
        &mut claude_debug_entries,
        now,
    );

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
    let hosts = vec![
        build_codex_global_host_session(&codex_state, now),
        claude_host,
    ];
    let active_host = select_active_host_session(&hosts, None).map(|session| session.host.clone());
    let other_host_summary = other_host_summary_for(&hosts, active_host.as_ref(), now);

    let projects_before_apply = projects.clone();
    let mut runtime = RuntimeState {
        codex: codex_state,
        active_host,
        other_host_summary,
        hosts,
        knowledgebase_push: snapshot_knowledgebase_push_status(),
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
    runtime.spotlight_project =
        find_spotlight(&runtime.projects, &ide_signal, &active_project_path);
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
        focus_project_path: focus
            .as_ref()
            .map(|project| project.path.clone())
            .unwrap_or_default(),
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
        if previous.focus_task_status != "blocked"
            && current_signature.focus_task_status == "blocked"
        {
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

            let interrupted =
                should_attempt_auto_resume(&previous.codex_status, &signature.codex_status);
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
    let settings = alert_settings
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
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
            // Remote alerts should never block the runtime poll loop.
            thread::spawn(move || {
                let _ = post_remote_alert(&config, &payload);
            });
        }
    }
    if let Some(project) = project {
        let _ = post_knowledgebase_event(project, event_type, title, body);
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
    let mut guard = state
        .lock()
        .map_err(|_| "alert settings lock poisoned".to_string())?;
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
        project_path:
            "/Users/wucongpeng/Documents/ai/skill/workflow-skills-copy/workflow-statusbar".into(),
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
    let next_height = (content_height + 12.0)
        .ceil()
        .min((monitor_height - 96.0).max(260.0));

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

fn mark_main_window_shown() {
    if let Ok(mut guard) = main_window_last_shown_at().lock() {
        *guard = Some(Instant::now());
    }
}

fn is_main_window_show_grace_active() -> bool {
    main_window_last_shown_at()
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .map(|shown_at| shown_at.elapsed() < Duration::from_millis(MAIN_WINDOW_SHOW_GRACE_MS))
        .unwrap_or(false)
}

fn hide_main_window_with_delay<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    tray_rect: Option<Rect>,
) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(TRAY_HIDE_DELAY_MS));
        if tray_rect.is_none() && is_main_window_show_grace_active() {
            return;
        }
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
        let scale_factor = window.scale_factor().map_err(|err| err.to_string())?;
        let (tray_x, tray_y) = match rect.position {
            Position::Physical(position) => (position.x, position.y),
            Position::Logical(position) => (
                (position.x * scale_factor).round() as i32,
                (position.y * scale_factor).round() as i32,
            ),
        };
        let (tray_w, tray_h) = match rect.size {
            Size::Physical(size) => (size.width as i32, size.height as i32),
            Size::Logical(size) => (
                (size.width * scale_factor).round() as i32,
                (size.height * scale_factor).round() as i32,
            ),
        };
        let mut popup_x = tray_x + (tray_w / 2) - (size.width as i32 / 2);
        let mut popup_y = if cfg!(target_os = "windows") {
            tray_y - size.height as i32 - 8
        } else {
            // macOS menu bar icon sits at the top; show popup below tray icon.
            tray_y + tray_h + 2
        };

        let mut target_bounds: Option<(i32, i32, i32, i32)> = None;
        if let Ok(monitors) = window.available_monitors() {
            let pick_monitor_bounds = |px: f64, py: f64| -> Option<(i32, i32, i32, i32)> {
                monitors.iter().find_map(|monitor| {
                    let pos = monitor.position();
                    let msize = monitor.size();
                    let left = pos.x;
                    let top = pos.y;
                    let right = pos.x + msize.width as i32;
                    let bottom = pos.y + msize.height as i32;
                    if px >= left as f64
                        && px <= right as f64
                        && py >= top as f64
                        && py <= bottom as f64
                    {
                        Some((left, top, msize.width as i32, msize.height as i32))
                    } else {
                        None
                    }
                })
            };

            if cfg!(target_os = "windows") {
                if let Ok(cursor) = app.cursor_position() {
                    target_bounds = pick_monitor_bounds(cursor.x, cursor.y);
                }
            }

            if target_bounds.is_none() {
                let tray_cx = (tray_x + tray_w / 2) as f64;
                let tray_cy = (tray_y + tray_h / 2) as f64;
                target_bounds = pick_monitor_bounds(tray_cx, tray_cy);
            }

            if target_bounds.is_none() {
                if let Some(first) = monitors.first() {
                    let pos = first.position();
                    let size = first.size();
                    target_bounds = Some((pos.x, pos.y, size.width as i32, size.height as i32));
                }
            }
        }

        if target_bounds.is_none() {
            if let Some(monitor) = window.current_monitor().map_err(|err| err.to_string())? {
                let pos = monitor.position();
                let size = monitor.size();
                target_bounds = Some((pos.x, pos.y, size.width as i32, size.height as i32));
            }
        }

        if cfg!(target_os = "windows") {
            if let Some((mx, my, mw, mh)) = target_bounds {
                let right_margin = 14;
                let bottom_margin = 56;
                popup_x = mx + mw - size.width as i32 - right_margin;
                popup_y = my + mh - size.height as i32 - bottom_margin;
            } else if let Ok(cursor) = app.cursor_position() {
                popup_x = cursor.x.round() as i32 - (size.width as i32 / 2);
                popup_y = cursor.y.round() as i32 - size.height as i32 - 10;
            }
        }

        if let Some((mx, my, mw, mh)) = target_bounds {
            let min_x = mx + 12;
            let min_y = my + 12;
            let max_x = (mx + mw - size.width as i32 - 12).max(min_x);
            let max_y = (my + mh - size.height as i32 - 12).max(min_y);
            popup_x = popup_x.clamp(min_x, max_x);
            popup_y = popup_y.clamp(min_y, max_y);
        }

        window
            .set_position(Position::Physical(PhysicalPosition {
                x: popup_x,
                y: popup_y,
            }))
            .map_err(|err| err.to_string())?;
    } else {
        position_top_center(&window)?;
    }

    mark_main_window_shown();
    window.show().map_err(|err| err.to_string())?;
    window.unminimize().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_floating_visibility<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    visible: bool,
) -> Result<(), String> {
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
    open_url(&path)
}

#[tauri::command]
fn open_knowledgebase<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    open_knowledgebase_internal()?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
fn kb_get_stats() -> Result<KbStats, String> {
    kb_get_stats_internal()
}

#[tauri::command]
fn kb_search(query: String) -> Result<KbSearchResponse, String> {
    kb_search_internal(&query)
}

#[tauri::command]
fn kb_trace(item_id: String) -> Result<KbTraceResponse, String> {
    kb_trace_internal(&item_id)
}

#[tauri::command]
fn kb_register_project(path: String, name: Option<String>) -> Result<String, String> {
    kb_register_project_internal(&path, name)
}

#[tauri::command]
fn kb_ingest_inbox(path: String) -> Result<serde_json::Value, String> {
    let (events, processed_files) = kb_ingest_inbox_internal(&path)?;
    Ok(serde_json::json!({
        "project": path,
        "events": events,
        "processed_files": processed_files
    }))
}

#[tauri::command]
fn kb_compact_conversations() -> Result<serde_json::Value, String> {
    kb_compact_conversations_internal()
}

#[tauri::command]
fn kb_push_event(
    path: String,
    event: serde_json::Value,
    process_now: Option<bool>,
) -> Result<(), String> {
    kb_push_event_internal(&path, &event, process_now.unwrap_or(true))
}

#[tauri::command]
fn kb_collect_project(path: String) -> Result<KbCollectProjectResult, String> {
    kb_collect_project_internal(&path)
}

#[tauri::command]
fn kb_list_projects() -> Result<Vec<KbProjectStatus>, String> {
    kb_list_projects_internal()
}

fn build_tray_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<Menu<R>, String> {
    let open_dashboard = MenuItem::with_id(
        app,
        TRAY_MENU_OPEN_DASHBOARD,
        "打开面板",
        true,
        None::<&str>,
    )
    .map_err(|err| err.to_string())?;
    let open_alert_settings = MenuItem::with_id(
        app,
        TRAY_MENU_OPEN_ALERT_SETTINGS,
        "提醒配置",
        true,
        None::<&str>,
    )
    .map_err(|err| err.to_string())?;
    let open_knowledgebase = MenuItem::with_id(
        app,
        TRAY_MENU_OPEN_KNOWLEDGEBASE,
        "打开知识库",
        true,
        None::<&str>,
    )
    .map_err(|err| err.to_string())?;
    let quit = MenuItem::with_id(app, TRAY_MENU_QUIT, "退出", true, None::<&str>)
        .map_err(|err| err.to_string())?;

    Menu::with_items(
        app,
        &[
            &open_dashboard,
            &open_alert_settings,
            &open_knowledgebase,
            &quit,
        ],
    )
    .map_err(|err| err.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime_cache: SharedRuntimeCache = Arc::new(Mutex::new(RuntimeCache::default()));

    tauri::Builder::default()
        .manage(runtime_cache.clone())
        .setup(move |app| {
            let alert_settings: SharedAlertSettings =
                Arc::new(Mutex::new(read_alert_settings(app.handle())));
            app.manage(alert_settings.clone());
            let _ = ensure_knowledgebase_web_server();
            #[cfg(target_os = "macos")]
            app.set_activation_policy(ActivationPolicy::Accessory);

            let main_window = app
                .get_webview_window("main")
                .expect("main window should exist");
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

            let icon = app.default_window_icon().cloned();
            let tray_handle = app.handle().clone();
            let tray_menu = build_tray_menu(app.handle())
                .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;
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
                    TRAY_MENU_OPEN_KNOWLEDGEBASE => {
                        let _ = open_knowledgebase(app.clone());
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
            thread::spawn(move || {
                let home = home_dir();
                let mut kb_auto_cursor = KbAutoCollectCursor::default();
                loop {
                    if let Some(home) = home.as_ref() {
                        let _ = kb_auto_collect_runtime_conversations(home, &mut kb_auto_cursor);
                    }
                    emit_runtime_state(&poller_app, &poller_cache, &poller_alert_settings);
                    thread::sleep(Duration::from_secs(POLL_INTERVAL_SECONDS));
                }
            });
            spawn_knowledgebase_health_watchdog(app.handle().clone(), alert_settings.clone());

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
            open_path,
            open_knowledgebase,
            kb_get_stats,
            kb_search,
            kb_trace,
            kb_register_project,
            kb_ingest_inbox,
            kb_compact_conversations,
            kb_push_event,
            kb_collect_project,
            kb_list_projects
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
        assert!(path_matches(
            "/tmp/solo",
            "/tmp/solo/.ai/runtime/project-state.json"
        ));
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

        let index = find_best_project_index(
            &projects,
            "/Users/wucongpeng/Documents/ai/skill/workflow-skills-copy",
        )
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
