import type { ProjectSnapshot, RuntimeState } from "../lib/types";
import { codexStatusLabels } from "../lib/codex-labels";
import { projectOtherHostSummary, projectPrimaryHostLabel, runtimeOtherHostSummary, runtimePrimaryHostLabel } from "../lib/host-utils";
import { selectDisplayHostSession } from "../lib/host-session";
import { isWorkflowLinked, parseTaskProgress } from "../lib/progress";

type StatusCardProps = {
  state: RuntimeState;
  project: ProjectSnapshot | null;
  compact?: boolean;
};

function formatToken(value: number) {
  if (!value) {
    return "0";
  }
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(value >= 10_000_000 ? 0 : 1)}M`;
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(value >= 100_000 ? 0 : 1)}k`;
  }
  return String(value);
}

export function StatusCard({ state, project, compact = false }: StatusCardProps) {
  const hasProject = Boolean(project);
  const workflowLinked = isWorkflowLinked(project);
  const displayHostSession = selectDisplayHostSession(project);
  let autoResumeCopy = "未接入 workflow";
  if (project) {
    const autoResumeEnabled = displayHostSession?.auto_resume_enabled ?? project.auto_resume_enabled;
    autoResumeCopy = autoResumeEnabled
      ? "自动续跑已开启"
      : !workflowLinked
        ? "未接入 workflow"
        : "自动续跑未开启";
  } else {
    autoResumeCopy = "等待打开项目";
  }
  const effectiveLastMessageRole =
    displayHostSession?.last_message_role
    || (hasProject ? "" : state.codex.last_message_role);
  const effectiveLastMessageText =
    displayHostSession?.last_message_text
    || (hasProject ? "" : state.codex.last_message_text);
  const lastMessageRole =
    effectiveLastMessageRole === "user"
      ? "最后输入"
      : effectiveLastMessageRole === "assistant"
        ? "最后回复"
        : "最后对话";
  const lastMessageText = hasProject
    ? effectiveLastMessageText || "暂无可展示的最近对话内容"
    : "状态栏仍在后台运行。打开 IDE 项目后，会自动展示对应项目的会话、任务和 Token。";
  const projectLine = project?.name || state.codex.active_ide_project_name || "";
  const taskLine = project
    ? workflowLinked
      ? `${project.current_task_id || project.current_req_id || "待同步"} · ${project.current_task_title || project.current_req_title || "等待 task / req 回写"}`
      : "未接入 workflow，暂无任务同步"
    : "";
  const displayThreadName = hasProject
    ? displayHostSession?.thread_name || "暂无可展示的最近会话"
    : "等待打开 IDE 项目";
  const compactFallbackMessage = project?.health || project?.gate_status || "未接入 workflow";
  const displayLastMessage =
    compact && !effectiveLastMessageText
      ? compactFallbackMessage
      : lastMessageText;
  const displayLastMessageRole =
    compact && !effectiveLastMessageText
      ? "最近状态"
      : lastMessageRole;
  const tokenLine = project && (displayHostSession?.token_total ?? 0) > 0
    ? `Token ${formatToken(displayHostSession?.token_total ?? 0)} · 输入 ${formatToken(displayHostSession?.token_input ?? 0)} · 输出 ${formatToken(displayHostSession?.token_output ?? 0)} · 推理 ${formatToken(displayHostSession?.token_reasoning ?? 0)}`
    : hasProject
      ? "Token 未采集"
      : "打开项目后采集 Token";
  const headerTitle = projectLine || "未检测到打开的 IDE 项目";
  const primaryHostLabel = project ? projectPrimaryHostLabel(project) : runtimePrimaryHostLabel(state);
  const otherHostSummary = project ? projectOtherHostSummary(project) : runtimeOtherHostSummary(state);
  const headerCaption = compact
    ? project?.gate_status || "未接入 workflow"
    : projectLine
      ? `${primaryHostLabel} 监控${otherHostSummary ? ` · ${otherHostSummary}` : ""}`
      : "打开项目后自动开始监控";
  const progressRatio = project && workflowLinked ? parseTaskProgress(project.progress_label) : null;

  const displayStatus = displayHostSession?.status || (hasProject ? "offline" : state.codex.status);
  const displayStatusLabel =
    project && !workflowLinked && displayStatus === "stalled"
      ? "等待中"
      : codexStatusLabels[displayStatus];

  return (
    <section className={`card ${compact ? "card--focus" : "card--hero"}`}>
      <div className="agent-card__head">
        <div className="agent-card__brand">
          <div className={compact ? "agent-avatar agent-avatar--project" : "agent-avatar"}>
            {project ? project.name.slice(0, 1) : "X"}
          </div>
          <div className="agent-title">
            {compact ? <h2>{headerTitle}</h2> : <h1>{headerTitle}</h1>}
            <p>{headerCaption}</p>
          </div>
        </div>
        <div className="agent-card__status">
          <span className={`status-dot status-dot--${displayStatus}`} />
          <strong>{displayStatusLabel}</strong>
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
        <span>心跳 {project ? (displayHostSession?.heartbeat_at || "暂无") : state.codex.heartbeat_at}</span>
        <span>{autoResumeCopy}</span>
      </div>

      <div className="token-line" title={tokenLine}>
        {tokenLine}
      </div>

      {progressRatio !== null ? (
        <div className="progress-track">
          <div className="progress-fill" style={{ width: `${progressRatio}%` }} />
        </div>
      ) : null}
    </section>
  );
}
