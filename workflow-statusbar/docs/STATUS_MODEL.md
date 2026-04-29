# Runtime State Model

前端统一读取如下结构。当前主模型是 `hosts` / `active_host` / `other_host_summary`；`codex` 字段仍保留，是为了兼容旧 UI 和旧调用方，不代表当前状态模型仍是单 Host 模型。

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
  active_host: "codex" | "claude" | null;
  other_host_summary: string;
  hosts: HostSession[];
  projects: ProjectSnapshot[];
  groups: ProjectGroup[];
  summary: Summary;
  spotlight_project: ProjectSnapshot | null;
  updated_at: string;
};

type HostSession = {
  host: "codex" | "claude";
  status: "running" | "waiting_input" | "stalled" | "idle" | "offline";
  heartbeat_at: string;
  thread_id: string;
  thread_name: string;
  project_path: string;
  last_message_role: string;
  last_message_text: string;
  process_running: boolean;
  source: string;
  confidence: string;
  token_total: number;
  token_input: number;
  token_output: number;
  token_reasoning: number;
  auto_resume_enabled: boolean;
  follow_up_prompted: boolean;
  updated_at: number;
};
```

## 状态来源

- Codex:
  - `~/.codex/state_5.sqlite`
  - `~/.codex/logs_2.sqlite`
  - `pgrep -f "codex"`
- Claude:
  - `~/.claude/history.jsonl`
  - `~/.claude/projects/*/*.jsonl`
  - `pgrep -f "claude"`
- workflow:
  - 最近线程工作目录向上查找 `.ai/runtime/project-state.json`

## 当前判定

- `running`: 进程存在且最近日志心跳在 20 秒内
- `waiting_input`: 进程存在且最近日志心跳在 90 秒内
- `stalled`: 进程存在但心跳超时
- `idle`: 进程不存在
- `offline`: 对应 Host 的本地状态源不可访问或不可用

多 Host 情况下，主 Host 选择优先级为：

1. `running`
2. `waiting_input`
3. `stalled`
4. 最近活跃时间最新
5. 完全同分时使用 `codex` 作为稳定兜底

卡片 UI 仅展示主 Host，并通过 `other_host_summary` 提供轻量提示（例如：`另有 Claude 会话`）。
