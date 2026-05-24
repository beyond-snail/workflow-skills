# AGENTS

## 默认输出
- 默认简单明了：先给 3-5 行短分析，再执行。
- 只有跨模块、SQL/接口、生产故障、安全/数据风险、提交发布或用户要求详细分析时，展开文件/链路/影响范围/根因。
- 最终回复只写：改了什么、验证结果、提交/推送状态、未覆盖风险。

## Codex Token 控制
- Codex 压缩恢复优先读取 `.ai/memory/context-brief.md` 和 checkpoint 的 `Transcript Digest`，不得默认读取完整 transcript。
- 多窗口并行时，项目共享稳定状态使用 `.ai/memory/context-brief.md`；窗口级当前焦点优先读取 `.ai/memory/session-briefs/<session_id>.md`。
- 压缩恢复默认只读取 `AGENTS.md` 与 `.ai/memory/context-brief.md`；除非需要事实结论、代码改动、SQL/接口判断或验证提交，否则不得扩展读取历史文件、完整 transcript、完整 checkpoint 或大文档。
- `.ai/runtime/conversations/`、`.ai/memory/compact-checkpoints/` 和 `~/.codex/memories/compact-checkpoints/` 视为冷归档；只有追溯证据时才按关键词局部读取。
- skill 使用只读取触发的 `SKILL.md` 和必要 references；PRD、大日志、大 diff、历史任务文件必须先检索再局部读取。
- 工具输出需要主动限量，优先使用 `rg`、`sed -n`、`git diff --stat`、tail 摘要，避免把大文件整段送入上下文。
