import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";
import type { ProjectGroup } from "../lib/types";

type ProjectGroupsProps = {
  groups: ProjectGroup[];
};

export function ProjectGroups({ groups }: ProjectGroupsProps) {
  const [expanded, setExpanded] = useState(false);
  const visibleGroups = groups.filter((group) => group.items.length > 0);
  const visibleProjects = visibleGroups.flatMap((group) => group.items);

  useEffect(() => {
    const window = getCurrentWindow();
    window.setSize(new LogicalSize(392, expanded ? 760 : 430)).catch(() => {
      // Ignore resize failures outside the Tauri runtime.
    });
  }, [expanded]);

  if (!visibleGroups.length) {
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
            {visibleProjects.map((project) => (
              <button className="card card--group-project" key={project.path} type="button" onClick={() => invoke("open_path", { path: project.path })}>
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
                  <span>最近同步 {project.last_sync_at}</span>
                  <span>{project.current_task_status || project.health || "待同步"}</span>
                </div>

                <div className="progress-track">
                  <div
                    className="progress-fill progress-fill--soft"
                    style={{
                      width:
                        project.workflow_stage === "execution"
                          ? "68%"
                          : project.workflow_stage === "requirement"
                            ? "44%"
                            : project.workflow_stage === "bootstrap"
                              ? "18%"
                              : "100%",
                    }}
                  />
                </div>
              </button>
            ))}
          </div>
        </div>
      ) : null}
    </section>
  );
}
