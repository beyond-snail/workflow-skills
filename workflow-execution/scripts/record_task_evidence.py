#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import datetime
from pathlib import Path

from cli_common import add_dry_run_arg, print_header
def add_list_block(lines: list[str], label: str, values: list[str]) -> None:
    if not values:
        return
    lines.append(f"- {label}：")
    for value in values:
        lines.append(f"  - {value}")


def find_task_section(lines: list[str], task_id: str) -> int | None:
    heading = f"## {task_id}"
    for idx, line in enumerate(lines):
        if line.strip() == heading:
            return idx
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Append standardized evidence block for a task into a markdown file")
    parser.add_argument("--file", required=True, help="Target markdown file")
    parser.add_argument("--task-id", required=True, help="Task ID")
    parser.add_argument("--summary", help="One-line summary")
    parser.add_argument("--status", default="done", help="Evidence status, e.g. done/blocked")
    parser.add_argument("--verification", action="append", default=[], help="Verification item, repeatable")
    parser.add_argument("--artifact", action="append", default=[], help="Artifact or file path, repeatable")
    parser.add_argument("--blocker", action="append", default=[], help="Blocker item, repeatable")
    parser.add_argument("--next-step", action="append", default=[], help="Next step item, repeatable")
    parser.add_argument("--timestamp", default=datetime.now().strftime("%Y-%m-%d %H:%M:%S"), help="Timestamp label")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    target = Path(args.file)
    if not target.exists():
        print(f"Error: markdown file not found: {target}")
        return 1

    block: list[str] = [f"### {args.timestamp}", f"- 状态：{args.status}"]
    if args.summary:
        block.append(f"- 摘要：{args.summary}")
    add_list_block(block, "验证", args.verification)
    add_list_block(block, "产物", args.artifact)
    add_list_block(block, "阻塞", args.blocker)
    add_list_block(block, "下一步", args.next_step)

    print_header(
        "Task Evidence",
        {
            "file": args.file,
            "task_id": args.task_id,
            "status": args.status,
        },
    )
    print("- block_preview:")
    for line in block:
        print(f"  {line}")

    if args.dry_run:
        return 0

    content = target.read_text(encoding="utf-8")
    lines = content.splitlines()
    task_section_idx = find_task_section(lines, args.task_id)

    if task_section_idx is None:
        if lines and lines[-1].strip():
            lines.append("")
        lines.append(f"## {args.task_id}")
        lines.append("")
        task_section_idx = len(lines) - 2

    insert_idx = len(lines)
    for idx in range(task_section_idx + 1, len(lines)):
        if lines[idx].startswith("## ") and lines[idx].strip() != f"## {args.task_id}":
            insert_idx = idx
            break

    insert_block = []
    if insert_idx > 0 and lines[insert_idx - 1].strip():
        insert_block.append("")
    insert_block.extend(block)
    insert_block.append("")
    lines[insert_idx:insert_idx] = insert_block

    target.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("[PASS] evidence appended")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
