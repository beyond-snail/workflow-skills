# AGENTS

## 默认低调用量分级规则

本节优先于下方旧版“默认完整闭环”描述；但不覆盖生产、SQL、权限、安全、发布、客户交付等高风险规则。

- 问答 / 统计 / 解释 / 原因判断：`必要最小检索 -> 分析 -> 回答`。不改动、不验证、不沉淀、不提交。
- 排查 / 方案 / 风险评估：`必要检索 -> 分析 -> 方案`。未经用户确认不改代码；只扩大到定位根因所需的文件、日志、SQL 或接口。
- 开发 / 修复 / 开干 / 验收 / 发布 / 提交：`检索历史 -> 分析 -> 改动 -> 验证 -> 沉淀 -> 提交`。仅在用户明确要求实现、修复、开干、提交、推送、发布，或任务涉及生产/数据/SQL/权限/安全/客户交付时启用完整闭环。
- 普通任务默认先用 `rg` / `rg --files` 定位，局部读取命中文件；不要默认读取完整 PRD、完整任务看板、完整日志、完整 transcript 或大 diff。
- 普通任务默认最多读取 5 个相关文件、最多执行 8 次 shell 命令；超过后先汇报当前结论和下一步建议。
- 如果用户只是问原因、统计、解释或方案，默认只回答结论，不进入代码修改。

## 默认输出
- 默认简单明了：先给 3-5 行短分析，再执行。
- 只有跨模块、SQL/接口、生产故障、安全/数据风险、提交发布或用户要求详细分析时，展开文件/链路/影响范围/根因。
- 最终回复只写：改了什么、验证结果、提交/推送状态、未覆盖风险。
- 涉及发布、验收、生产数据、SQL、权限、安全、跨模块接口、客户交付时，不得静默使用 compact；需说明风险并升级 audit 或请求确认。

## Codex Token 控制
- Codex 压缩恢复优先读取 `.ai/memory/context-brief.md` 和 checkpoint 的 `Transcript Digest`，不得默认读取完整 transcript。
- 多窗口并行时，项目共享稳定状态使用 `.ai/memory/context-brief.md`；窗口级当前焦点优先读取 `.ai/memory/session-briefs/<session_id>.md`。
- 压缩恢复默认只读取 `AGENTS.md` 与 `.ai/memory/context-brief.md`；除非需要事实结论、代码改动、SQL/接口判断或验证提交，否则不得扩展读取历史文件、完整 transcript、完整 checkpoint 或大文档。
- `.ai/runtime/conversations/`、`.ai/memory/compact-checkpoints/` 和 `~/.codex/memories/compact-checkpoints/` 视为冷归档；只有追溯证据时才按关键词局部读取。
- skill 使用只读取触发的 `SKILL.md` 和必要 references；PRD、大日志、大 diff、历史任务文件必须先检索再局部读取。
- 工具输出需要主动限量，优先使用 `rg`、`sed -n`、`git diff --stat`、tail 摘要，避免把大文件整段送入上下文。
