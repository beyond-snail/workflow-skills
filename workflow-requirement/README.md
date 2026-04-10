# workflow-requirement 技能

## 技能介绍

`workflow-requirement` 是一个通用需求与任务治理技能，用于管理需求入池、设计文档、任务看板和执行交接的通用治理流程。当项目采用“需求池 + 任务看板 + 按日期命名执行文档”的方式推进时，应使用此技能。默认收口在“待人工审核”，不会在任务拆解后直接自动进入开发执行。

## 功能特性

- **需求入池管理**：将需求正式写入需求池，确保需求可追踪
- **设计文档生成**：自动创建按日期命名的设计文档目录和文件
- **任务看板同步**：将需求拆分为具体任务，同步到任务看板
- **执行材料补齐**：生成开发任务拆解、测试验收标准等执行材料
- **一句话治理入口**：通过 `run_requirement_round.py` 一次完成建包、入池、拆任务和交接检查
- **PRD 正文填充**：自动把 PRD 关键内容展开到需求池摘要、设计文档、追溯和测试材料
- **多任务拆解**：不再只生成一条启动任务，而是按 PRD 模块细化到任务看板
- **人工审核门**：任务拆解和交接检查通过后，默认停住，等待人工审核再开干
- **HTML导出**：将需求池和任务看板导出为美观的HTML页面，方便产品查看
- **任务记忆初始化**：优先初始化 `.ai/memory/tasks/<task>/` 并同步 `.ai/memory/tasks/index.md`，老仓库兼容回退 `.ai/memory/tasks/`
- **回合自检**：dry-run 和 live 模式都会输出 requirement 自检结果，确认需求池、任务看板、任务记忆和 bundle 落点是否齐全

## 安装方法

1. **安装到宿主 skill 目录**：
   ```bash
   # 按宿主平台放到对应 skill 根目录，例如：
   # ~/.codex/skills/workflow-requirement
   # ~/.claude/skills/workflow-requirement
   # <repo>/.trae/skills/workflow-requirement
   git clone <技能仓库地址> <skill-root>/workflow-requirement
   ```

2. **确保依赖**：
   - Python 3.6+
   - 必要的Python库（如需要）

## 使用方法

### 推荐提示词

对话里推荐直接这样说：

1. `用 workflow-requirement 处理这个 PRD`
2. `把这个 PRD 入池并拆成任务看板`
3. `帮我生成需求包，先不要开干`
4. `按 workflow-requirement 生成需求治理材料`
5. `把这个需求整理成 requirement bundle，停在人工审核门`

### 一句话入口

推荐优先使用：

```bash
python3 <skill-dir>/scripts/run_requirement_round.py \
  --theme "订单销售成本结转" \
  --summary "一句话整理需求并自动拆任务"
```

如需顺带生成给产品查看的总览页，可追加：

```bash
  --export-html
```

脚本会自动完成建包、回填治理文件、交接检查，并停在人工审核门。
在存在有效 `PRD` 时，还会自动填充需求池摘要、设计正文、PRD追溯、UAT用例和多条开发任务。
如仓库存在 `.ai/memory/tasks/_template/`，会优先初始化目标任务记忆目录并登记到索引；老仓库兼容回退 `.ai/memory/tasks/_template/`。

### 1. 冻结需求

将新需求或需求调整写入需求池，确保需求标题稳定、可追踪，并在备注中写清范围、阶段、依赖和关键限制。

推荐使用脚本回填需求池：

```bash
python3 <skill-dir>/scripts/sync_requirement_pool.py \
  --req-file doc/requirements/需求池.md \
  --req-id REQ-2026-03-27-01 \
  --title "新需求演练" \
  --status planned \
  --source "[doc/PRD/新需求PRD.md](doc/PRD/新需求PRD.md)" \
  --design-doc "[doc/requirements/2026-03-27-新需求演练/design/2026-03-27-技术设计-新需求演练.md](doc/requirements/2026-03-27-新需求演练/design/2026-03-27-技术设计-新需求演练.md)" \
  --task-board "[doc/requirements/任务看板.md](doc/requirements/任务看板.md)" \
  --dry-run
```

