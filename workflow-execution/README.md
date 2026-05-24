# workflow-execution 技能

## 技能介绍

`workflow-execution` 是一个通用任务执行技能，用于执行基于任务看板的通用开发流程。当项目采用“需求池 + 任务看板 + 验证 + 提交 + 发布闸门”的方式推进时，应使用此技能。它只能在人工审核通过后，由用户显式触发开发时启动。

## 功能特性

- **任务选择**：从任务看板中选择待执行的任务
- **状态管理**：自动更新任务状态（todo → doing → done）
- **实现验证**：执行项目的必要校验命令，确保代码质量
- **一句话开工入口**：通过 `run_execution_round.py` 在显式授权后自动推进收口链路
- **自动收口**：收到显式开工指令后，默认继续完成验证、compact 证据回写、提交推送和发布闸门
- **代码提交**：生成规范的 commit message，执行 git 提交和推送
- **发布闸门检查**：运行项目发布闸门，判断是否允许部署
- **记忆上下文加载**：开工前优先检索 `.ai/memory/tasks/index.md` 和 `.ai/memory/knowledge/`，老仓库兼容回退
- **任务记忆回写**：默认只写 `verify.md` 摘要；`--writeback audit` 时可自动写详细测试/验收、`inbox.md`，并可追加 `issues.md`、`decisions.md`、知识条目
- **执行自检**：dry-run 和 live 模式都会输出 execution 自检结果，检查任务看板、记忆目录、知识目录和校验命令是否齐全
- **任务记忆归档**：支持在任务收口后把 `.ai/memory/tasks/<task>/` 归档到 `archived/`
- **自动建议**：会根据 `bugfix/continuation`、根因/决策信号和复用信号，主动建议补 `issue`、`decision`、`knowledge`
- **归档降权检索**：默认优先命中活跃任务记忆，只有活跃目录缺失时才回退到 `archived/`

## 安装方法

1. **安装到宿主 skill 目录**：
   ```bash
   # 按宿主平台放到对应 skill 根目录，例如：
   # ~/.codex/skills/workflow-execution
   # ~/.claude/skills/workflow-execution
   # <repo>/.trae/skills/workflow-execution
   git clone <技能仓库地址> <skill-root>/workflow-execution
   ```

2. **确保依赖**：
   - Python 3.6+
   - Git
   - 项目所需的构建工具（如 Maven、npm、Gradle 等）

## 使用方法

### 推荐提示词

对话里推荐直接这样说：

1. `用 workflow-execution 开干`
2. `按任务看板继续做这条任务`
3. `继续处理这个需求，进入 execution`
4. `处理上次那个 bug，走 workflow-execution bugfix 模式`
5. `继续收口这个任务`

### 一句话开工入口

推荐优先使用：

```bash
python3 <skill-dir>/scripts/run_execution_round.py \
  --confirm-start \
  --req-id REQ-2026-03-19-01 \
  --summary "审核后执行开发并自动收口" \
  --stage-file src/main/java/com/acme/DemoService.java
```

说明：

1. `--confirm-start` 表示人工审核已完成，允许正式开工
2. 不传 `--task-id` 时，会自动选择当前 `doing` 或最早 `todo`
3. 不传 `--record-file` / `--test-result-file` 时，会优先从需求目录自动发现测试记录与验收文档
4. 为避免误暂存，自动提交场景必须显式传 `--stage-file`
5. 默认不传 `--mode` 也可以；execution 会自动在 `feature / bugfix / continuation` 之间判定
6. 如需强制指定，仍可显式传 `--mode bugfix|continuation|feature`
7. 若任务已经彻底收口，可加 `--archive-task-memory` 自动归档任务记忆目录
8. 若没有显式传 `--archive-task-memory`，execution 会在 `done / continuation` 场景主动提示是否归档
9. 默认 `--writeback compact`，只写测试/验证摘要和短版 `context-brief.md`
10. 若用户明确要求正式验收、发布前回归、审计留痕、完整回写、测试报告、联调记录或验收材料，追加 `--writeback audit`
11. 若嫌命令过长，可改用统一入口：

```bash
./.ai/bin/workflow exec \
  --workspace-root . \
  --req-id REQ-2026-03-19-01 \
  --task-id TASK-2026-03-25-20 \
  --summary "审核后执行开发并自动收口" \
  --dry-run

# 更短别名
./.ai/bin/workflow exec \
  --workspace-root . \
  --req-id REQ-2026-03-19-01 \
  --task-id TASK-2026-03-25-20 \
  --summary "审核后执行开发并自动收口" \
  --dry-run
```

### 1. 选择任务

先确认本回合已经过人工审核，且用户显式说了 `开干`、`执行开发`、`开始实现` 或同类指令；只有满足这个前提，才从任务看板中选择待执行任务。优先选择当前 `doing` 状态的任务，否则选择最早的 `todo` 任务。

推荐先跑：

```bash
python3 <skill-dir>/scripts/select_next_task.py \
  --task-file doc/requirements/任务看板.md \
  --req-id REQ-2026-03-19-01
```

### 2. 更新任务状态

- 开始前将任务状态改为 `doing`
- 完成后将任务状态改为 `done` 并补充证据

