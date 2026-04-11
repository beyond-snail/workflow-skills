import React from 'react';
import { getProjectDisplayName, riskClass, shortLabel, stageTitle } from '../lib/cockpit';

export function SectionTitle({ title, caption, action }) {
  return (
    <div className="section-title">
      <div>
        {caption ? <p className="section-title__eyebrow">{caption}</p> : null}
        <h3>{title}</h3>
      </div>
      {action ? <div className="section-action">{action}</div> : null}
    </div>
  );
}

export function MetaChip({ label, value, tone, compact = false }) {
  return (
    <div className={`meta-chip ${tone ? `tone-${tone}` : ''} ${compact ? 'is-compact' : ''}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

export function MetricCard({ label, value, accent = false, danger = false }) {
  return (
    <article className={`metric-card ${accent ? 'is-accent' : ''} ${danger ? 'is-danger' : ''}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

export function ProjectTile({ project, onClick, delay }) {
  return (
    <button
      className={`project-tile stage-${project.stage} project-risk-${riskClass(project.risk)}`}
      type="button"
      onClick={onClick}
      style={{ animationDelay: `${delay}ms` }}
    >
      <div className="project-tile__top">
        <span className="project-tile__alias">{project.alias}</span>
        <span className={`risk-pill risk-${riskClass(project.risk)}`}>{project.risk}</span>
      </div>

      <h3>{getProjectDisplayName(project)}</h3>
      <p className="project-tile__summary">{project.summary}</p>

      <div className="project-tile__meta">
        <span>{stageTitle(project.stage)}</span>
        <span>{project.tasks.length} 任务</span>
      </div>

      <div className="project-tile__foot">
        <strong>{project.lastUpdated}</strong>
        <small title={project.sourcePath}>{shortLabel(project.sourcePath) || project.owner}</small>
      </div>
    </button>
  );
}

export function TaskCard({ task, delay }) {
  const metaItems = [task.reqId, task.owner, task.priority].filter(Boolean);

  return (
    <article className="task-card" style={{ animationDelay: `${delay}ms` }}>
      <div className="task-card__top">
        <div>
          <span className="task-id">{task.id}</span>
          <h4>{task.title}</h4>
        </div>
        <span className={`status-pill status-${task.status}`}>{task.statusLabel ?? task.status}</span>
      </div>

      <div className="task-meta">
        {metaItems.map((item) => (
          <span key={item}>{item}</span>
        ))}
      </div>

      <div className="task-body">
        <div className="task-flow__item">
          <span>下一步</span>
          <strong>{task.next}</strong>
        </div>
        <div className="task-flow__item">
          <span>证据</span>
          <strong title={task.evidenceDetail || task.evidence}>{task.evidence}</strong>
        </div>
      </div>

      {task.blocker ? <p className="task-blocker">{task.blocker}</p> : null}
    </article>
  );
}
