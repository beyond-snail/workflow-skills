# 三个 skill 分析

## 1. workflow-bootstrap

### 中文名称建议

仓库底座初始化

### 作用

它负责把一个仓库初始化成“可治理”的状态。

### 解决的问题

很多项目一开始没有统一的协作规则、没有稳定的项目上下文、没有记忆层和 profile，后续 requirement 和 execution 就会失去共同事实基础。

### 主入口

- `scripts/init_workflow_bootstrap.py`
- `scripts/check_workflow_health.py`
- `scripts/workflow_cli.py`

### 它做什么

- 自动识别语言、构建工具、测试命令、PRD 目录、源码目录
- 自动识别新项目 / 老项目状态
- 生成 `AGENTS.md`、`docs/workflow/PROJECT_CONTEXT.md`
- 创建 `.ai/governance/`、`.ai/memory/`、`.ai/runtime/profile/`
- 生成 `.ai/runtime/project-state.json` 最小骨架
- 生成 `docs/workflow/legacy-analysis.md` 与 `.ai/runtime/cache/legacy-scan.json`
- 提供 workflow 的统一命令入口
- 做健康检查，找出缺失项和 legacy 残留

### 典型产物

- 仓库协作规则
- 项目上下文
- 任务记忆骨架
- profile 配置
- `docs/workflow/requirements/` 的治理骨架
- 老项目画像与接口链路初稿
- 统一运行态骨架 `project-state.json`

### 典型判断点

- 项目是不是刚接入 workflow
- 当前仓库是 `fresh / legacy_bootstrap / current / partial_current / mixed_legacy_current` 哪一类
- 旧仓库有没有残留 `doc/tasks`、`doc/knowledge`
- 老项目里能不能自动提炼业务域、关键接口链路和历史文档证据
- `project-profile.yml` 是否能正确推导 build/test 命令

### 边界

- 不做需求整理
- 不做任务执行
- 不改业务代码

---

## 2. workflow-requirement

### 中文名称建议

需求治理

### 作用

它负责把正式 PRD 沉淀成需求池、任务看板和可交接材料。

### 解决的问题

PRD 如果不先治理，就会直接滑向实现，导致需求边界、任务拆分和证据链都不稳定。

### 主入口

- `scripts/run_requirement_round.py`

### 关键辅助脚本

- `scripts/create_requirement_bundle.py`
- `scripts/populate_requirement_content.py`
- `scripts/check_handoff_readiness.py`
- `scripts/sync_requirement_pool.py`
- `scripts/sync_task_board.py`
- `scripts/sync_prd_trace.py`
- `scripts/init_task_memory.py`
- `scripts/sync_task_index.py`
- `scripts/markdown_to_html.py`

### 它做什么

- 识别需求池、任务看板、PRD 目录和仓库事实
- 生成 `REQ-ID`、`TASK-ID`
- 创建 dated 需求包
- 填充需求池、任务看板、设计文档、PRD 追溯和测试材料
- 读取 `.ai/runtime/cache/legacy-scan.json` 并命中老项目上下文
- 初始化 task memory 并同步任务索引
- 回写 `.ai/runtime/project-state.json`
- 做交接检查

### 典型产物

- 需求池记录
- 任务看板拆解
- 设计目录
- task memory
- 交接结论
- legacy context 命中摘要
- 更新后的 `project-state.json`

### 典型判断点

- PRD 是否已经冻结
- 是否需要轻量模式
- 是否命中已有业务域、接口链路或历史治理文档
- 是否已经具备开发前置材料

### 边界

- 会拆任务，但不进入编码
- 默认停在人工审核门
- 不运行构建、测试、发布闸门

---

## 3. workflow-execution

### 中文名称建议

执行收口

### 作用

它负责在人工审核通过后推进实现、验证、证据回写、提交和发布闸门。

### 解决的问题

很多任务不是“写完代码就结束”，而是要保证实现、验证、证据、状态和记忆都同步收口。

### 主入口

- `scripts/run_execution_round.py`

### 关键辅助脚本

- `scripts/select_next_task.py`
- `scripts/update_task_status.py`
- `scripts/load_memory_context.py`
- `scripts/record_task_evidence.py`
- `scripts/record_task_verify.py`
- `scripts/record_task_issue.py`
- `scripts/record_task_decision.py`
- `scripts/promote_task_knowledge.py`
- `scripts/archive_task_memory.py`
- `scripts/run_release_gate.py`
- `scripts/generate_commit_message.py`
- `scripts/append_memory_inbox.py`

### 它做什么

- 选择当前任务
- 将任务从 `todo` 推到 `doing`
- 读取 memory、knowledge 和 legacy context
- 跑构建和测试
- 写入证据和验证记录
- 回写 issue / decision / knowledge
- 回写 `.ai/runtime/project-state.json`
- 提交并推送
- 跑 release gate
- 更新任务状态
- 必要时归档任务记忆

### 典型产物

- 代码变更
- 验证结果
- 证据记录
- 任务状态
- 更新后的 `project-state.json`
- commit / push
- release gate 结论

### 典型判断点

- 是否已显式开工
- 当前任务是 `doing` 还是 `todo`
- 是否命中 bugfix / continuation 信号
- 是否有可复用的老项目业务域、接口链路和历史文档上下文
- 是否该补 issue / decision / knowledge

### 边界

- 必须显式开工
- 不能绕过人工审核门
- 不能只改代码不留痕

---

## 调用关系

三个 skill 的顺序是固定的：

1. 先用 `workflow-bootstrap` 建底座
2. 再用 `workflow-requirement` 做需求治理
3. 最后用 `workflow-execution` 做执行收口

## 核心依赖

- `AGENTS.md`
- `docs/workflow/PROJECT_CONTEXT.md`
- `.ai/runtime/profile/project-profile.yml`
- `.ai/runtime/project-state.json`
- `.ai/runtime/cache/legacy-scan.json`
- `.ai/memory/tasks/index.md`
- `docs/workflow/requirements/需求池.md`
- `docs/workflow/requirements/任务看板.md`
