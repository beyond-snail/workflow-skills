# CLI 参考命令

仅在需要具体命令示例时再读取本文件。

## 主入口

```bash
python3 <skill-dir>/scripts/run_execution_round.py \
  --confirm-start \
  --req-id REQ-xxxx \
  --summary "本轮开发摘要"
```

默认 `--writeback compact`：只写测试摘要、`verify.md` 摘要、`project-state.json` 和短版 `context-brief.md`。

正式验收/审计留痕再追加：

```bash
python3 <skill-dir>/scripts/run_execution_round.py \
  --confirm-start \
  --req-id REQ-xxxx \
  --summary "正式验收回写" \
  --writeback audit
```

bugfix / continuation 建议追加：

```bash
python3 <skill-dir>/scripts/run_execution_round.py \
  --confirm-start \
  --req-id REQ-xxxx \
  --summary "本轮开发摘要" \
  --mode continuation \
  --issue-note "本轮问题摘要" \
  --decision-note "本轮关键决策" \
  --promote-knowledge
```

## 常用脚本

```bash
python3 <skill-dir>/scripts/select_next_task.py \
  --task-file docs/workflow/requirements/任务看板.md \
  --req-id REQ-xxxx
```

```bash
python3 <skill-dir>/scripts/update_task_status.py \
  --task-file docs/workflow/requirements/任务看板.md \
  --task-id TASK-xxxx \
  --status doing \
  --expected-current todo
```

```bash
python3 <skill-dir>/scripts/record_task_evidence.py \
  --file /path/to/evidence.md \
  --task-id TASK-xxxx \
  --summary "本次完成内容" \
  --verification "mvn -q -DskipTests compile 通过"
```

```bash
python3 <skill-dir>/scripts/load_memory_context.py \
  --req-id REQ-xxxx \
  --task-id TASK-xxxx \
  --keyword "模块关键词"
```

```bash
python3 <skill-dir>/scripts/record_task_verify.py \
  --file .ai/memory/tasks/.../verify.md \
  --action "mvn -q -DskipTests compile" \
  --result PASS \
  --coverage "编译校验"
```

```bash
python3 <skill-dir>/scripts/record_task_issue.py \
  --file .ai/memory/tasks/.../issues.md \
  --issue-id ISSUE-001 \
  --phenomenon "现象"
```

```bash
python3 <skill-dir>/scripts/record_task_decision.py \
  --file .ai/memory/tasks/.../decisions.md \
  --decision-id DEC-001 \
  --decision "决策内容"
```

```bash
python3 <skill-dir>/scripts/promote_task_knowledge.py \
  --title "知识标题" \
  --summary "稳定结论" \
  --source-task-dir .ai/memory/tasks/...
```

```bash
python3 <skill-dir>/scripts/generate_commit_message.py \
  --task-file docs/workflow/requirements/任务看板.md \
  --task-id TASK-xxxx
```

```bash
python3 <skill-dir>/scripts/run_release_gate.py \
  --project-root /path/to/repo \
  --req-file docs/workflow/requirements/需求池.md \
  --req-id REQ-xxxx \
  --doc-file docs/workflow/requirements/任务看板.md
```
