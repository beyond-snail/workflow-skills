---
name: workflow-bootstrap
description: 当需要给一个新仓库或旧仓库补齐 workflow + memory 协作底座时使用。适用于自动初始化 AGENTS、docs/workflow 下的 PROJECT_CONTEXT/开发协作约定/requirements/PRD、.ai 目录骨架、宿主补充文件、任务记忆模板、runtime profile，以及最小 requirements 治理文件。
---

# Workflow Bootstrap Skill (v2.6.3)

初始化 workflow + memory 的最小底座，为后续 `workflow-requirement` / `workflow-execution` 提供稳定目录与状态骨架。

## 什么时候用

1. 新仓库首次接入 workflow 三 skill。
2. 老仓库需要补齐 `.ai/`、`docs/workflow/` 和任务记忆结构。
3. 需要迁移历史 `doc/tasks`、`doc/knowledge` 到 `.ai/memory`。
4. 需要初始化 `.ai/runtime/project-state.json` 作为状态事实源。

## 核心边界

1. 仅做底座初始化，不进入需求拆解或编码执行。
2. 不改业务代码。
3. 自动扫描并生成初稿，但不替代项目特有规则判断。
4. 老仓库优先兼容迁移，尽量保守。
5. 默认可执行迁移与自检，支持 `--dry-run` 预演。
6. 后续阶段由 requirement / execution 增量回写状态。

## 关键入口

优先：

```bash
python3 <skill-dir>/scripts/init_workflow_bootstrap.py --host codex --host claude
```

预演：

```bash
python3 <skill-dir>/scripts/init_workflow_bootstrap.py --host codex --host claude --dry-run
```

健康检查与一致性检查：

```bash
python3 <skill-dir>/scripts/check_workflow_health.py --workspace-root .
python3 <skill-dir>/scripts/check_workflow_consistency.py --workspace-root .
```

统一入口：

```bash
python3 <skill-dir>/scripts/workflow_cli.py init --workspace-root . --host codex --host claude
python3 <skill-dir>/scripts/workflow_cli.py doctor --workspace-root .
python3 <skill-dir>/scripts/workflow_cli.py cons --workspace-root .
```

## 默认产物（最小集）

1. 协作文件：`AGENTS.md`、`docs/workflow/PROJECT_CONTEXT.md`、`docs/workflow/开发协作约定.md`。
2. 需求治理：`docs/workflow/requirements/需求池.md`、`docs/workflow/requirements/任务看板.md`、`docs/workflow/PRD/.gitkeep`。
3. 记忆层：`.ai/memory/context-brief.md`、`.ai/memory/session-briefs/.gitkeep`、`.ai/memory/session-briefs/README.md`、`.ai/memory/tasks/index.md`、`.ai/memory/tasks/_template/*`、`.ai/memory/knowledge/README.md`。
4. 运行态：`.ai/runtime/profile/project-profile.yml`、`.ai/runtime/project-state.json`、`.ai/runtime/cache/legacy-scan.json`、`.ai/runtime/inbox/.gitkeep`、`.ai/runtime/conversations/.gitkeep`。
5. 工具入口：`.ai/bin/workflow`、`wf-init`、`wf-doctor`、`wf-cons`、`wf-req`、`wf-exec`、`wf-arc`。

## 输出要求（最小）

默认只输出 5 行以内：

1. 文件变更摘要。
2. 自动识别结果摘要。
3. 自检告警。
4. 当前状态。
5. 下一步命令。

需要排障或用户要求详细时，再展开文件清单和检测细节。

## 维护说明

1. 版本：`v2.6.3`。
2. 只维护最小底座，不承载需求与执行阶段逻辑。
3. 与 requirement / execution 共享 `project-state.json` 事实源。
