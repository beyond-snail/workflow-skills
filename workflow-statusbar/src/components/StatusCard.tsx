import type { ProjectSnapshot, RuntimeState } from "../lib/types";
import { codexStatusLabels } from "../lib/codex-labels";

type StatusCardProps = {
  state: RuntimeState;
  project: ProjectSnapshot | null;
  compact?: boolean;
};

export function StatusCard({ state, project, compact = false }: StatusCardProps) {
  const activeThreadName = state.codex.active_thread_name || "等待活跃线程";
  let autoResumeCopy = "未接入 workflow";
  if (project) {
    autoResumeCopy = compact
      ? project.auto_resume_enabled
        ? "自动续跑已开启"
        : project.workflow_stage === "unknown"
          ? "未接入 workflow"
          : "自动续跑未开启"
      : state.codex.auto_resume_enabled
        ? `自动续跑已开启${state.codex.monitored_project_name ? ` · ${state.codex.monitored_project_name}` : ""}`
        : "自动续跑未开启";
  }
  const lastMessageRole =
    state.codex.last_message_role === "user"
      ? "最后输入"
      : state.codex.last_message_role === "assistant"
        ? "最后回复"
        : "最后对话";
  const lastMessageText = state.codex.last_message_text || "暂无可展示的最近对话内容";
  const projectLine = project?.name || state.codex.active_ide_project_name || "";
  const taskLine = project
    ? `${project.current_task_id || project.current_req_id || "待同步"} · ${project.current_task_title || project.current_req_title || "等待 task / req 回写"}`
    : "";
  const displayThreadName = compact && project ? project.codex_thread_name || activeThreadName : activeThreadName;
  const displayLastMessage = compact
    ? project?.health || project?.gate_status || "未接入 workflow"
    : lastMessageText;
  const displayLastMessageRole = compact ? "最近状态" : lastMessageRole;
  const headerTitle = projectLine || "Codex";
  const headerCaption = compact
    ? project?.gate_status || "未接入 workflow"
    : projectLine
      ? "Codex 监控"
      : "等待识别项目";
  const progressRatio =
    compact
      ? project?.workflow_stage === "execution"
        ? 68
        : project?.workflow_stage === "requirement"
          ? 44
          : project?.workflow_stage === "done"
            ? 100
            : 18
      : state.codex.status === "running"
      ? 78
      : state.codex.status === "waiting_input"
        ? 52
        : state.codex.status === "stalled"
          ? 34
          : state.codex.status === "offline"
            ? 8
            : 18;

  return (
    <section className={`card ${compact ? "card--focus" : "card--hero"}`}>
      <div className="agent-card__head">
        <div className="agent-card__brand">
          <div className={compact ? "agent-avatar agent-avatar--project" : "agent-avatar"}>
            {compact && project ? project.name.slice(0, 1) : "X"}
          </div>
          <div className="agent-title">
            {compact ? <h2>{headerTitle}</h2> : <h1>{headerTitle}</h1>}
            <p>{headerCaption}</p>
          </div>
        </div>
        <div className="agent-card__status">
          <span className={`status-dot status-dot--${compact && project ? project.codex_status : state.codex.status}`} />
          <strong>{compact && project ? codexStatusLabels[project.codex_status] : codexStatusLabels[state.codex.status]}</strong>
        </div>
      </div>

      <div className="agent-card__divider" />

      <div className="status-detail-row">
        <span className="status-detail-row__label">当前会话</span>
        <span className="status-detail-row__value status-detail-row__value--single" title={displayThreadName}>
          {displayThreadName}
        </span>
      </div>

      {taskLine ? (
        <div className="status-detail-row">
          <span className="status-detail-row__label">当前任务</span>
          <span className="status-detail-row__value status-detail-row__value--single" title={taskLine}>
            {taskLine}
          </span>
        </div>
      ) : null}

      <div className="status-detail-row status-detail-row--top">
        <span className="status-detail-row__label">{displayLastMessageRole}</span>
        <span className="status-detail-row__value status-detail-row__value--multiline" title={displayLastMessage}>
          {displayLastMessage}
        </span>
      </div>

      <div className="agent-card__subrow">
        <span>心跳 {compact && project ? project.codex_heartbeat_at : state.codex.heartbeat_at}</span>
        <span>{autoResumeCopy}</span>
      </div>

      <div className="progress-track">
        <div className="progress-fill" style={{ width: `${progressRatio}%` }} />
      </div>
    </section>
  );
}
