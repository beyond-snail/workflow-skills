# PPT 页纲

## 使用方式

这个文件不是培训练习，而是给 NotebookLM 生成 PPT 的页纲骨架。

建议将它和以下文档一起导入：

- [NotebookLM资料索引.md](./NotebookLM资料索引.md)
- [流程总览说明.md](./流程总览说明.md)
- [三个skill分析.md](./三个skill分析.md)
- [使用方法与误区.md](./使用方法与误区.md)
- [操作步骤说明.md](./操作步骤说明.md)
- [未来扩展方向.md](./未来扩展方向.md)
- [可视化草图.md](./可视化草图.md)
- [PPT成稿结构.md](./PPT成稿结构.md)

## 1. 为什么会有 workflow 三 skill

核心结论：

AI 协作开发需要把“初始化、需求治理、执行收口”拆开，否则边界、证据和状态都会混在一起。

要点：

- 传统协作里常见的问题是事实不统一、交接不清、状态漂移
- 三段式的本质是给 AI 协作加门槛
- `bootstrap`、`requirement`、`execution` 解决的是三个不同层次的问题

建议配图：

- 三段式流程图
- 从“混乱协作”到“治理链路”的对比图

## 2. 三阶段总览

核心结论：

这三套 skill 构成一个固定顺序的闭环。

要点：

- `workflow-bootstrap` 先建立仓库底座
- `workflow-bootstrap` 现在同时承担老项目自动扫描与画像
- `workflow-requirement` 再把 PRD 变成可交接材料
- `workflow-execution` 最后在审核通过后完成收口
- 三个阶段共同维护 `project-state.json` 这一份统一状态源

建议配图：

- `bootstrap -> requirement -> execution` 的单线流程图
- 三个阶段各自的输入输出卡片

## 3. `workflow-bootstrap` 解决什么问题

核心结论：

它解决的是“仓库还没有协作底座”的问题。

要点：

- 自动识别语言、构建工具、测试命令、源码目录、PRD 目录
- 自动识别当前是新项目还是老项目
- 生成 `AGENTS.md`、`docs/workflow/PROJECT_CONTEXT.md`
- 创建 `.ai/governance/`、`.ai/memory/`、`.ai/runtime/profile/`
- 生成 `.ai/runtime/project-state.json`
- 老项目会额外产出 `legacy-analysis.md` 和 `legacy-scan.json`
- 提供统一命令入口和健康检查

建议配图：

- 仓库初始化前后的对比图
- `.ai/` 目录结构示意图

## 4. `workflow-bootstrap` 的输出

核心结论：

bootstrap 的输出不是业务代码，而是协作规则和项目事实。

要点：

- `AGENTS.md`：协作契约
- `docs/workflow/PROJECT_CONTEXT.md`：项目事实
- `project-profile.yml`：构建和测试配置
- `.ai/memory/`：任务和知识落点
- `docs/workflow/requirements/`：需求治理骨架
- `project-state.json`：统一状态事实源
- 老项目画像：`legacy-analysis.md` + `legacy-scan.json`

建议配图：

- 输出物清单图
- 目录树示意图

## 5. `workflow-requirement` 解决什么问题

核心结论：

它解决的是“PRD 还没有被治理成可执行材料”的问题。

要点：

- 将 PRD 转成需求池
- 将需求池转成任务看板
- 生成 dated 需求包
- 命中并复用老项目业务域、接口链路和历史文档上下文
- 初始化 task memory
- 回写 `project-state.json`
- 做交接检查，但不进入开发

建议配图：

- PRD -> 需求池 -> 任务看板 -> 任务记忆 的转换图

## 6. `workflow-requirement` 的边界

核心结论：

requirement 不是执行入口，而是交接门。

要点：

- 会拆任务，但不写业务代码
- 会补材料，但不跑构建和测试
- 会给出交接结论，但不会自动进入 execution
- 默认停在人工审核门

建议配图：

- 审核门示意图
- “可进入 / 不可进入”决策图

