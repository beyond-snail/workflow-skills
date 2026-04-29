# Workflow Pack Examples

This directory contains copyable examples for workflow pack schema `1.0.0`.

## Files

- `minimal-development-handoff-pack.json`: minimal task handoff pack with two required items.
- `complete-project-knowledge-pack.json`: project-level pack with health, risk, action, and template items.
- `COMPATIBILITY.md`: versioning, checksum, import, and migration notes.

## Validate

Wrap an example as `package_json` before sending it to the local API:

```bash
node -e "const fs=require('fs'); const p=process.argv[1]; console.log(JSON.stringify({package_json: JSON.parse(fs.readFileSync(p,'utf8'))}))" \
  workflow-statusbar/docs/workflow-pack-examples/minimal-development-handoff-pack.json \
  | curl -fsS -X POST http://127.0.0.1:8788/api/workflow-packs/validate \
      -H 'content-type: application/json' \
      --data-binary @-
```

## Import

Import only after validation returns `valid=true`:

```bash
node -e "const fs=require('fs'); const p=process.argv[1]; console.log(JSON.stringify({package_json: JSON.parse(fs.readFileSync(p,'utf8'))}))" \
  workflow-statusbar/docs/workflow-pack-examples/complete-project-knowledge-pack.json \
  | curl -fsS -X POST http://127.0.0.1:8788/api/workflow-packs/import \
      -H 'content-type: application/json' \
      --data-binary @-
```