### 2. 创建设计文档

使用脚本创建需求目录和完整分层骨架：

```bash
python3 <skill-dir>/scripts/create_requirement_bundle.py \
  --docs-root doc \
  --date 2026-03-19 \
  --theme "订单销售成本结转"
```

默认会：

1. 创建完整分层骨架
2. 自动生成 `REQ-ID`
3. 自动生成一条初始 `TASK-ID`
4. 自动回填 `doc/requirements/需求池.md`
5. 自动回填 `doc/requirements/任务看板.md`
6. 若治理文件不存在，则按默认模板自动初始化

默认会创建以下结构：
- `doc/requirements/2026-03-19-订单销售成本结转/`
- `doc/requirements/2026-03-19-订单销售成本结转/00-目录说明.md`
- `doc/requirements/2026-03-19-订单销售成本结转/design/2026-03-19-技术设计-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/design/2026-03-19-详细开发设计-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/design/2026-03-19-开发任务拆解-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/design/2026-03-19-物理表设计-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/design/2026-03-19-表名对照表-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/design/2026-03-19-PRD追溯-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/design/2026-03-19-产品确认清单-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/design/2026-03-19-流程图与实现对齐-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/ddl/2026-03-19-DDL-订单销售成本结转.sql`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/ddl/2026-03-19-DDL-字段修正-订单销售成本结转.sql`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/ddl/2026-03-19-DDL-索引修正-订单销售成本结转.sql`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/ddl/2026-03-19-DDL-精简字段-订单销售成本结转.sql`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/ddl/2026-03-19-DDL-主键序列-订单销售成本结转.sql`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/fix/2026-03-19-SQL-历史补全-订单销售成本结转.sql`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/fix/2026-03-19-SQL-人工映射模板-订单销售成本结转.sql`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/testdata/2026-03-19-SQL-测试样本-订单销售成本结转.sql`
- `doc/requirements/2026-03-19-订单销售成本结转/testing/2026-03-19-联调验收记录-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/testing/2026-03-19-测试结果-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/testing/2026-03-19-UAT测试用例-订单销售成本结转.md`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/ddl/`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/fix/`
- `doc/requirements/2026-03-19-订单销售成本结转/sql/testdata/`
- `doc/requirements/2026-03-19-订单销售成本结转/testing/`
- `doc/requirements/2026-03-19-订单销售成本结转/scripts/README.md`

如需兼容旧项目的平铺结构，可改用：

```bash
python3 <skill-dir>/scripts/create_requirement_bundle.py \
  --docs-root doc \
  --date 2026-03-19 \
  --theme "订单销售成本结转" \
  --layout flat
```

如只想建目录和文档，不想自动回填治理文件，可显式加：

```bash
python3 <skill-dir>/scripts/create_requirement_bundle.py \
  --docs-root doc \
  --date 2026-03-19 \
  --theme "订单销售成本结转" \
  --skip-governance-sync
```

### 3. 同步任务看板

将需求拆分为具体任务，同步到任务看板，并为每个任务添加验收标准。

推荐使用脚本回填任务看板：

```bash
python3 <skill-dir>/scripts/sync_task_board.py \
  --task-file doc/requirements/任务看板.md \
  --req-id REQ-2026-03-27-01 \
  --req-title "新需求演练" \
  --task-id TASK-2026-03-27-01 \
  --task-title "创建技术设计文档" \
  --status todo \
  --acceptance "技术设计文档已建立" \
  --doc-link "[doc/requirements/2026-03-27-新需求演练/design/2026-03-27-技术设计-新需求演练.md](doc/requirements/2026-03-27-新需求演练/design/2026-03-27-技术设计-新需求演练.md)" \
  --dry-run
