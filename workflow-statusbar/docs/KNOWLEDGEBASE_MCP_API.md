# Knowledgebase API / MCP Guide

This document explains how local AI clients can read the workflow knowledgebase through the V1 API or the stdio MCP server.

## Requirements

1. Start `workflow-statusbar`; the knowledgebase HTTP server must be reachable at `http://127.0.0.1:8788`.
2. Run commands from the `workflow-statusbar` directory.
3. Keep the service local. The V1 API and MCP server are designed for localhost usage, not public network exposure.

## Permission Model

- All V1 API responses are read-only.
- The MCP server only exposes read tools.
- External AI clients must not write directly to the formal knowledgebase.
- Write-like V1 requests are rejected with `403` and `write_protected`.
- Non-ASCII query parameters, including Chinese text, must be URL encoded when using raw HTTP.

## MCP Server

Start the MCP server:

```bash
cd workflow-statusbar
npm run kb:mcp
```

Optional API base URL override:

```bash
KB_API_BASE_URL=http://127.0.0.1:8788 npm run kb:mcp
```

The server uses stdio JSON-RPC and exposes these tools:

| Tool | Purpose | Main Arguments |
| --- | --- | --- |
| `search_memory` | Search historical knowledge items | `query` |
| `get_prompt_template` | List or rank prompt templates | `scene`, `status` |
| `build_task_context` | Build a read-only task context package | `input`, `limit` |
| `get_evidence_trace` | Read one evidence item and trace | `id` |
| `list_asset_health` | Read health summary and suggested actions | none |

### MCP Smoke Test

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_memory","arguments":{"query":"retro"}}}' \
  | npm run --silent kb:mcp
```

Expected result:

- `tools/list` returns the five tools above.
- Tool responses contain JSON text with `readonly: true`.
- `build_task_context` returns an empty `session_id`, because it does not create starter history.

## Client Examples

### Codex-style MCP Config

Use the command below as the MCP server command in a local client configuration:

```json
{
  "mcpServers": {
    "workflow-knowledgebase": {
      "command": "npm",
      "args": ["run", "--silent", "kb:mcp"],
      "cwd": "/Users/wucongpeng/Documents/ai/skill/workflow-skills-copy/workflow-statusbar",
      "env": {
        "KB_API_BASE_URL": "http://127.0.0.1:8788"
      }
    }
  }
}
```

### Claude-style Desktop Config

```json
{
  "mcpServers": {
    "workflow-knowledgebase": {
      "command": "node",
      "args": [
        "/Users/wucongpeng/Documents/ai/skill/workflow-skills-copy/workflow-statusbar/scripts/kb-mcp-server.mjs"
      ],
      "env": {
        "KB_API_BASE_URL": "http://127.0.0.1:8788"
      }
    }
  }
}
```

### ChatGPT-style Tool Bridge

When a client cannot run MCP directly, call the V1 HTTP API from a local tool bridge. Always include client headers so calls are easy to diagnose:

```bash
curl -fsS 'http://127.0.0.1:8788/api/v1/search?q=retro' \
  -H 'x-kb-client: local-tool-bridge' \
  -H 'x-kb-tool: search_memory' \
  -H 'x-kb-params: query=retro'
```

## V1 HTTP API

### Search Memory

```bash
curl -fsS 'http://127.0.0.1:8788/api/v1/search?q=retro'
```

### List Prompt Templates

```bash
curl -fsS 'http://127.0.0.1:8788/api/v1/templates?status=verified'
```

### Build Task Context

```bash
curl -fsS 'http://127.0.0.1:8788/api/v1/task-context' \
  -H 'content-type: application/json' \
  -d '{"input_text":"TASK-2026-04-28-33 外部 AI 使用文档","limit":5}'
```

### Get Evidence Trace

```bash
curl -fsS 'http://127.0.0.1:8788/api/v1/evidence/<item-id>'
```

### List Asset Health

```bash
curl -fsS 'http://127.0.0.1:8788/api/v1/health'
```

### Inspect Call Logs

```bash
curl -fsS 'http://127.0.0.1:8788/api/v1/call-logs?limit=20'
```

Call log fields include:

- `client_name`
- `tool_name`
- `params_summary`
- `duration_ms`
- `status_code`
- `error_message`
- `created_at`

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `connection refused` | Confirm `workflow-statusbar` is running and `http://127.0.0.1:8788/api/stats` responds. |
| Chinese query fails in raw curl | URL encode the query string, or send JSON body where supported. |
| MCP tool returns `api_error` | Query `/api/v1/call-logs?limit=20` and inspect `status_code` / `error_message`. |
| Empty `session_id` from `build_task_context` | Expected behavior. The V1 task-context API is read-only and does not create history. |
| Need write access | Not supported by V1/MCP. Use the Web UI or wait for the confirmation queue task. |
| `write_protected` | The client used a write-like method or unknown V1 write path. Use a documented read tool instead. |
