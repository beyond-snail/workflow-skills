---
name: workflow-execution
description: 当任务已通过人工审核且用户显式要求进入实现阶段时使用。
---

# Workflow Execution Skill (v2.6.0)

在人工审核通过且收到显式开工指令后，按任务看板执行实现、验证、证据回写、记忆沉淀与提交收口。

## 什么时候用

1. 用户明确说 `开干` 或等价开工指令。
2. 需求已完成 requirement 收口并通过人工审核。
3. 需要推进下一条任务或继续收口指定任务。
4. 需要在执行后同步更新 `.ai/runtime/project-state.json`。

## 先读什么（按需）

1. 必读：`references/shared-governance.md`。
2. 执行契约疑问：`references/execution-contract.md`。
3. 高风险任务：`references/execution-rules.md`。
4. 阶段切换疑问：`references/stage-transition-checklist.md`。
5. 小修复：`references/lightweight-mode.md`。
6. 命令示例：`references/cli-recipes.md`。

## 核心边界

1. 必须满足“人工审核已完成 + 显式开工”。
2. 不得因 requirement 已拆任务而自动进入 execution。
3. 默认推进到验证、证据、提交、推送、发布闸门；仅在用户明确缩小范围时降级。
4. 遇阻塞可停在 `blocked`，但必须留痕。
5. 有记忆层时，执行前先检索历史，执行后至少回写一种任务记忆。

## 不可跳过原则

1. `PRD` 高于现有实现口径。
2. 无显式开工指令不得进入 execution。
3. 代码/测试/状态变化必须同步证据与文档。
4. bugfix/continuation 不能只改代码不补任务记忆。
5. 收口后必须回写 `.ai/runtime/project-state.json`。

## 关键入口

优先：

```bash
python3 <skill-dir>/scripts/run_execution_round.py --confirm-start --req-id REQ-xxxx --summary "本轮开发摘要"
```

常用统一入口：

```bash
python3 <skill-dir>/scripts/workflow_cli.py exec --workspace-root . --req-id REQ-xxxx --task-id TASK-xxxx
```

预演或缩小范围：

```bash
python3 <skill-dir>/scripts/workflow_cli.py exec --workspace-root . --req-id REQ-xxxx --task-id TASK-xxxx --dry-run
python3 <skill-dir>/scripts/workflow_cli.py exec --workspace-root . --req-id REQ-xxxx --task-id TASK-xxxx --no-commit --no-push --no-release-gate
```

部分需求 + 自测回写（不跑整套）：

```bash
python3 <skill-dir>/scripts/run_execution_round.py \
  --confirm-start \
  --req-id REQ-xxxx \
  --task-id TASK-xxxx \
  --summary "单项能力局部自测回写" \
  --no-default-build-cmd \
  --no-default-test-cmd \
  --self-test-note "数据对账通过" \
  --self-test-note "接口返回符合预期" \
  --focus-keyword FEATURE-123 \
  --no-commit --no-push --no-release-gate
```

说明：默认会在 `测试结果/联调记录` 中同时追加“自动回写记录”并更新对应表格行（`实际/证据/状态`），且会自动生成“步骤级明细表（动作/预期/实际/证据）”。
默认会自动使用 `task_id + task_title + req_id + req_title + summary` 识别目标行，不传 `--focus-keyword` 也可回写；`--focus-keyword` 仅用于人工纠偏。

## 输出要求（最小）

1. 完成了哪些任务。
2. 执行了哪些验证。
3. 更新了哪些证据与任务记忆。
4. 当前模式（完整/轻量）。
5. 当前结论（done/blocked/still doing）。
6. `project-state.json` 更新结果。

## 维护说明

1. 版本：`v2.6.0`。
2. 与 requirement 的 `references/shared-governance.md` 必须一致。
3. 本 skill 聚焦执行收口，不承载需求整理职责。
