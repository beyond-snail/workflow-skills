# Knowledgebase V5 Regression

Date: 2026-04-29

## Scope

This regression covers the V5 MCP/API feature set:

- V1 read-only API contract
- stdio MCP tools
- API client and call logs
- write protection
- external client documentation examples

## Commands

```bash
cargo check
node --check scripts/kb-mcp-server.mjs
npm run build
```

End-to-end smoke:

```bash
cargo run --manifest-path src-tauri/Cargo.toml
```

Then verify:

- `GET /api/v1/search?q=retro`
- `GET /api/v1/templates?status=verified`
- `POST /api/v1/task-context`
- `GET /api/v1/evidence/<item-id>`
- `GET /api/v1/health`
- MCP `tools/list`
- MCP `tools/call` for `search_memory`, `build_task_context`, `list_asset_health`
- `POST /api/v1/search?q=retro` write protection
- `GET /api/v1/call-logs?limit=30`

## Result

All checks passed.

Observed smoke summary:

```json
{
  "apiReadonly": true,
  "tools": [
    "search_memory",
    "get_prompt_template",
    "build_task_context",
    "get_evidence_trace",
    "list_asset_health"
  ],
  "contextSession": "",
  "writeStatus": "403",
  "logs": 23
}
```

## Residual Risk

- Raw HTTP clients must URL encode non-ASCII query parameters.
- V1/MCP intentionally rejects write-like requests; future write workflows should use a confirmation queue.
