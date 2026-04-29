# Workflow Pack Compatibility

## Current Version

- Current schema: `1.0.0`
- Checksum algorithm: `sha256`
- Checksum input: canonical JSON built from `schema_version`, `pack_type`, `title`, `source`, `items`, and `markdown`.
- Excluded from checksum: `pack_id`, `created_at`, and `checksum`.

## Import Rules

- `error` issues block import.
- `warning` issues do not block import.
- Missing local source refs are warnings because a pack may come from another machine.
- Checksum mismatch, unsupported pack type, incompatible schema version, missing required fields, and same `pack_id` with different checksum are errors.
- Import stores the pack as `workflow_packs.status=imported` and writes `workflow_pack_items` index rows.
- Import does not write formal `items` knowledge records.

## Forward Compatibility

- Readers should ignore unknown optional fields.
- Writers must keep required envelope and item fields.
- New `pack_type` values require schema endpoint and docs updates.
- A future `1.1.x` version may add optional sections without breaking `1.0.0` readers.
- A future `2.0.0` version may change required fields and should be treated as incompatible until a migration exists.

## Migration Notes

- Validate before import.
- If validation reports `checksum_mismatch`, regenerate checksum from canonical package content before retrying.
- If validation reports `missing_source_ref`, import can continue, but the user should treat those entries as candidate evidence until local sources are collected.
- If validation reports `pack_id_conflict`, change `pack_id` only when the package is intentionally forked; otherwise keep the existing local pack.