```

### 4. 自动回写 PRD追溯 / 测试结果 / 联调验收

推荐使用以下脚本，避免手工改表格和长文档：

```bash
python3 <skill-dir>/scripts/sync_prd_trace.py \
  --file doc/requirements/.../design/YYYY-MM-DD-PRD追溯-主题.md \
  --mode trace \
  --prd-section "7.2 / F001" \
  --prd-desc "需求描述" \
  --mapping "`设计文档`；`代码位置`；`测试类`" \
  --status "已完成" \
  --note "补充说明" \
  --dry-run
```

```bash
python3 <skill-dir>/scripts/record_test_result.py \
  --file doc/requirements/.../testing/YYYY-MM-DD-测试结果-主题.md \
  --title "自动化补跑" \
  --status pass \
  --summary "本次补跑摘要" \
  --command "mvn -q -Dtest=DemoTest test" \
  --result "Tests run: 1, Failures: 0" \
  --conclusion "测试结论" \
  --dry-run
```

```bash
python3 <skill-dir>/scripts/record_acceptance_result.py \
  --file doc/requirements/.../testing/YYYY-MM-DD-联调验收记录-主题.md \
  --title "联调补录" \
  --status pass \
  --summary "本次联调摘要" \
  --scope "Step6 手动接口" \
  --verification "runStep6(validateOnly=false) 返回 passed=true" \
  --conclusion "联调结论" \
  --next-step "同步任务状态" \
  --dry-run
```

### 5. 导出HTML给产品查看

```bash
python3 <skill-dir>/scripts/markdown_to_html.py \
  --requirements doc/requirements/需求池.md \
  --tasks doc/requirements/任务看板.md
```

这将生成 `doc/requirements/需求看板.html` 文件，产品可以直接在浏览器中打开查看。

## 工作流程

1. **冻结需求**：将需求写入需求池
2. **创建设计文档**：生成按日期命名的设计文档骨架
3. **正文填充**：读取 PRD，把需求池摘要、设计文档、追溯和测试材料填成可读初稿
4. **同步任务看板**：将需求拆分为具体任务，而不是只留一条占位任务
5. **补齐执行材料**：生成开发任务拆解、测试验收标准等
6. **同步README/目录索引**：优先更新项目根 README；若项目实际索引落在需求目录 `00-目录说明.md`，则至少同步该文件；若两者都存在，则两者都要更新
7. **导出HTML**：生成美观的HTML页面给产品查看
8. **准备开发交接**：明确需求ID、任务ID、设计文档路径等信息，并确认：
   - 正式 `PRD` 已冻结
   - `需求池` 已入池
   - `任务看板` 已拆解
   - 至少已具备 `技术设计`、`开发任务拆解`、`PRD追溯`
   - 已明确当前阶段与下一阶段边界
9. **停在人工审核门**：输出交接结论和建议开工指令，等待人工审核；只有审核通过并显式说 `开干` / `执行开发` 后，才切到 `workflow-execution`

推荐在交接前跑一次：

```bash
python3 <skill-dir>/scripts/check_handoff_readiness.py \
  --req-file doc/requirements/需求池.md \
  --task-file doc/requirements/任务看板.md \
  --req-id REQ-2026-03-19-01 \
  --docs-root .
```

## 适用场景

- 需要把需求正式“入池”
- 需要把需求拆成任务看板
- 需要创建设计文档、任务拆解文档、验收文档
- 需要在开发开始前把流程和材料补齐

## 版本历史

- v1.3.0：新增 `populate_requirement_content.py`，支持基于 PRD 自动填充正文并细化任务看板
- v1.2.0：新增 `run_requirement_round.py` 一句话治理入口，默认停在人工审核门
- v1.0.0：初始版本，实现需求治理核心功能

## 贡献指南

欢迎提交Issue和Pull Request，帮助改进此技能。

## 许可证

MIT License
