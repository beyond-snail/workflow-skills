#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import date
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from profile_paths import ProjectPaths


HEADER = "| 任务ID | 任务名称 | 类型 | 关联需求/关键词 | 状态 | 最新结论 | 目录 | 最后更新 |"
SEPARATOR = "| --- | --- | --- | --- | --- | --- | --- | --- |"


def row_for(args: argparse.Namespace) -> str:
    return (
        f"| `{args.task_id}` | {args.title} | {args.type} | {args.keywords} | {args.status} | "
        f"{args.latest} | `{args.directory}` | {args.updated_at} |"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Sync task index entry")
    add_profile_arg(parser)
    parser.add_argument("--task-id", required=True)
    parser.add_argument("--title", required=True)
    parser.add_argument("--type", default="feature")
    parser.add_argument("--keywords", default="")
    parser.add_argument("--status", default="todo")
    parser.add_argument("--latest", default="")
    parser.add_argument("--directory", required=True)
    parser.add_argument("--updated-at", default=date.today().isoformat())
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    index_file = project_paths.tasks_index
    new_row = row_for(args)

    print_header(
        "Sync Task Index",
        {
            "task_id": args.task_id,
            "index": str(index_file),
            "mode": "dry-run" if args.dry_run else "live",
        },
    )
    print(f"- row: {new_row}")

    if args.dry_run:
        return 0

    content = index_file.read_text(encoding="utf-8") if index_file.exists() else ""
    lines = content.splitlines()
    replaced = False
    for i, line in enumerate(lines):
        if f"`{args.task_id}`" in line:
            lines[i] = new_row
            replaced = True
            break
    if not replaced:
        try:
            anchor = lines.index(SEPARATOR, lines.index(HEADER))
        except ValueError:
            if lines and lines[-1].strip():
                lines.append("")
            lines.extend([HEADER, SEPARATOR, new_row])
        else:
            insert_at = anchor + 1
            while insert_at < len(lines) and lines[insert_at].startswith("| "):
                insert_at += 1
            lines.insert(insert_at, new_row)
    index_file.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("[PASS] task index synced")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
