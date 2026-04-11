# Project Cockpit

一个面向多项目交付管理的 React 驾驶舱。

## 页面结构

- 首页：九宫格项目墙，只负责选项目
- 项目页：任务执行页，展示当前项目的任务、证据、风险和时间线

## 目录

- `src/data.js`：项目、任务、证据、风险的示例数据
- `src/lib/cockpit.js`：状态映射、本地同步、任务排序与格式化
- `src/components/CockpitUI.jsx`：页面通用卡片组件
- `src/components/ComposerModal.jsx`：新建项目弹窗
- `src/App.jsx`：驾驶舱页面装配层
- `src/styles.css`：视觉样式与动效

## 运行

```bash
npm install
npm run dev
```

## 构建

```bash
npm run build
```

## 设计目标

这个页面不是技能说明页，而是给团队日常看项目推进情况用的控制台。

支持：

- 九宫格项目选择
- 项目页任务执行
- 阶段总览
- 证据与风险
- 时间线
- 本地状态文件同步
