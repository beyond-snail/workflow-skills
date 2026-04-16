---
name: workflow-bootstrap
description: 当需要给一个新仓库或旧仓库补齐 workflow + memory 协作底座时使用。适用于自动初始化 AGENTS、docs/workflow 下的 PROJECT_CONTEXT/开发协作约定/requirements/PRD、.ai 目录骨架、宿主补充文件、任务记忆模板、runtime profile，以及最小 requirements 治理文件。
---

# Workflow Bootstrap Skill (v2.6.0)

初始化一个仓库的最小 workflow + memory 底座，让后续 `workflow-requirement` 和 `workflow-execution` 有稳定的目录和配置可用。

当前版本的 `wf-init` 默认自动扫描仓库状态，不要求用户先说明是新项目还是老项目。对老项目会自动生成画像、识别旧底座资产、产出迁移建议，并尽量以保守方式兼容旧目录。

## 什么时候用

在以下场景使用：

1. 新仓库首次接入 workflow 三 skill
2. 老仓库需要补齐 `.ai/` 目标目录和最小记忆结构，并自动迁移历史记忆目录
3. 需要自动生成 `docs/workflow/PROJECT_CONTEXT.md`、`docs/workflow/开发协作约定.md`、宿主补充文件和 `.ai/runtime/profile/project-profile.yml`
4. 需要在底座初始化时同步生成 `.ai/runtime/project-state.json` 骨架，作为后续状态同步的单一事实源
5. 需要自动识别老项目状态并生成 `docs/workflow/legacy-analysis.md` 与 `.ai/runtime/cache/legacy-scan.json`

## 推荐提示词

对话里推荐直接这样说：

1. `用 workflow-bootstrap 初始化这个仓库`
2. `给这个老项目补齐 workflow 三 skill 底座`
3. `帮我把这个仓库接入 workflow + memory`
4. `初始化这个项目的 AGENTS、docs/workflow 和 .ai 目录`
5. `给这个仓库跑一遍 workflow bootstrap，并做健康检查`

## 核心边界

1. 只做最小初始化，不默认生成培训手册、接入方案、升级清单
2. 默认自动迁移历史 `doc/tasks/`、`doc/knowledge/` 到 `.ai/memory/`
3. 不改业务代码，不创建需求包，不进入 execution
4. 允许在旧仓库中建立 `.ai/` 骨架并保持旧目录兼容
5. 会自动扫描仓库并填充可用初稿，但不替代项目特有规则判断
6. `wf-init` 默认自动执行“状态识别 -> 老项目画像 -> 兼容迁移/补齐 -> 生成底座”

## 默认生成

1. 根目录：
   - `AGENTS.md`（若缺失）
2. `docs/workflow/`
   - `PROJECT_CONTEXT.md`（若缺失）
   - `开发协作约定.md`（若缺失）
   - `PRD/.gitkeep`
3. `.ai/governance/`
   - `codex-host.md`
   - `claude-host.md`
4. `.ai/memory/`
   - `tasks/index.md`
   - `tasks/_template/*`
   - `knowledge/README.md`
5. `.ai/runtime/`
   - `profile/project-profile.yml`
   - `cache/.gitkeep`
   - `state/.gitkeep`
   - `project-state.json`
6. `.ai/bin/`
   - `workflow`
   - `wf-init`
   - `wf-doctor`
   - `wf-cons`
   - `wf-req`
   - `wf-exec`
   - `wf-arc`
7. `docs/workflow/requirements/`
   - `需求池.md`（若缺失）
   - `任务看板.md`（若缺失）
8. `docs/workflow/`
   - `legacy-analysis.md`（自动刷新）
9. `.ai/runtime/cache/`
   - `legacy-scan.json`（自动刷新）

## 自动迁移与自检

bootstrap 默认还会尝试：

1. 迁移 `doc/tasks/` -> `.ai/memory/tasks/`
2. 迁移 `doc/knowledge/` -> `.ai/memory/knowledge/`
3. 重写 Markdown 中的旧路径引用
4. 对初始化结果执行最小自检并输出 warning/info
5. 初始化 `archived/` 目录，供后续任务记忆归档使用
6. 自动识别当前仓库属于 `fresh / legacy_bootstrap / current / partial_current / mixed_legacy_current`

## 自动识别内容

bootstrap 会自动尝试识别：

1. 项目名称
2. 语言与构建工具
3. 默认编译命令
4. 默认测试命令
5. `docs_root`
6. `PRD` 目录
7. 代码主目录
8. 模块目录
9. 常见分层提示（如 `controller/service/mapper`）
10. 老项目业务域初稿
11. 关键接口链路初稿
12. 旧底座资产与迁移建议
13. 文档证据清单（README / PRD / 设计 / 测试 / 历史治理文档）

并把这些结果写入：

