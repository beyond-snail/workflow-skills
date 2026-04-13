import type { ProjectSnapshot, RuntimeState } from "../lib/types";
import { codexStatusLabels } from "../lib/codex-labels";

type StatusCardProps = {
  state: RuntimeState;
  project: ProjectSnapshot | null;
};

export function StatusCard({ state, project }: StatusCardProps) {
  const activeThreadName = state.codex.active_thread_name || "等待活跃线程";
  let autoResumeCopy = "未接入 workflow";
  if (project) {
    autoResumeCopy = state.codex.auto_resume_enabled
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
  const progressRatio =
    state.codex.status === "running"
      ? 78
      : state.codex.status === "waiting_input"
        ? 52
        : state.codex.status === "stalled"
          ? 34
          : state.codex.status === "offline"
            ? 8
            : 18;

  return (
    <section className="card card--hero">
      <div className="agent-card__head">
        <div className="agent-card__brand">
          <div className="agent-avatar">X</div>
          <div className="agent-title">
            <h1>Codex</h1>
            <p>{projectLine ? "监控中" : "等待识别项目"}</p>
          </div>
        </div>
        <div className="agent-card__status">
          <span className={`status-dot status-dot--${state.codex.status}`} />
          <strong>{codexStatusLabels[state.codex.status]}</strong>
        </div>
      </div>

      <div className="agent-card__divider" />

      {projectLine ? (
        <div className="status-detail-block">
          <p className="status-detail-block__title" title={projectLine}>
            {projectLine}
          </p>
        </div>
      ) : null}

      <div className="status-detail-row">
        <span className="status-detail-row__label">当前会话</span>
        <span className="status-detail-row__value status-detail-row__value--single" title={activeThreadName}>
          {activeThreadName}
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
        <span className="status-detail-row__label">{lastMessageRole}</span>
        <span className="status-detail-row__value status-detail-row__value--multiline" title={lastMessageText}>
          {lastMessageText}
        </span>
      </div>

      <div className="agent-card__subrow">
        <span>心跳 {state.codex.heartbeat_at}</span>
        <span>{autoResumeCopy}</span>
      </div>

      <div className="progress-track">
        <div className="progress-fill" style={{ width: `${progressRatio}%` }} />
      </div>
    </section>
  );
}
