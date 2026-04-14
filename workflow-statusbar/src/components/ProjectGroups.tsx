import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";
import type { ProjectGroup } from "../lib/types";
import { codexStatusLabels } from "../lib/codex-labels";
import { projectOtherHostSummary, projectPrimaryHostLabel } from "../lib/host-utils";
import { isWorkflowLinked, parseTaskProgress } from "../lib/progress";

type ProjectGroupsProps = {
  groups: ProjectGroup[];
  spotlightPath: string | null;
};

export function ProjectGroups({ groups, spotlightPath }: ProjectGroupsProps) {
  const [expanded, setExpanded] = useState(false);
  const visibleGroups = groups.filter((group) => group.items.length > 0);
  const visibleProjects = visibleGroups
    .flatMap((group) => group.items)
    .filter((project, index, projects) => {
      if (spotlightPath && project.path === spotlightPath) {
        return false;
      }
      return projects.findIndex((candidate) => candidate.path === project.path) === index;
    });

  useEffect(() => {
    const window = getCurrentWindow();
    const hasProjectCards = visibleProjects.length > 0;
    window.setSize(new LogicalSize(392, expanded && hasProjectCards ? 760 : 430)).catch(() => {
      // Ignore resize failures outside the Tauri runtime.
    });
  }, [expanded, visibleProjects.length]);

  if (!visibleGroups.length || !visibleProjects.length) {
    return null;
  }

  return (
    <section className="group-stack">
      {!expanded ? (
        <button className="card card--group-toggle" type="button" onClick={() => setExpanded(true)}>
          <div className="group-toggle__copy">
            <span className="eyebrow">项目分组</span>
            <strong>{visibleGroups.length} 个分组</strong>
          </div>
          <span className="group-toggle__action">展开</span>
        </button>
      ) : null}

      {expanded ? (
        <div className="group-panel">
          <div className="group-toolbar group-toolbar--minimal">
            <button className="inline-link-button" type="button" onClick={() => setExpanded(false)}>
              收起
            </button>
          </div>

          <div className="group-panel__list">
            {visibleProjects.map((project) => {
              const workflowLinked = isWorkflowLinked(project);
              const progressRatio = workflowLinked ? parseTaskProgress(project.progress_label) : null;
              const hostLabel = projectPrimaryHostLabel(project);
              const otherHostSummary = projectOtherHostSummary(project);

              return (
                <button className="card card--group-project" key={project.path} type="button" onClick={() => invoke("open_path", { path: project.path })}>
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
                    </span>
                    <span>{project.auto_resume_enabled ? "自动续跑已开启" : workflowLinked ? project.current_task_status || project.health || "待同步" : "未接入 workflow"}</span>
                  </div>

                  {progressRatio !== null ? (
                    <div className="progress-track">
                      <div
                        className="progress-fill progress-fill--soft"
                        style={{ width: `${progressRatio}%` }}
                      />
                    </div>
                  ) : null}
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
    </section>
  );
}
