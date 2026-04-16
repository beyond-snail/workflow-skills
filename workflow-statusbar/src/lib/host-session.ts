import type { CodexStatus, HostSession, ProjectSnapshot } from "./types";

function hostPriority(status: CodexStatus) {
  switch (status) {
    case "running":
      return 5;
    case "waiting_input":
      return 4;
    case "stalled":
      return 3;
    case "idle":
      return 2;
    case "offline":
      return 1;
    default:
      return 0;
  }
}

function compareHostSession(left: HostSession, right: HostSession) {
  const priorityDiff = hostPriority(left.status) - hostPriority(right.status);
  if (priorityDiff !== 0) {
    return priorityDiff;
  }

  if (left.updated_at !== right.updated_at) {
    return left.updated_at - right.updated_at;
  }

  if (left.host === "codex" && right.host !== "codex") {
    return 1;
  }
  if (left.host !== "codex" && right.host === "codex") {
    return -1;
  }

  return 0;
}

function rankSessions(sessions: HostSession[]) {
  return sessions.reduce((best, current) => {
    if (!best) {
      return current;
    }
    return compareHostSession(current, best) > 0 ? current : best;
  }, null as HostSession | null);
}

export function selectDisplayHostSession(project?: ProjectSnapshot | null): HostSession | null {
  if (!project) {
    return null;
  }

  const hosts = project.hosts ?? [];
  if (hosts.length === 0) {
    return null;
  }

  if (project.active_host) {
    const preferred = hosts.filter((item) => item.host === project.active_host);
    if (preferred.length > 0) {
      return rankSessions(preferred);
    }
  }

  return rankSessions(hosts);
}
