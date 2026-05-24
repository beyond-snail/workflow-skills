# Context Brief

## 作用
- Codex 压缩恢复后的优先上下文摘要。
- 只保留当前任务、关键决策、关键文件、验证、阻塞和下一步。
- 覆盖更新，不长期追加；建议控制在 120 行以内。

## 当前上下文
- 当前需求：待补充
- 当前任务：待补充
- 当前状态：initialized
- 关键决策：优先使用本文件恢复上下文，避免默认读取完整 transcript 或大历史文件。
- 关键文件：AGENTS.md；.ai/memory/tasks/index.md；.ai/runtime/project-state.json
- 验证结论：待项目任务收口时由 workflow-execution 更新。
- 阻塞风险：暂无
- 下一步：执行任务收口后覆盖更新本文件。

## 恢复规则
- 新窗口或压缩恢复后，先读 `AGENTS.md`、本文件、`.ai/memory/tasks/index.md` 和 `.ai/runtime/project-state.json`。
- 完整 transcript、compact checkpoint、conversation 冷归档仅在追溯证据时按关键词局部读取。