## 7. `workflow-execution` 解决什么问题

核心结论：

它解决的是“任务已经审核通过，但还没有完成实现收口”的问题。

要点：

- 显式开工后才开始
- 自动选择任务、更新状态
- 读取 memory、knowledge 和 legacy context
- 跑构建与测试
- 写证据、写 verify、回写 issue / decision / knowledge
- 回写 `project-state.json`
- 提交、推送、跑 release gate

建议配图：

- 执行回路图
- 验证与证据链路图

## 8. `workflow-execution` 的边界

核心结论：

execution 是收口链路，不是单纯的代码修改器。

要点：

- 没有显式开工不能启动
- 不能只改代码不留痕
- 任务状态和证据必须同步
- 默认会更重，包含提交和闸门

建议配图：

- “开工 -> 实现 -> 验证 -> 留痕 -> 提交 -> 闸门”链路图

## 9. 三者如何串联

核心结论：

三者串联后，才形成可审计、可交接、可收口的 AI 协作链路。

要点：

- `bootstrap` 提供仓库事实和老项目画像
- `requirement` 提供需求事实
- `execution` 提供执行事实
- 三者共同维护 memory、证据和状态

建议配图：

- 三层堆栈图
- 从 PRD 到实现的完整闭环图

## 10. 常见误用

核心结论：

误用通常不是工具错，而是阶段边界被破坏。

要点：

- 跳过 bootstrap
- 老项目不跑 `wf-init` 就直接治理
- requirement 直接进 execution
- 忽略 memory 和 evidence
- 把审核门当形式
- 把 release gate 当可选项

建议配图：

- 误用清单
- 风险结果图

## 11. 最后结论

核心结论：

这套 workflow 的价值，不在于“让 AI 自动干活”，而在于“让 AI 在正确阶段做正确的事，并留下正确的证据”。

要点：

- 统一事实源
- 清晰阶段边界
- 证据链完整
- 适合多人、多轮、跨窗口接力

建议配图：

- 一句话总结页
- 闭环收口图

## 12. 操作步骤总览

核心结论：

分享时除了讲分析，还要让大家知道这三个 skill 在实际工作中怎么用。

要点：

- `workflow-bootstrap` 先建底座并自动扫描老项目画像，再看 health check
- `workflow-requirement` 先冻结 PRD，再生成需求包、复用 legacy context，并停在审核门
- `workflow-execution` 先确认审核通过，再显式开工推进收口
- 全程要保证 memory、证据、legacy context 和任务状态同步

建议配图：

- 三个 skill 的步骤流水线
- 每一步的输入输出对照图

## 13. 未来扩展方向

核心结论：

这三套 skill 的未来不是简单加功能，而是从“三段流程”演进成“更完整的阶段化工作流”和“更强的证据知识系统”。

要点：

- 可以扩展出 `review`、`qa`、`release`、`archive` 等阶段
- 可以把脚本里的规则抽成可配置策略
- 可以把证据和 legacy context 沉淀升级成项目知识
- 可以从单仓库能力扩展到跨仓库协作

建议配图：

- 演进路线图
- 从三段式到多阶段的扩展图

## 14. 可视化草图

核心结论：

前面的内容可以再压成三张图，方便 PPT 直接使用。

要点：

- 总流程图：展示 `bootstrap -> requirement -> execution`
- 三 skill 对照图：展示每个 skill 的作用、输入、输出和边界
- 误用对照图：展示常见错误路径和正确路径

建议配图：

- 三张图放在结尾的附录页或中间过渡页

## 15. PPT 成稿结构

核心结论：

前面的分析、操作步骤和可视化草图，最终可以收敛成一份 14 页左右的成稿结构。

要点：

- 每一页有明确标题
- 每一页都有对应图
- 每一页都有一句话讲法
- 整体顺序符合 40 到 60 分钟的分享节奏

建议配图：

- 最终 PPT 页面列表
- 每页标题与配图的对照表
