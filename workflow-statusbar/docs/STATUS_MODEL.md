# Runtime State Model

前端统一读取如下结构：

```ts
type RuntimeState = {
  codex: {
    status: "running" | "waiting_input" | "stalled" | "idle" | "offline";
    heartbeat_at: string;
    active_thread_id: string;
    active_thread_name: string;
    active_project_path: string;
    source: string;
    confidence: string;
    process_running: boolean;
  };
  projects: ProjectSnapshot[];
  groups: ProjectGroup[];
  summary: Summary;
  spotlight_project: ProjectSnapshot | null;
  updated_at: string;
};
```

## 状态来源

- Codex:
  - `~/.codex/state_5.sqlite`
  - `~/.codex/logs_2.sqlite`
  - `pgrep -f "codex"`
- workflow:
  - 最近线程工作目录向上查找 `.ai/runtime/project-state.json`

## 当前判定

- `running`: 进程存在且最近日志心跳在 20 秒内
- `waiting_input`: 进程存在且最近日志心跳在 90 秒内
- `stalled`: 进程存在但心跳超时
- `idle`: 进程不存在
- `offline`: 无法访问 `~/.codex`
