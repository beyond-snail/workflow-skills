---
name: workflow-execution
description: 当任务已通过人工审核且用户显式要求进入实现阶段时使用。
---

# Workflow Execution Skill (v2.4.0)

在人工审核通过并收到显式开工指令后，按任务看板实现、验证、回写证据、同步任务记忆、提交并跑发布闸门。

## 什么时候用

在以下场景使用：

1. 用户在人工审核通过后显式说 `开干`
2. 已明确进入执行阶段，需要推进下一条任务
3. 需要对某条已进入执行态的任务做开发收口

## 先读什么

只加载当前回合需要的 reference：

1. 始终读取 `references/shared-governance.md`
2. 实施顺序有疑问时读取 `references/execution-contract.md`
3. 数据库类 / 批处理类 / 高风险收口任务时读取 `references/execution-rules.md`
4. 阶段切换有疑问时读取 `references/stage-transition-checklist.md`
5. 小范围修复时读取 `references/lightweight-mode.md`
6. 需要命令示例时读取 `references/cli-recipes.md`

## 核心边界

1. 本 skill 必须以“人工审核已完成 + 用户显式开工”为起点
2. 不得因为 requirement 已拆好任务就自动进入执行
3. 一旦进入执行，默认自动推进到验证、证据回写、提交、发布闸门，除非用户明确缩小范围
4. 若遇阻塞，可以停在 `blocked`，但不能不留痕
5. 若仓库存在 `.ai/memory/` 或历史 `.ai/memory/tasks/` / `.ai/memory/knowledge/` 记忆层，执行前必须先检索历史，执行后必须回写至少一类任务记忆
6. archived 目录仅作为回退检索来源，不能抢占活跃任务记忆

## 不可跳过原则

1. `PRD` 永远高于代码、测试记录和现有实现口径
2. 没有显式开工指令，不能进入 execution
3. 代码、测试、状态结论变化时，必须同步更新证据和相关文档
4. bugfix / continuation 回合不能只补代码不补 `verify.md` / `inbox.md` 等任务记忆
5. 新仓库默认优先读取 `.ai/memory`，老仓库兼容回退到 `.ai/memory/tasks` / `.ai/memory/knowledge`

## 关键入口

优先使用：

```bash
python3 <skill-dir>/scripts/run_execution_round.py \
  --confirm-start \
  --req-id REQ-xxxx \
  --summary "本轮开发摘要"
```

必要时再下钻到：

1. `scripts/select_next_task.py`
2. `scripts/update_task_status.py`
3. `scripts/record_task_evidence.py`
4. `scripts/generate_commit_message.py`
5. `scripts/run_release_gate.py`
6. `scripts/load_memory_context.py`
7. `scripts/append_memory_inbox.py`
8. `scripts/record_task_verify.py`
9. `scripts/record_task_issue.py`
10. `scripts/record_task_decision.py`
11. `scripts/promote_task_knowledge.py`
12. `scripts/archive_task_memory.py`

详细 CLI 示例见 `references/cli-recipes.md`。

## 常用话术

可直接按下面的话术触发：

1. `开干`
2. `开干，推进下一条任务`
3. `开干，只改代码不提交`
4. `开干，轻量模式，做最小验证和证据回写`
5. `继续收口 <REQ-ID> / <TASK-ID>`
6. `继续处理上次那个 bug`

## 主流程

1. 确认开工门：
   - 人工审核已经完成
   - 用户已显式说 `开干` / `执行开发` / 等价指令
2. 识别仓库事实：
   - 任务看板
   - 需求池
   - 任务索引
   - 项目知识
   - 校验命令
   - 发布闸门命令
3. 选择模式：
   - 默认完整模式
   - 若 `features.lightweight_mode = true` 或任务明显是小修复，按 `references/lightweight-mode.md` 缩减动作
   - 对历史问题续做，优先进入 `bugfix` / `continuation` 模式
4. 选择任务并改状态：
   - 优先当前 `doing`
   - 否则最早 `todo`
   - 开始前切到 `doing`
5. 实现与验证：
   - 先检索历史任务和知识
   - 先对齐 `PRD`
   - 再实现代码
   - 跑最小必要或完整验证
   - 同步更新相关文档、证据和任务记忆
6. 提交与发布：
   - 默认提交 / 推送 / 发布闸门
   - 若用户明确说“只改代码不提交”或“只实现不跑闸门”，按要求缩小范围
7. 输出收口结论：
   - done / blocked / still doing
   - 验证结果
   - 证据位置
   - 发布闸门结果
8. 若任务已彻底收口，可追加 `--archive-task-memory` 完成任务记忆归档
9. 若命中 bugfix/continuation、根因/决策或复用信号，应优先采纳 execution 的 issue/decision/knowledge 建议

## 轻量模式

轻量模式用于单任务、小范围代码修复、无复杂阶段切换的执行任务。

可以省：

1. 全量阶段切换检查
2. 与当前任务无关的扩展文档刷新
3. 非关键测试命令
4. 自动推送或发布闸门（仅当用户明确缩小范围）

不能省：

1. `PRD` 基线
2. 显式开工门
3. 正确的任务状态
4. 至少一份可追溯证据
5. 阻塞原因与替代验证说明

细则见 `references/lightweight-mode.md`。

## 输出要求

至少输出：

1. 完成了哪些任务
2. 跑了哪些验证
3. 更新了哪些证据
4. 更新了哪些任务记忆 / 项目知识
5. 当前是完整模式还是轻量模式
6. 当前结果是 `done`、`blocked` 还是仍需继续
7. 如涉及需求对齐，明确本次 `PRD` 基线和剩余未对齐点

## 维护说明

1. 版本：`v2.4.0`
2. 本 skill 与 `workflow-bootstrap`、`workflow-requirement` 配套使用
3. `references/shared-governance.md` 必须与 requirement 中的同名文件保持一致
4. 不再依赖 `_shared`；保持 skill 自包含
