import type { ProjectSnapshot, RuntimeState } from "../lib/types";

type StatusCardProps = {
  state: RuntimeState;
  summary: Array<{ label: string; value: number }>;
  project: ProjectSnapshot | null;
};

const codexLabels: Record<RuntimeState["codex"]["status"], string> = {
  running: "执行中",
  waiting_input: "等待中",
  stalled: "可能卡住",
  idle: "空闲",
  offline: "离线",
};

export function StatusCard({ state, summary, project }: StatusCardProps) {
  const activeCount = summary.reduce((total, item) => total + item.value, 0);
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
            <p>{project ? project.name : "Workflow Statusbar"}</p>
          </div>
        </div>
        <div className="agent-card__status">
          <span className={`status-dot status-dot--${state.codex.status}`} />
          <strong>{codexLabels[state.codex.status]}</strong>
        </div>
      </div>

      <div className="agent-card__divider" />

      <div className="agent-card__row">
        <div className="agent-card__signal">
          <span className="status-dot status-dot--running" />
          <strong>当前会话</strong>
        </div>
        <div className="agent-card__meta">
          <span>{state.codex.active_thread_name || "等待活跃线程"}</span>
          <strong>{activeCount}</strong>
        </div>
      </div>

      <div className="agent-card__subrow">
        <span>最近心跳 {state.codex.heartbeat_at}</span>
        <span>{state.codex.source}</span>
      </div>

      <div className="progress-track">
        <div className="progress-fill" style={{ width: `${progressRatio}%` }} />
      </div>
    </section>
  );
}
