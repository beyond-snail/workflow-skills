# Session Brief

## 作用
- 仅记录当前窗口/当前 session 的任务焦点。
- 解决多窗口并行时共享 `context-brief.md` 被互相覆盖的问题。
- 覆盖更新，不长期追加。

## 当前会话
- session_id：
- 当前任务：
- 当前结论：
- 关键文件：
- 验证结论：
- 阻塞风险：
- 下一步：

## 恢复规则
- 当前窗口压缩恢复后，优先读本文件，再读 `.ai/memory/context-brief.md`。
- 本文件只描述当前窗口；跨窗口共享状态以 `context-brief.md` 为准。
