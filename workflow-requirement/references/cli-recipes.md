# CLI 参考命令

仅在需要具体命令示例时再读取本文件。

## 主入口

```bash
python3 <skill-dir>/scripts/run_requirement_round.py \
  --theme "主题" \
  --summary "一句话需求摘要"
```

默认会启用“高质量正文保留”（二次执行不降级覆盖）。
如需强制覆写已有正文，可追加：

```bash
  --allow-content-overwrite
```

如需显式控制任务记忆初始化：

```bash
python3 <skill-dir>/scripts/run_requirement_round.py \
  --theme "主题" \
  --summary "一句话需求摘要" \
  --task-memory-type feature
```

## 常用脚本

```bash
python3 <skill-dir>/scripts/create_requirement_bundle.py \
  --docs-root /path/to/repo/docs/workflow \
  --date YYYY-MM-DD \
  --theme "主题"
```

```bash
python3 <skill-dir>/scripts/populate_requirement_content.py \
  --req-file docs/workflow/requirements/需求池.md \
  --task-file docs/workflow/requirements/任务看板.md \
  --req-id REQ-xxxx \
  --initial-task-id TASK-xxxx \
  --theme "主题" \
  --date YYYY-MM-DD \
  --bundle-dir docs/workflow/requirements/YYYY-MM-DD-主题 \
  --prd-file docs/workflow/PRD/xxx.md
```

如需二次执行时保护已有高质量正文：

```bash
python3 <skill-dir>/scripts/populate_requirement_content.py \
  ... \
  --preserve-non-placeholder
```

```bash
python3 <skill-dir>/scripts/check_handoff_readiness.py \
  --req-file docs/workflow/requirements/需求池.md \
  --task-file docs/workflow/requirements/任务看板.md \
  --req-id REQ-xxxx \
  --docs-root .
```

```bash
python3 <skill-dir>/scripts/init_task_memory.py \
  --task-id TASK-xxxx \
  --title "任务标题" \
  --date YYYY-MM-DD \
  --req-id REQ-xxxx
```

```bash
python3 <skill-dir>/scripts/sync_task_index.py \
  --task-id TASK-xxxx \
  --title "任务标题" \
  --keywords "`REQ-xxxx` / 关键词" \
  --directory .ai/memory/tasks/YYYY-MM-DD-任务标题/
```

## 证据回写

```bash
python3 <skill-dir>/scripts/sync_prd_trace.py \
  --file docs/workflow/requirements/.../design/YYYY-MM-DD-PRD追溯-主题.md \
  --mode trace \
  --prd-section "7.2 / F001" \
  --prd-desc "需求描述" \
  --mapping "`设计文档`；`代码位置`；`测试类`"
```

```bash
python3 <skill-dir>/scripts/record_test_result.py \
  --file docs/workflow/requirements/.../testing/YYYY-MM-DD-测试结果-主题.md \
  --title "自动化补跑" \
  --status pass \
  --summary "本次补跑摘要"
```

默认写 compact 一行摘要；正式测试报告再加 `--format audit`。

```bash
python3 <skill-dir>/scripts/record_acceptance_result.py \
  --file docs/workflow/requirements/.../testing/YYYY-MM-DD-联调验收记录-主题.md \
  --title "联调补录" \
  --status pass \
  --summary "本次联调摘要"
```

默认写 compact 一行摘要；正式联调/验收材料再加 `--format audit`。
