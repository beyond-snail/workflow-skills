# workflow-bootstrap CLI 示例

## 1. 新仓库完整初始化

```bash
python3 scripts/init_workflow_bootstrap.py --host codex --host claude
```

## 2. 只生成 Codex 宿主补充

```bash
python3 scripts/init_workflow_bootstrap.py --host codex
```

## 3. 预演但不落文件

```bash
python3 scripts/init_workflow_bootstrap.py --host codex --host claude --dry-run
```

## 4. 允许覆盖宿主补充文件

```bash
python3 scripts/init_workflow_bootstrap.py --host codex --force-host-files
```
