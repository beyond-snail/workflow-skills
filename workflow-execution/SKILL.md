---
name: workflow-execution
description: 当任务已通过人工审核且用户显式要求进入实现阶段时使用。
---

# Workflow Execution Skill (v2.6.3)

在人工审核通过且收到显式开工指令后，按任务看板执行实现、验证、短证据回写、记忆沉淀与提交收口。

## 什么时候用

1. 用户明确说 `开干` 或等价开工指令。
2. 需求已完成 requirement 收口并通过人工审核。
3. 需要推进下一条任务或继续收口指定任务。
4. 需要在执行后同步更新 `.ai/runtime/project-state.json` 与 `.ai/memory/context-brief.md`。

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
3. 默认写回采用 `compact`，只保留可续接状态、验证摘要、风险和下一步。
4. 遇阻塞可停在 `blocked`，但必须留痕。
5. 详细测试报告、验收材料、表格行细节、inbox 自动收件仅在 `audit` 模式或用户明确要求时执行。

## 不可跳过原则

1. `PRD` 高于现有实现口径。
2. 无显式开工指令不得进入 execution。
3. 代码/测试/状态变化必须同步证据与文档。
4. bugfix/continuation 不能只改代码不补任务记忆。
5. 收口后必须回写 `.ai/runtime/project-state.json` 和短版 `.ai/memory/context-brief.md`。

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

详细审计/正式验收：

```bash
python3 <skill-dir>/scripts/run_execution_round.py \
  --confirm-start \
  --req-id REQ-xxxx \
  --task-id TASK-xxxx \
  --summary "正式验收回写" \
  --writeback audit
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

说明：默认 `--writeback compact` 只写测试结果一行摘要、`verify.md` 一行摘要、`project-state.json` 和短版 `context-brief.md`。`--writeback audit` 才会追加步骤级明细表、联调/验收证据块、表格行细节和 bugfix/continuation 的 inbox 自动收件。
当用户明确说“正式验收”“发布前回归”“审计留痕”“完整回写”“写测试报告”“写联调记录”“生成验收材料”时，使用 `--writeback audit`。

## 输出要求（最小）

默认只输出 5 行以内：

1. 任务结果。
2. 验证结果。
3. 回写模式与关键文件。
4. 提交/推送/闸门状态。
5. 未覆盖风险或下一步。

阻塞、失败、审计模式或用户要求详细时，再展开证据和影响范围。

## 维护说明

1. 版本：`v2.6.3`。
2. 与 requirement 的 `references/shared-governance.md` 必须一致。
3. 本 skill 聚焦执行收口，不承载需求整理职责。
