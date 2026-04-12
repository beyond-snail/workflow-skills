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
npm run tauri build
```

## 目录

- `src/`: React 前端和卡片界面
- `src-tauri/`: Tauri 后端、托盘、窗口控制、状态聚合
- `fixtures/`: 示例状态文件
- `docs/`: 状态模型和实现说明
