import { invoke } from "@tauri-apps/api/core";
import type { CodexState, ProjectSnapshot } from "../lib/types";

type FocusCardProps = {
  project: ProjectSnapshot;
  codex: CodexState;
};

export function FocusCard({ project, codex }: FocusCardProps) {
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
        <span>最近心跳 {codex.heartbeat_at}</span>
        <button className="inline-link-button" type="button" onClick={() => invoke("open_path", { path: project.path })}>
          打开目录
        </button>
      </div>

      <div className="progress-track">
        <div
          className="progress-fill progress-fill--soft"
          style={{ width: project.workflow_stage === "execution" ? "68%" : project.workflow_stage === "requirement" ? "44%" : "18%" }}
        />
      </div>
    </section>
  );
}
