import { invoke } from "@tauri-apps/api/core";
import type { ProjectSnapshot } from "../lib/types";
import { codexStatusLabels } from "../lib/codex-labels";
import { isWorkflowLinked, parseTaskProgress } from "../lib/progress";

type FocusCardProps = {
  project: ProjectSnapshot;
};

export function FocusCard({ project }: FocusCardProps) {
  const autoResumeCopy = project.auto_resume_enabled ? "自动续跑已开启" : "自动续跑未开启";
  const progressRatio = isWorkflowLinked(project) ? parseTaskProgress(project.progress_label) : null;

  return (
    <section className="card card--focus">
      <div className="agent-card__head">
        <div className="agent-card__brand">
          <div className="agent-avatar agent-avatar--project">
            {project.name.slice(0, 1)}
          </div>
          <div className="agent-title">
            <h2>{project.name}</h2>
            <p>{project.current_task_title || project.current_req_title || "等待 task / req 回写"}</p>
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
          <span>{project.current_task_id || project.current_req_id || "待同步"} / {project.stage_label}</span>
          <strong>{project.progress_label.replace("任务 ", "")}</strong>
        </div>
      </div>

      <div className="agent-card__subrow">
        <span>Codex {codexStatusLabels[project.codex_status]} · {project.codex_heartbeat_at} · {autoResumeCopy}</span>
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