推荐使用脚本回写状态：

```bash
python3 <skill-dir>/scripts/update_task_status.py \
  --task-file doc/requirements/任务看板.md \
  --task-id TASK-2026-03-19-01 \
  --status doing \
  --expected-current todo \
  --dry-run
```

确认无误后去掉 `--dry-run`。

### 3. 实现与验证

- 读取设计文档和验收标准
- 实现代码和相关文档
- 代码实现完成后，如用户没有缩小范围，默认继续执行项目的必要校验命令（如编译、lint、类型检查等）
- 如需把执行结果回写到联调记录、测试结果或专项说明，使用：

```bash
python3 <skill-dir>/scripts/record_task_evidence.py \
  --file doc/requirements/.../testing/联调验收记录.md \
  --task-id TASK-2026-03-19-01 \
  --summary "完成主链路联调" \
  --verification "runStep6(validateOnly=false) 返回 passed=true" \
  --artifact "doc/requirements/.../testing/联调验收记录.md" \
  --next-step "同步任务状态为 done" \
  --dry-run
```

### 4. 提交与推送

自动运行项目校验命令，生成规范的 commit message，执行 git 提交和推送：

```bash
# 生成规范的 commit message
python3 <skill-dir>/scripts/generate_commit_message.py \
  --task-file doc/requirements/任务看板.md \
  --task-id TASK-2026-03-19-01 \
  --dry-run

# 执行 git 提交和推送
git status --short
git add <specific-files>
git commit -m "feat(task): TASK-2026-03-19-01 依据PRD完成技术设计文档编写"
git push
```

说明：

1. 不要默认使用 `git add .`
2. 先确认 `.gitignore` 已覆盖敏感文件
3. `--dry-run` 可先验证 task / req 解析结果

阶段交接前置条件：

1. 正式 `PRD` 已冻结
2. `需求池` 已入池
3. `任务看板` 已拆解
4. 至少已具备 `技术设计`、`开发任务拆解`、`PRD追溯`
5. 已明确当前阶段与下一阶段边界
6. 已完成人工审核，并收到用户显式开工指令

### 5. 检查发布闸门

运行项目发布闸门，判断是否允许部署：

- `[BLOCK]`：停止部署并继续剩余任务
- `[PASS]`：允许调用项目自己的部署入口

默认闸门清单：

1. 编译或构建通过
2. 关键自动化测试通过，或已记录阻塞原因与替代验证
3. 文档同步完成
4. PRD 追溯状态已更新
5. 暂存文件范围已确认

推荐直接使用脚本执行：

```bash
python3 <skill-dir>/scripts/run_release_gate.py \
  --project-root . \
  --req-file doc/requirements/需求池.md \
  --req-id REQ-2026-03-19-01 \
  --allowed-req-status done \
  --doc-file doc/requirements/任务看板.md \
  --doc-file doc/requirements/2026-03-19-订单销售成本结转/design/2026-03-25-PRD追溯-订单销售成本结转.md \
  --build-cmd "mvn -q -DskipTests compile" \
  --test-cmd "mvn -q -Dtest=SalesCostTransferSpecialSourceFallbackIntegrationTest test" \
  --require-clean-worktree \
  --dry-run
```

如果测试被样本/环境阻塞，但已在文档写明“阻塞原因 + 替代验证”，可改用：

```bash
python3 <skill-dir>/scripts/run_release_gate.py \
  --project-root . \
  --req-file doc/requirements/需求池.md \
  --req-id REQ-2026-03-19-01 \
  --allowed-req-status done \
  --doc-file doc/requirements/任务看板.md \
  --allow-test-blocked \
  --evidence-file doc/requirements/2026-03-19-订单销售成本结转/testing/2026-03-24-测试结果-Step6自动化-订单销售成本结转.md
```

### 6. 回退与阻塞

1. 如执行中发现方向错误，可将任务从 `doing` 回退到 `todo`
2. 如受环境或样本阻塞，应改成 `blocked`
3. 阻塞时必须记录：
   - 阻塞原因
   - 已完成的替代验证
   - 剩余待验证点
4. 测试数据阻塞时，优先补：
   - 真实样本执行记录
   - 合成样本自动化验证
   - 数据库校验 SQL
   - UAT 测试用例文档

## 工作流程

1. **选择任务**：从任务看板选择待执行的任务
2. **更新任务状态**：将任务状态改为 `doing`
3. **实现与验证**：实现代码并执行校验命令
4. **提交与推送**：生成 commit message 并执行 git 操作
5. **检查发布闸门**：判断是否允许部署

## 适用场景

- 人工审核通过后，用户显式说 `开干`
- 人工审核通过后，需要按任务看板推进下一条任务
- 人工审核通过后，需要完成任务收口
- 已明确进入执行阶段，需要确认是否允许继续推进

## 版本历史

- v2.2.0：新增 `run_execution_round.py` 一句话开工入口，支持自动验证、回写、提交与发布闸门
- v1.0.0：初始版本，实现任务执行核心功能

## 贡献指南

欢迎提交Issue和Pull Request，帮助改进此技能。

## 许可证

MIT License
