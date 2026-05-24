#!/usr/bin/env python3
"""Write a compact recovery brief for Codex context compaction."""
from __future__ import annotations

import argparse
from datetime import datetime
from pathlib import Path

from cli_common import add_dry_run_arg, print_header


MAX_TEXT = 160
MAX_ITEMS = 5


def compact_text(value: str, max_chars: int = MAX_TEXT) -> str:
    text = " ".join((value or "").split())
    if not text:
        return ""
    if len(text) <= max_chars:
        return text
    return text[:max_chars].rstrip() + f"... <truncated {len(text) - max_chars} chars>"


def unique(items: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for item in items:
        text = item.strip()
        if not text or text in seen:
            continue
        seen.add(text)
        result.append(text)
    return result


def display_path(path_text: str, root: Path) -> str:
    text = path_text.strip()
    if not text:
        return ""
    path = Path(text)
    if not path.is_absolute():
        return text
    try:
        return str(path.resolve().relative_to(root))
    except ValueError:
        return str(path)


def inline_items(items: list[str], empty: str = "无") -> str:
    cleaned = unique([compact_text(item) for item in items])[:MAX_ITEMS]
    if not cleaned:
        return empty
    return "；".join(cleaned)


def build_brief(args: argparse.Namespace, root: Path) -> str:
    updated_at = datetime.now().astimezone().isoformat(timespec="seconds")
    changed_files = unique([display_path(item, root) for item in args.changed_file])[:MAX_ITEMS]
    evidence = unique([display_path(item, root) for item in args.evidence])[:MAX_ITEMS]

    lines = [
        "# Context Brief",
        "",
        f"- updated_at: {updated_at}",
        f"- workspace: {root}",
        f"- requirement: `{args.req_id or 'N/A'}` {compact_text(args.req_title) or '未指定'}",
        f"- task: `{args.task_id or 'N/A'}` {compact_text(args.task_title) or '未指定'}",
        f"- status: {args.task_status or 'unknown'} / {args.mode or 'unknown'}",
        f"- summary: {compact_text(args.summary) or '待补充'}",
        f"- files: {inline_items(changed_files)}",
        f"- evidence: {inline_items(evidence)}",
        f"- verified: {inline_items(args.verification)}",
        f"- risk: {inline_items(args.blocker)}",
        f"- next: {inline_items(args.next_step)}",
        "",
    ]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Update .ai/memory/context-brief.md for compact recovery")
    parser.add_argument("--workspace-root", default=".", help="Project workspace root")
    parser.add_argument("--req-id", default="")
    parser.add_argument("--req-title", default="")
    parser.add_argument("--task-id", default="")
    parser.add_argument("--task-title", default="")
    parser.add_argument("--task-status", default="")
    parser.add_argument("--mode", default="")
    parser.add_argument("--summary", default="")
    parser.add_argument("--changed-file", action="append", default=[])
    parser.add_argument("--evidence", action="append", default=[])
    parser.add_argument("--verification", action="append", default=[])
    parser.add_argument("--blocker", action="append", default=[])
    parser.add_argument("--next-step", action="append", default=[])
    add_dry_run_arg(parser)
    args = parser.parse_args()

    root = Path(args.workspace_root).expanduser().resolve()
    target = root / ".ai/memory/context-brief.md"
    content = build_brief(args, root)

    print_header(
        "Context Brief Update",
        {
            "target": str(target),
            "mode": "dry-run" if args.dry_run else "live",
            "bytes": str(len(content.encode("utf-8"))),
        },
    )
    if args.dry_run:
        print(content)
        return 0

    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8")
    print(f"- context_brief: {target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
