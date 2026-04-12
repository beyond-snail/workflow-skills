import type { CodexStatus } from "./types";

export const codexStatusLabels: Record<CodexStatus, string> = {
  running: "执行中",
  waiting_input: "等待回复",
  stalled: "可能卡住",
  idle: "空闲",
  offline: "离线",
};
