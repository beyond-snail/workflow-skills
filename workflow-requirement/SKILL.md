---
name: workflow-requirement
description: 当需要在编码前把 PRD 沉淀为需求池、任务看板和可审计交接材料时使用。
---

# Workflow Requirement Skill (v2.3.0)

把正式 `PRD` 沉淀为可执行、可追溯、可审计的需求材料，并在人工审核门前收口；如仓库支持任务记忆目录，还负责初始化 `task memory` 入口。

## 什么时候用

在以下场景使用：

1. 需要把需求正式入池
2. 需要把 `PRD` 展开成 dated 需求包、任务看板和追溯材料
3. 需要检查某个需求是否已经具备交接到开发执行的前置条件
4. 需要在需求收口后同步更新 `.ai/runtime/project-state.json`，让驾驶舱读取同一份事实源

## 先读什么

只加载当前回合需要的 reference：

1. 始终读取 `references/shared-governance.md`
2. 新仓库适配时读取 `references/repo-adaptation-checklist.md`
3. 小改动或小 bugfix 时读取 `references/lightweight-mode.md`
4. 需要命令示例时读取 `references/cli-recipes.md`
5. 阶段切换判断有疑问时读取 `references/stage-governance-template.md`

## 核心边界

1. 本 skill 只做到“需求整理 -> 文档补齐 -> 自动拆任务 -> 开发交接准备”
2. 默认停在“待人工审核”，不能直接进入编码
3. 不运行构建、测试、发布闸门
4. 若交接检查通过，只输出显式交接结论和推荐开工指令
5. 可以初始化任务记忆，但不负责写入执行期 bugfix 细节

## 不可跳过原则

1. `PRD` 永远高于代码、测试记录和现有实现口径
2. 未经过人工审核，不能从 requirement 直接进入 execution
3. 只要结论、状态或交付物发生变化，就必须同步更新需求池、任务看板和至少一份追溯证据
4. 若仓库存在 `.ai/memory/` 或历史 `.ai/memory/tasks/` 记忆结构，requirement 阶段至少要初始化任务记忆目录和任务索引
5. requirement 收口后必须同步更新 `.ai/runtime/project-state.json`

## 关键入口

优先使用：

```bash
python3 <skill-dir>/scripts/run_requirement_round.py \
  --theme "主题" \
  --summary "一句话需求摘要"
```

必要时再下钻到：

1. `scripts/create_requirement_bundle.py`
2. `scripts/populate_requirement_content.py`
3. `scripts/check_handoff_readiness.py`
4. `scripts/sync_requirement_pool.py`
5. `scripts/sync_task_board.py`
6. `scripts/init_task_memory.py`
7. `scripts/sync_task_index.py`

详细 CLI 示例见 `references/cli-recipes.md`。

## 常用话术

可直接按下面的话术触发：

1. `用 <PRD路径> 生成完整需求包，停在人工审核门`
2. `基于 <PRD路径> 入需求池并自动拆任务`
3. `检查 <REQ-ID> 现在是否满足交接到开发的条件`
4. `这个需求走轻量模式，基于 <PRD路径> 补最小治理材料`

## 主流程

1. 识别仓库事实：
   - 需求池
   - 任务看板
   - 文档根目录
   - README / `00-目录说明.md`
   - 命名规范与状态集合
2. 冻结基线：
   - 找到正式 `PRD`
   - 后续治理全部以该 `PRD` 为准
3. 选择模式：
   - 默认完整模式
   - 若 `features.lightweight_mode = true` 或请求明显是小改动，按 `references/lightweight-mode.md` 缩减交付
4. 创建或刷新需求包：
   - 生成 dated 文档目录
   - 生成或刷新 `REQ-ID` / `TASK-ID`
   - 填充需求池摘要、设计正文、追溯、测试材料
   - 同步任务看板
   - 若支持记忆层，则优先初始化 `.ai/memory/tasks/<task>/` 并更新 `.ai/memory/tasks/index.md`
   - 老仓库无 `.ai/` 时，兼容回退 `.ai/memory/tasks/<task>/`
   - 同步 `.ai/runtime/project-state.json`
5. 同步索引：
   - 更新根 README 或当前需求目录 `00-目录说明.md`
6. 交接检查：
   - 校验必要文档是否存在
   - 校验正文是否不是空骨架
   - 校验任务看板是否已拆解
7. 输出交接结论：
   - 明确当前仍在人工审核门
   - 只给出显式开工建议，不进入 execution

## 轻量模式

轻量模式用于小 bugfix / 小范围逻辑修正 / 单任务需求。

可以省：

1. 完整物理表设计
2. 产品确认清单
3. 流程图与实现对齐说明
4. HTML 总览页

不能省：

1. `PRD` 基线
2. `需求池`
3. `任务看板`
4. 至少一份可追溯文档
5. 人工审核门

细则见 `references/lightweight-mode.md`。

## 输出要求

至少输出：

1. 创建或更新了哪些需求
2. 创建或更新了哪些文档
3. 新增或更新了哪些任务
4. 当前是完整模式还是轻量模式
5. 当前是在“待人工审核”还是仍有缺口
6. 推荐的显式开工指令
7. 若启用了记忆层，明确任务记忆目录位置
8. 若启用了状态骨架，明确 `.ai/runtime/project-state.json` 的更新结果

## 维护说明

1. 版本：`v2.3.0`
2. 本 skill 与 `workflow-bootstrap`、`workflow-execution` 配套使用
3. `references/shared-governance.md` 必须与 execution 中的同名文件保持一致
4. 不再依赖 `_shared`；保持 skill 自包含
5. 新仓库默认以 `.ai/memory` 为记忆主路径
6. 需求状态与任务状态会同步回写到 `.ai/runtime/project-state.json`
