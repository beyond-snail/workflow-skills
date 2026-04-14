# Implementation Notes

`workflow-statusbar` 的 MVP 采用两层结构：

- Tauri 后端聚合 `~/.codex` + `project-state.json`
- React 前端只消费统一的 `RuntimeState`

## 当前实现

1. 托盘图标点击切换主弹层窗口显示
2. 后端每 5 秒重新聚合一次状态
3. 聚合结果通过 `runtime-state` 事件推送给前端
4. `running` 或 `stalled` 时自动显示悬浮窗
5. 焦点任务切换、阻塞和 Codex 离开运行态时触发通知

## 后续可扩展

- 从 `logs_2.sqlite` 读取更细的 thread-level 事件
- 增加用户设置页和阈值配置
- 把托盘图标做成按状态变色
- 增加手动锁定项目的能力

## 当前专项计划

- 多 Host 监控升级任务清单：`docs/MULTI_HOST_MONITORING_TASKS.md`
