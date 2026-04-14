import { invoke } from "@tauri-apps/api/core";
import type { ProjectSnapshot } from "../lib/types";
import { codexStatusLabels } from "../lib/codex-labels";
import { projectOtherHostSummary, projectPrimaryHostLabel } from "../lib/host-utils";
import { isWorkflowLinked, parseTaskProgress } from "../lib/progress";

type FocusCardProps = {
  project: ProjectSnapshot;
};

export function FocusCard({ project }: FocusCardProps) {
  const workflowLinked = isWorkflowLinked(project);
  const autoResumeCopy = project.auto_resume_enabled
    ? "自动续跑已开启"
    : workflowLinked
      ? "自动续跑未开启"
      : "未接入 workflow";
  const progressRatio = workflowLinked ? parseTaskProgress(project.progress_label) : null;
  const hostLabel = projectPrimaryHostLabel(project);
  const otherHostSummary = projectOtherHostSummary(project);

  return (
    <section className="card card--focus">
      <div className="agent-card__head">
        <div className="agent-card__brand">
          <div className="agent-avatar agent-avatar--project">
            {project.name.slice(0, 1)}
          </div>
          <div className="agent-title">
            <h2>{project.name}</h2>
            <p>{workflowLinked ? project.current_task_title || project.current_req_title || "等待 task / req 回写" : "未接入 workflow，暂无任务同步"}</p>
          </div>
        </div>
        <div className="agent-card__status agent-card__status--muted">
          <span className={`status-dot status-dot--${project.is_blocked ? "stalled" : project.workflow_stage}`} />
          <strong>{project.gate_status}</strong>
        </div>
      </div>

      <div className="agent-card__divider" />

      <div className="agent-card__row">
        <div className="agent-card__signal">
          <span className="status-dot status-dot--running" />
          <strong>当前任务</strong>
        </div>
        <div className="agent-card__meta">
          <span>{workflowLinked ? `${project.current_task_id || project.current_req_id || "待同步"} / ${project.stage_label}` : "未接入 workflow，暂无任务同步"}</span>
          <strong>{workflowLinked ? project.progress_label.replace("任务 ", "") : "未接入 workflow"}</strong>
        </div>
      </div>

      <div className="agent-card__subrow">
        <span>
          {hostLabel} {codexStatusLabels[project.codex_status]} · {project.codex_heartbeat_at}
          {otherHostSummary ? ` · ${otherHostSummary}` : ""}
          {" · "}
          {autoResumeCopy}
        </span>
        <button className="inline-link-button" type="button" onClick={() => invoke("open_path", { path: project.path })}>
          打开目录
        </button>
      </div>

      {progressRatio !== null ? (
        <div className="progress-track">
          <div
            className="progress-fill progress-fill--soft"
            style={{ width: `${progressRatio}%` }}
          />
        </div>
      ) : null}
    </section>
  );
}
