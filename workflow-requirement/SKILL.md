---
name: workflow-requirement
description: 当需要在编码前把 PRD 沉淀为需求池、任务看板和可审计交接材料时使用。
---

# Workflow Requirement Skill (v2.6.1)

将正式 `PRD` 收敛为可执行、可追溯、可审计的需求材料，并停在人工审核门。

## 什么时候用

1. 需求入池与需求包建立。
2. 需要把 PRD 拆成任务看板。
3. 需要验证是否具备交接到执行阶段的条件。
4. 需要同步更新 `.ai/runtime/project-state.json`。

## 先读什么（按需）

1. 必读：`references/shared-governance.md`。
2. 新仓库适配：`references/repo-adaptation-checklist.md`。
3. 小改动：`references/lightweight-mode.md`。
4. 命令示例：`references/cli-recipes.md`。
5. 阶段判断争议：`references/stage-governance-template.md`。

## 核心边界

1. 只做需求治理与交接准备，不进入编码。
2. 默认停在人工审核门。
3. 不运行构建、测试、发布闸门。
4. 可初始化任务记忆，但不写执行期问题细节。

## 不可跳过原则

1. `PRD` 是最高基线。
2. 未人工审核，不得进入 execution。
3. 状态变化必须同步需求池、任务看板与追溯材料。
4. 存在记忆层时，至少初始化任务记忆目录和索引。
5. 收口后必须回写 `.ai/runtime/project-state.json`。

## 关键入口

优先：

```bash
python3 <skill-dir>/scripts/run_requirement_round.py --theme "主题" --summary "一句话摘要"
```

常用统一入口：

```bash
python3 <skill-dir>/scripts/workflow_cli.py req --workspace-root . --theme "主题"
```

轻量或预演：

```bash
python3 <skill-dir>/scripts/workflow_cli.py req --workspace-root . --theme "主题" --dry-run
python3 <skill-dir>/scripts/workflow_cli.py req --workspace-root . --theme "主题" --skip-content-population --skip-handoff-check
```

## 输出要求（最小）

默认只输出 5 行以内：

1. 需求/任务变更摘要。
2. 关键文档路径。
3. 当前结论。
4. 下一步人工审核指令。
5. `project-state.json` / 任务记忆状态。

需要审计、交接或用户要求详细时，再展开完整清单。

## 维护说明

1. 版本：`v2.6.1`。
2. 与 execution 的 `references/shared-governance.md` 必须保持一致。
3. 新仓库默认 `.ai/memory` 为记忆主路径。
