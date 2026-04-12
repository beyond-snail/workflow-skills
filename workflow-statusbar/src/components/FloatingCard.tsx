import { invoke } from "@tauri-apps/api/core";
import type { RuntimeState } from "../lib/types";

type FloatingCardProps = {
  state: RuntimeState;
};

const codexLabels: Record<RuntimeState["codex"]["status"], string> = {
  running: "执行中",
  waiting_input: "等待中",
  stalled: "可能卡住",
  idle: "空闲",
  offline: "离线",
};

export function FloatingCard({ state }: FloatingCardProps) {
  const project = state.spotlight_project;

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
          <strong>{codexLabels[state.codex.status]}</strong>
        </div>
      </header>

      <div className="agent-card__divider" />

      <div className="agent-card__row">
        <div className="agent-card__signal">
          <span className="status-dot status-dot--running" />
          <strong>{project ? project.stage_label : "当前会话"}</strong>
        </div>
        <div className="agent-card__meta">
          <span>{project ? project.current_task_id || project.current_req_id || "待同步" : state.codex.active_thread_name}</span>
          <strong>{project ? "进行中" : "--"}</strong>
        </div>
      </div>

      <div className="agent-card__subrow">
        <span>最近心跳 {state.codex.heartbeat_at}</span>
        <button className="inline-link-button" type="button" onClick={() => invoke("set_floating_visibility", { visible: false })}>
          隐藏
        </button>
      </div>

      <div className="progress-track">
        <div className="progress-fill" style={{ width: project ? "66%" : "16%" }} />
      </div>
    </section>
  );
}