1. `docs/workflow/PROJECT_CONTEXT.md`
2. `docs/workflow/开发协作约定.md`
3. `.ai/runtime/profile/project-profile.yml`
4. `.ai/runtime/project-state.json`
5. `docs/workflow/legacy-analysis.md`
6. `.ai/runtime/cache/legacy-scan.json`
7. `PROJECT_CONTEXT.md` 中的业务域摘要与关键链路索引

## 可选参数

1. `--host codex`
2. `--host claude`
3. 可多次传入
4. `--force-host-files` 可覆盖宿主文件骨架
5. `--force-context` 可覆盖 `docs/workflow/PROJECT_CONTEXT.md`
6. `--force-governance` 可覆盖 `docs/workflow/开发协作约定.md`
7. `--force-profile` 可覆盖 `.ai/runtime/profile/project-profile.yml`
8. `--skip-legacy-migration` 可跳过旧记忆目录迁移

## 关键入口

优先使用：

```bash
python3 <skill-dir>/scripts/init_workflow_bootstrap.py --host codex --host claude
```

需要预演时：

```bash
python3 <skill-dir>/scripts/init_workflow_bootstrap.py --host codex --host claude --dry-run
```

## 输出要求

至少输出：

1. 创建了哪些目录和文件
2. 哪些文件因已存在而跳过
3. 自动识别出的语言、构建工具、命令、PRD 目录、代码主目录和模块目录
4. 当前 profile 落点
5. 迁移与自检结果
6. 后续如何接 `workflow-requirement` / `workflow-execution`
7. 自动识别到的 workflow 状态、业务域数量、关键接口链路数量

## 健康检查入口

可独立执行：

```bash
python3 <skill-dir>/scripts/check_workflow_health.py --workspace-root .
python3 <skill-dir>/scripts/check_workflow_consistency.py --workspace-root .
```

统一短命令入口：

```bash
python3 <skill-dir>/scripts/workflow_cli.py health --workspace-root .
python3 <skill-dir>/scripts/workflow_cli.py doctor --workspace-root .
python3 <skill-dir>/scripts/workflow_cli.py consistency --workspace-root .
python3 <skill-dir>/scripts/workflow_cli.py cons --workspace-root .
python3 <skill-dir>/scripts/workflow_cli.py bootstrap --workspace-root . --host codex --host claude
python3 <skill-dir>/scripts/workflow_cli.py init --workspace-root . --host codex --host claude
python3 <skill-dir>/scripts/workflow_cli.py requirement --workspace-root . --theme "主题" --dry-run
python3 <skill-dir>/scripts/workflow_cli.py req --workspace-root . --theme "主题" --dry-run
python3 <skill-dir>/scripts/workflow_cli.py requirement --workspace-root . --theme "主题" --skip-content-population --skip-handoff-check
python3 <skill-dir>/scripts/workflow_cli.py execution --workspace-root . --req-id REQ-xxxx --task-id TASK-xxxx --dry-run
python3 <skill-dir>/scripts/workflow_cli.py exec --workspace-root . --req-id REQ-xxxx --task-id TASK-xxxx --dry-run
python3 <skill-dir>/scripts/workflow_cli.py execution --workspace-root . --req-id REQ-xxxx --task-id TASK-xxxx --no-commit --no-push --no-release-gate
python3 <skill-dir>/scripts/workflow_cli.py archive --workspace-root . --task-id TASK-xxxx --task-dir .ai/memory/tasks/xxxx --dry-run
python3 <skill-dir>/scripts/workflow_cli.py arc --workspace-root . --task-id TASK-xxxx --task-dir .ai/memory/tasks/xxxx --dry-run
```

仓库内短命令入口：

```bash
.ai/bin/workflow doctor --workspace-root .
.ai/bin/workflow cons --workspace-root .
.ai/bin/workflow init --workspace-root . --host codex --host claude
.ai/bin/workflow req --workspace-root . --theme "主题" --dry-run
.ai/bin/workflow exec --workspace-root . --req-id REQ-xxxx --task-id TASK-xxxx --dry-run
.ai/bin/workflow arc --workspace-root . --task-id TASK-xxxx --task-dir .ai/memory/tasks/xxxx --dry-run
```

更短别名：

```bash
.ai/bin/wf-doctor --workspace-root .
.ai/bin/wf-cons --workspace-root .
.ai/bin/wf-init --workspace-root . --host codex --host claude
.ai/bin/wf-req --workspace-root . --theme "主题" --dry-run
.ai/bin/wf-exec --workspace-root . --req-id REQ-xxxx --task-id TASK-xxxx --dry-run
.ai/bin/wf-arc --workspace-root . --task-id TASK-xxxx --task-dir .ai/memory/tasks/xxxx --dry-run
```

## 维护说明

1. 版本：`v2.6.0`
2. 这是 workflow 家族的仓库初始化入口
3. 生成的 profile 默认把 `.ai/memory` 作为任务记忆和知识主路径
4. bootstrap 同时生成 `.ai/runtime/project-state.json` 的最小骨架，后续由 requirement / execution 增量回写
