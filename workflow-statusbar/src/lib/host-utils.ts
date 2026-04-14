import type { HostKind, ProjectSnapshot, RuntimeState } from "./types";

function hostLabel(host?: HostKind | null) {
  if (host === "claude") {
    return "Claude";
  }
  return "Codex";
}

export function runtimePrimaryHostLabel(state: RuntimeState) {
  return hostLabel(state.active_host);
}

export function projectPrimaryHostLabel(project?: ProjectSnapshot | null) {
  return hostLabel(project?.active_host);
}

export function runtimeOtherHostSummary(state: RuntimeState) {
  return state.other_host_summary || "";
}

export function projectOtherHostSummary(project?: ProjectSnapshot | null) {
  return project?.other_host_summary || "";
}
