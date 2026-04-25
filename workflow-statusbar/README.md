# workflow-statusbar

`workflow-statusbar` 是 `workflow-skills-copy` 仓库内的桌面子工程，用于常驻监听：

- 三类 workflow skill 写入的 `.ai/runtime/project-state.json`
- 本机 `~/.codex` 会话、日志与最近线程
- Codex 当前是否仍在持续执行

产品形态：

- 菜单栏托盘入口
- 卡片式主弹层
- 执行中小悬浮窗
- 关键状态变化通知

## 自动知识采集（Personal Knowledgebase）

状态栏会在关键状态变化时自动推送事件到本地知识库服务：

- 默认地址：`http://127.0.0.1:8787/api/events/push`
- 事件来源：任务完成、项目完成、阻塞、中断、自动续跑失败等状态变化
- 推送失败不会影响状态栏主功能（忽略错误，继续运行）

可用环境变量：

- `WORKFLOW_STATUSBAR_KB_PUSH`：是否启用推送（默认启用）
  - 可选：`true/false`、`1/0`
- `WORKFLOW_STATUSBAR_KB_ENDPOINT`：知识库服务地址（默认 `http://127.0.0.1:8787`）

示例：

```bash
export WORKFLOW_STATUSBAR_KB_PUSH=true
export WORKFLOW_STATUSBAR_KB_ENDPOINT=http://127.0.0.1:8787
npm run tauri dev
```

## 开发

```bash
cd workflow-statusbar
source "$HOME/.cargo/env"
npm install
npm run tauri dev
```

## 打包

```bash
cd workflow-statusbar
source "$HOME/.cargo/env"
npm run package:current
```

常用打包命令：

```bash
# 当前平台
npm run package:current

# macOS
npm run package:mac

# Windows
npm run package:win
```

说明：

- `package:current`：按当前机器平台打包
- `package:mac`：生成 `app` 和 `dmg`
- `package:win`：生成 `nsis` 和 `msi`
- `Windows` 包通常建议在 `Windows` 机器或 CI 环境中打包；如果当前是 `macOS`，没有额外交叉编译环境时，优先先打 `mac` 包

正式发布（`v*` tag）补充：

- CI 会对 macOS 包执行签名和公证（notarization），用于避免安装时出现“已损坏”
- 需要在 GitHub 仓库 Secrets 配置以下字段：
  - `APPLE_DEVELOPER_ID_CERT_P12_BASE64`
  - `APPLE_DEVELOPER_ID_CERT_PASSWORD`
  - `APPLE_DEVELOPER_ID_IDENTITY`
  - `APPLE_ID`
  - `APPLE_TEAM_ID`
  - `APPLE_APP_SPECIFIC_PASSWORD`
- 未配置上述 Secrets 时，`v*` tag 的 macOS 发布任务会直接失败并提示缺失项
- CI 默认并行产出 macOS `x64`（Intel）和 `arm64`（Apple Silicon）两套包

## 目录

- `src/`: React 前端和卡片界面
- `src-tauri/`: Tauri 后端、托盘、窗口控制、状态聚合
- `fixtures/`: 示例状态文件
- `docs/`: 状态模型和实现说明
