# NotebookLM 资料索引

## 这套资料的用途

这些文档是给 NotebookLM 生成分享型 PPT 用的源材料，不是练习册，也不是培训手册。

分享主题：

`workflow-bootstrap -> workflow-requirement -> workflow-execution`

补充背景：

- `workflow-bootstrap` 已支持老项目自动扫描与画像
- `workflow-requirement` / `workflow-execution` 会复用老项目画像上下文
- 三个 skill 会共同维护 `.ai/runtime/project-state.json`

## 推荐导入顺序

1. [流程总览说明.md](./流程总览说明.md)
2. [三个skill分析.md](./三个skill分析.md)
3. [使用方法与误区.md](./使用方法与误区.md)
4. [PPT页纲.md](./PPT页纲.md)
5. [操作步骤说明.md](./操作步骤说明.md)
6. [分享话术.md](./分享话术.md)
7. [未来扩展方向.md](./未来扩展方向.md)
8. [可视化草图.md](./可视化草图.md)
9. [PPT成稿结构.md](./PPT成稿结构.md)

## 分享结论

- 这三套 skill 是一条仓库内治理链路
- `workflow-bootstrap` = 仓库底座初始化
- `workflow-requirement` = 需求治理
- `workflow-execution` = 执行收口
- 老项目也可以通过 `wf-init` 自动接入，不需要先人工分类
- `legacy-analysis.md` 给人看，`legacy-scan.json` 给后续 skill 复用

## 生成 PPT 时的建议提示词

可以直接让 NotebookLM 按下面的角度输出：

1. 先给出 40 分钟版本的分享大纲
2. 再给出 60 分钟版本的分享大纲
3. 按“背景 -> 三 skill 分析 -> 使用方式 -> 误区”组织内容
4. 每页只放一个核心观点，避免把脚本原文堆进去
5. 用中文，偏分析，不要写练习题
6. 明确讲出老项目自动画像与统一状态源的价值

## 适合的输出形式

- 8 到 12 页的 PPT 结构
- 每页标题 + 3 到 5 个要点
- 一页总览图
- 一页流程门槛图
- 一页误区对照图
- 一页操作步骤图
- 一页老项目自动画像与状态源图
- 一份逐页讲解话术
- 一页未来扩展方向
- 三张可视化草图
- 一份最终 PPT 成稿结构
