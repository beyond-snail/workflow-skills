import type { ProjectSnapshot } from "./types";

export function isWorkflowLinked(project: ProjectSnapshot | null | undefined) {
  return Boolean(project && project.workflow_stage !== "unknown");
}

export function parseTaskProgress(progressLabel?: string | null) {
  if (!progressLabel) {
    return null;
  }

  const match = progressLabel.match(/任务\s*(\d+)\s*\/\s*(\d+)/);
  if (!match) {
    return null;
  }

  const current = Number(match[1]);
  const total = Number(match[2]);
  if (!Number.isFinite(current) || !Number.isFinite(total) || total <= 0) {
    return null;
  }

  const ratio = Math.round((Math.min(current, total) / total) * 100);
  return Math.max(0, Math.min(ratio, 100));
}
