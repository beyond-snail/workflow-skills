# workflow-statusbar 多 Host 监控升级开发任务清单

## 目标

将 `workflow-statusbar` 从当前的 `Codex + workflow` 监控器，升级为支持多 AI Host 的项目监控器。第一阶段只支持 `Codex` 与 `Claude`，并保持现有卡片 UI 基本不变。

## 范围

### 本轮要做

- 抽象统一 Host 监控模型。
- 保留现有 `Codex` 监控能力。
- 新增 `Claude` 只读监控能力。
- 同一项目存在多个 Host 会话时，只展示一个主 Host，并增加轻量描述。
- 通知与状态文案从固定 `Codex` 改为动态 Host。

### 本轮不做

- IDE 识别升级。
- 独立悬浮卡启用。
- 双 Host 详情展开面板。
- Claude 自动续跑。
- 大幅改版现有卡片视觉结构。

## UI 约定

现有卡片结构保持不变，标题副文案从固定 `Codex 监控` 改为动态 Host 文案。

单 Host：

```text
通关小四门                         执行中
Codex 监控
```

或：

```text
通关小四门                         等待中
Claude 监控
```

多 Host：

```text
通关小四门                         执行中
Claude 监控 · 另有 Codex 会话
```

主 Host 选择规则：

1. `running`
2. `waiting_input`
3. `stalled`
4. 最近心跳最新
5. 默认 `codex`

## 状态说明

- `todo`：未开始。
- `doing`：执行中。
- `blocked`：阻塞，需要决策或外部信息。
- `done`：已完成并验证。
- `deferred`：明确延期。

## 任务清单

| 编号 | 优先级 | 状态 | 任务 | 产出 | 验收标准 |
| --- | --- | --- | --- | --- | --- |
| TASK-001 | P0 | done | 抽象 Host 类型与状态枚举 | 后端新增 `HostKind`、`HostSession`、通用状态字段 | 只接 Codex 时运行态输出不回归 |
| TASK-002 | P0 | done | 为 `RuntimeState` 增加多 Host 字段 | `RuntimeState` 支持 `hosts` / `active_host` 等字段 | 前端可读取主 Host 与其他 Host 摘要 |
| TASK-003 | P0 | done | 为 `ProjectSnapshot` 增加项目级 Host 聚合字段 | `ProjectSnapshot` 支持 `hosts[]`、`active_host`、`other_host_summary` | 单项目可承载 Codex 与 Claude 会话 |
| TASK-004 | P0 | done | 保留旧 `codex_*` 兼容字段 | 新模型回填旧字段 | 现有组件不需要一次性大改 |
| TASK-005 | P0 | done | 实现 Codex Adapter 包装层 | 现有 Codex 读取逻辑封装为统一 Host 输出 | Codex 监控结果与当前一致 |
| TASK-006 | P0 | done | 实现主 Host 选择函数 | `select_active_host` 或等价函数 | 多 Host mock 数据下选择结果符合规则 |
| TASK-007 | P0 | done | 前端标题副文案动态化 | `Codex 监控` 改为动态 `Codex/Claude 监控` | 单 Host UI 基本不变 |
| TASK-008 | P0 | done | 前端多 Host 轻量提示 | 展示 `另有 Codex 会话` / `另有 Claude 会话` | 双 Host 时只增加一段轻量文案 |
| TASK-009 | P1 | todo | 调研 Claude 本地状态源 | 记录 Claude 会话、消息、活跃时间、项目路径来源 | 明确可读取字段与不可读取字段 |
| TASK-010 | P1 | todo | 实现 Claude Adapter 只读采集 | 输出统一 `HostSession` | Claude 能被识别为 `running/waiting_input/stalled/idle/offline` |
| TASK-011 | P1 | todo | Claude 会话绑定 workflow 项目 | 按项目路径归属到 `ProjectSnapshot.hosts[]` | Claude 单独运行时项目卡可展示 Claude |
| TASK-012 | P1 | todo | Codex + Claude 同项目聚合 | 同项目双 Host 聚合与主 Host 选择 | 主 Host 正确，副 Host 轻量提示正确 |
| TASK-013 | P2 | todo | 告警文案 Host 动态化 | 通知标题与正文包含动态 Host | Codex/Claude 告警文案都自然 |
| TASK-014 | P2 | todo | 多 Host 告警去重 | 非主 Host 默认不重复轰炸 | 双 Host 状态变化不会重复提醒 |
| TASK-015 | P2 | todo | 扩展 runtime debug 日志 | debug 输出 Host 采集、绑定、选择过程 | 出问题时能定位哪个 Host 或项目绑定异常 |
| TASK-016 | P2 | todo | 回归验证单 Codex 场景 | 测试记录 | 当前已可用能力不回归 |
| TASK-017 | P2 | todo | 回归验证单 Claude 场景 | 测试记录 | Claude 可独立展示状态 |
| TASK-018 | P2 | todo | 回归验证 Codex + Claude 同项目场景 | 测试记录 | 主 Host 与轻量提示符合预期 |
| TASK-019 | P2 | todo | 回归验证 Codex + Claude 不同项目场景 | 测试记录 | 不同项目互不串线 |
| TASK-020 | P2 | todo | 更新架构与状态模型文档 | `STATUS_MODEL.md` / `架构与功能说明.md` 更新 | 文档与最终实现一致 |

## 执行记录

### 2026-04-14

- 创建多 Host 监控升级开发任务清单。
- 明确本轮不处理 IDE 识别和悬浮卡。
- 明确 UI 保持现有卡片结构，仅增加动态 Host 文案和多 Host 轻量提示。
- TASK-001：新增 Rust/TypeScript 通用 Host 类型，先不改变现有 Codex 运行逻辑。
- TASK-002：为 `RuntimeState` 增加多 Host 字段，先由 Codex 全局状态回填。
- TASK-003：为 `ProjectSnapshot` 增加项目级 `hosts[]`、`active_host` 和轻量描述字段，先由 Codex 项目运行态回填。
- TASK-004：新增兼容层，由 `hosts[]` 回填旧 `codex_*` 字段，降低前端切换风险。
- TASK-005：完成 Codex 采集包装层，新增统一构建函数（全局态/项目态），确保行为不变。
- TASK-006：新增主 Host 选择逻辑（按状态优先级 + 最近活跃 + Codex 兜底）并补充单元测试。
- TASK-007：主卡/焦点卡/分组卡/悬浮卡支持动态 Host 文案。
- TASK-008：在现有卡片结构下增加多 Host 轻量描述（`另有 xxx 会话`）。

## 后续更新规则

每次执行开发后，需要同步更新本文档：

- 更新对应任务状态。
- 在执行记录中追加日期、完成内容、验证结果。
- 如发现新风险或新任务，追加到任务清单末尾，不覆盖历史记录。
- 如任务延期，将状态改为 `deferred` 并说明原因。
