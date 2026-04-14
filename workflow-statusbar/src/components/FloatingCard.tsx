import { invoke } from "@tauri-apps/api/core";
import type { RuntimeState } from "../lib/types";
import { codexStatusLabels } from "../lib/codex-labels";
import { isWorkflowLinked, parseTaskProgress } from "../lib/progress";

type FloatingCardProps = {
  state: RuntimeState;
};

export function FloatingCard({ state }: FloatingCardProps) {
  const project = state.spotlight_project;
  const workflowLinked = isWorkflowLinked(project);
  const autoResumeCopy = project
    ? project.auto_resume_enabled
      ? "自动续跑开启"
      : workflowLinked
        ? "自动续跑关闭"
        : "未接入 workflow"
    : "等待关联项目";
  const progressRatio = workflowLinked ? parseTaskProgress(project?.progress_label) : null;

  return (
    <section className="card card--floating">
      <header className="agent-card__head">
        <div className="agent-card__brand">
          <div className="agent-avatar">X</div>
          <div className="agent-title">
            <h2>Codex</h2>
            <p>{project?.name ?? "等待关联项目"}</p>
          </div>
        </div>
        <div className="agent-card__status">
          <span className={`status-dot status-dot--${state.codex.status}`} />
          <strong>{codexStatusLabels[state.codex.status]}</strong>
        </div>
      </header>

      <div className="agent-card__divider" />

      <div className="agent-card__row">
        <div className="agent-card__signal">
          <span className="status-dot status-dot--running" />
          <strong>{project ? project.stage_label : "当前会话"}</strong>
        </div>
        <div className="agent-card__meta">
          <span>{project ? workflowLinked ? project.current_task_id || project.current_req_id || "待同步" : "未接入 workflow，暂无任务同步" : state.codex.active_thread_name}</span>
          <strong>{project ? codexStatusLabels[project.codex_status] : "--"}</strong>
        </div>
      </div>

      <div className="agent-card__subrow">
        <span>最近心跳 {project?.codex_heartbeat_at ?? state.codex.heartbeat_at} · {autoResumeCopy}</span>
        <button className="inline-link-button" type="button" onClick={() => invoke("set_floating_visibility", { visible: false })}>
          隐藏
        </button>
      </div>

      {progressRatio !== null ? (
        <div className="progress-track">
          <div className="progress-fill" style={{ width: `${progressRatio}%` }} />
        </div>
      ) : null}
    </section>
  );
}
