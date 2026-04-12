# workflow-statusbar 飞书提醒接入方案

## 目标

让 `workflow-statusbar` 在本地检测到关键事件后，不仅能在 macOS 本机提醒，还能把消息推送到飞书；在第二阶段支持用户在手机上查看中断原因，并通过飞书交互继续任务。

## 适用事件

- 任务中断
- 自动续跑失败
- 任务完成
- 项目完成
- 项目阻塞

## 总体架构

### Phase 1：单向推送

`workflow-statusbar`
-> 本地统一告警入口 `push_alert(...)`
-> HTTP 调用通知桥接服务
-> 通知桥接服务调用飞书接口
-> 飞书把消息推送到手机

### Phase 2：交互卡片

`workflow-statusbar`
-> 通知桥接服务
-> 飞书交互卡片消息
-> 用户在手机端点击 `继续 / 详情 / 暂停`

### Phase 3：回调继续任务

飞书事件回调
-> 通知桥接服务验签
-> 根据卡片里的业务标识找到 `project/thread/task`
-> 执行 `codex exec resume <thread_id> "..."`

## 为什么推荐飞书自建应用

只做“单向提醒”时，机器人 webhook 就够用。

但如果要支持：

- 手机上查看中断原因
- 点击按钮触发继续任务
- 查询详情
- 做权限控制和审计

就应该使用：

- 飞书自建应用
- 消息卡片
- 事件订阅回调

## 第一阶段最小实现

### 触发入口

当前项目里已经有统一告警入口：

- 本地系统通知
- 自动弹出状态栏面板

建议在 `push_alert(...)` 里追加一个远程推送步骤：

- `push_alert(...)`
  - 系统通知
  - 状态栏弹窗
  - `post_remote_alert(...)`

### 通知桥接服务职责

桥接服务建议独立，不建议直接由 `workflow-statusbar` 本体去持有飞书密钥和复杂回调逻辑。

服务职责：

- 接收本地告警事件
- 组装飞书消息
- 负责调用飞书发送接口
- 记录事件日志
- 后续负责回调验签和继续任务

### 建议事件载荷

```json
{
  "event_type": "task_interrupted",
  "project_name": "erp-finance",
  "project_path": "/Users/xxx/Documents/jty-work/erp-finance",
  "thread_id": "019d803f-0dd3-7e60-b060-e481b792b46c",
  "task_id": "TASK-2026-04-09-32",
  "task_title": "按 PRD v1.6 收口订单数量公式、报表和测试材料",
  "workflow_stage": "execution",
  "codex_status": "stalled",
  "heartbeat_at": "2 分钟前",
  "reason_summary": "Codex 已从执行中切换为可能卡住，自动续跑未成功",
  "occurred_at": "2026-04-12T18:30:00+08:00"
}
```

### 飞书消息内容建议

#### 任务中断

- 标题：`workflow-statusbar · 任务中断`
- 正文：
  - 项目：`erp-finance`
  - 任务：`TASK-2026-04-09-32`
  - 状态：`执行中 -> 可能卡住`
  - 最近心跳：`2 分钟前`
  - 原因摘要：`自动续跑失败`

#### 任务完成

- 标题：`workflow-statusbar · 任务完成`
- 正文：
  - 项目
  - 任务编号
  - 任务标题

#### 项目完成

- 标题：`workflow-statusbar · 项目完成`
- 正文：
  - 项目名
  - 进入完成阶段时间

## 第二阶段：飞书交互卡片

### 适合做交互卡片的事件

- 任务中断
- 自动续跑失败
- 项目阻塞

### 卡片字段建议

- 项目名
- 任务编号
- 任务标题
- 当前阶段
- Codex 状态
- 最近心跳
- 原因摘要

### 卡片按钮建议

- `继续`
- `详情`
- `暂停`

### 推荐交互逻辑

#### 继续

- 飞书回调带上事件 id 和按钮 value
- 服务端找到对应 `thread_id`
- 执行：

```bash
codex exec resume <thread_id> "继续当前任务，请从中断处继续执行；如果没有新的用户输入要求，直接按既定计划推进。"
```

#### 详情

- 返回更详细的上下文摘要
- 不建议直接推送全量上下文

建议详情内容：

- 最近一次任务状态
- 最近一次阶段
- 最近关键日志摘要
- 最近自动续跑尝试结果

#### 暂停

- 将该项目写入“暂停提醒列表”
- 暂时不再自动续跑

## 第三阶段：服务端回调处理

### 服务端需要做的事

- 校验飞书事件签名
- 解析按钮动作
- 根据事件 id 找到对应上下文
- 执行 resume 或返回详情
- 记录操作日志

### 推荐维护一张事件映射表

字段建议：

- `event_id`
- `event_type`
- `project_name`
- `project_path`
- `thread_id`
- `task_id`
- `task_title`
- `codex_status`
- `reason_summary`
- `created_at`
- `resolved_at`
- `operator`

这样飞书回调就不需要把所有上下文都塞进卡片，只要传一个稳定 `event_id` 即可。

## 本地代码落点建议

### workflow-statusbar 侧

建议增加：

- `AlertDispatcherConfig`
- `post_remote_alert(payload)`
- 在 `push_alert(...)` 中统一调用远程推送

### 配置方式建议

可以先走环境变量：

- `WORKFLOW_ALERT_PROVIDER=feishu`
- `WORKFLOW_ALERT_ENDPOINT=http://127.0.0.1:8787/alert`
- `WORKFLOW_ALERT_TOKEN=xxx`

后续再改为配置文件。

## 安全建议

- 不要把飞书应用密钥直接写进 `workflow-statusbar`
- 不要把全量任务上下文直接推送到手机
- 只推送摘要和必要标识
- 回调接口必须验签
- “继续任务”操作要记录审计日志

## 实施顺序建议

1. 先做通知桥接服务
2. 再做 `workflow-statusbar -> 桥接服务` 的 HTTP 推送
3. 打通飞书单向提醒
4. 再做交互卡片
5. 最后做回调继续任务

## 验收标准

### Phase 1

- 本地触发“任务中断”后，手机能收到飞书提醒
- 本地触发“任务完成”后，手机能收到飞书提醒

### Phase 2

- 手机上能看到中断原因摘要
- 手机上可以点击 `继续`

### Phase 3

- 点击 `继续` 后，服务端能成功定位 `thread_id`
- 任务可以通过 `codex exec resume` 恢复
- 操作记录可审计
