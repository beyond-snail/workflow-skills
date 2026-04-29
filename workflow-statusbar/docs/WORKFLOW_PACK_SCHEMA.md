# Workflow Pack Schema

Version: `1.0.0`

## Envelope

Every workflow pack uses the same envelope:

```json
{
  "schema_version": "1.0.0",
  "pack_type": "development_handoff_pack",
  "pack_id": "workflow-pack-example",
  "title": "Example development handoff",
  "created_at": "2026-04-29T00:00:00Z",
  "source": {
    "project_id": "proj-example",
    "req_id": "REQ-YYYY-MM-DD-00",
    "task_id": "TASK-YYYY-MM-DD-00"
  },
  "items": [],
  "markdown": "",
  "checksum": "sha256:..."
}
```

Required envelope fields:

- `schema_version`
- `pack_type`
- `pack_id`
- `title`
- `created_at`
- `source`
- `items`
- `markdown`
- `checksum`

## Item

Each `items[]` entry points to one piece of evidence, source material, command result, template, or generated section.

```json
{
  "item_id": "item-example",
  "item_type": "evidence",
  "title": "Relevant source",
  "source_ref": "items:item-id",
  "required": true,
  "payload": {}
}
```

Required item fields:

- `item_id`
- `item_type`
- `title`
- `source_ref`
- `required`
- `payload`

## Pack Types

| pack_type | Purpose | Required sections |
| --- | --- | --- |
| `requirement_context_pack` | PRD and requirement context | `metadata`, `requirement`, `tasks`, `evidence_index`, `acceptance` |
| `development_handoff_pack` | Task handoff for development AI | `metadata`, `task`, `context`, `files`, `risks`, `verification` |
| `verification_evidence_pack` | Build/API/UI verification evidence | `metadata`, `commands`, `api_smoke`, `ui_checks`, `risks` |
| `retrospective_pack` | Retrospective and deposition suggestions | `metadata`, `summary`, `lessons`, `suggestions`, `starter_evaluation` |
| `project_knowledge_pack` | Project-level migration or handoff | `metadata`, `project`, `health`, `evidence_index`, `templates`, `actions` |

## Runtime Contract

- Schema endpoint: `GET /api/workflow-packs/schema`
- Export endpoint: `POST /api/workflow-packs/export`
- Checksum algorithm: `sha256`
- Import/validate endpoints are intentionally left for the following V7 tasks.

### Export Examples

Development handoff pack:

```bash
curl -X POST http://127.0.0.1:8788/api/workflow-packs/export \
  -H 'content-type: application/json' \
  -d '{"pack_type":"development_handoff_pack","input_text":"TASK-YYYY-MM-DD-00","limit":8}'
```

Project knowledge pack:

```bash
curl -X POST http://127.0.0.1:8788/api/workflow-packs/export \
  -H 'content-type: application/json' \
  -d '{"pack_type":"project_knowledge_pack","project_id":"proj-example"}'
```
