# Knowledgebase V7 Regression

Date: 2026-04-29

## Scope

V7 covers the personal AI workflow pack protocol:

- Schema and storage tables.
- Export, validate, import, and detail APIs.
- Example packs and compatibility notes.
- V1 read-only API and MCP tools for workflow packs.

## Verified Commands

### Build

```bash
cargo check
node --check scripts/kb-mcp-server.mjs
npm run build
```

Result: passed.

### Schema

```bash
curl -fsS http://127.0.0.1:8788/api/workflow-packs/schema
```

Result:

- `schema_version=1.0.0`
- 5 pack types returned.
- checksum algorithm is `sha256`.

### Example Validation

```bash
node -e "const fs=require('fs'); const p=process.argv[1]; process.stdout.write(JSON.stringify({package_json: JSON.parse(fs.readFileSync(p,'utf8'))}))" \
  workflow-statusbar/docs/workflow-pack-examples/minimal-development-handoff-pack.json \
  | curl -fsS -X POST http://127.0.0.1:8788/api/workflow-packs/validate \
      -H 'content-type: application/json' \
      --data-binary @-
```

Result:

- Minimal development handoff example: `valid=true`, 2 items, 3 warnings.
- Complete project knowledge example: `valid=true`, 4 items, 4 warnings.

Warnings are expected for sample source refs that do not exist in the local database.

### Import And Detail

```bash
curl -fsS -X POST http://127.0.0.1:8788/api/workflow-packs/import \
  -H 'content-type: application/json' \
  --data-binary @/tmp/v7-reg-import-body.json
```

Result:

- Imported pack: `workflow-pack-v7-regression-1777440198697`
- `GET /api/workflow-packs/:id` returned `status=imported`.
- Detail returned 2 indexed items.

### V1 Read-Only Context

```bash
curl -fsS http://127.0.0.1:8788/api/v1/workflow-packs/workflow-pack-v7-regression-1777440198697/task-context
```

Result:

- `readonly=true`
- `evidence.length=2`

### MCP

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"build_workflow_pack_context","arguments":{"pack_id":"workflow-pack-v7-regression-1777440198697"}}}' \
  | npm run --silent kb:mcp
```

Result:

- `tools/list` returned 7 tools.
- `build_workflow_pack_context` returned `readonly=true`.
- MCP context returned 2 evidence items.

## Residual Risk

- TASK-43 historical export samples may fail strict validation with `checksum_mismatch`. The V7 validation gate correctly detects and blocks this. Future export-side work should migrate or regenerate affected historical exported packages with the canonical checksum policy used by the examples and validator.

