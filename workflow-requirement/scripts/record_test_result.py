#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import datetime
from pathlib import Path

from cli_common import add_dry_run_arg, print_header
from md_board_utils import ensure_heading


SECTION_HEADING = "## 自动回写记录"


def add_list_block(block: list[str], label: str, values: list[str]) -> None:
    if not values:
        return
    block.append(f"- {label}：")
    for value in values:
        block.append(f"  - {value}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Append standardized test result block into markdown doc")
    parser.add_argument("--file", required=True)
    parser.add_argument("--title", default="测试结果回写")
    parser.add_argument("--status", default="pass", help="pass/fail/blocked")
    parser.add_argument("--summary", default="")
    parser.add_argument("--command", action="append", default=[])
    parser.add_argument("--result", action="append", default=[])
    parser.add_argument("--conclusion", action="append", default=[])
    parser.add_argument("--blocker", action="append", default=[])
    parser.add_argument("--alternative", action="append", default=[])
    parser.add_argument("--timestamp", default=datetime.now().strftime("%Y-%m-%d %H:%M:%S"))
    add_dry_run_arg(parser)
    args = parser.parse_args()

    target = Path(args.file)
    if not target.exists():
        print(f"Error: markdown file not found: {target}")
        return 1

    lines = target.read_text(encoding="utf-8").splitlines()
    ensure_heading(lines, SECTION_HEADING)

    block = [f"### {args.timestamp} {args.title}", f"- 状态：{args.status}"]
    if args.summary:
        block.append(f"- 摘要：{args.summary}")
    add_list_block(block, "命令", args.command)
    add_list_block(block, "结果", args.result)
    add_list_block(block, "结论", args.conclusion)
    add_list_block(block, "阻塞", args.blocker)
    add_list_block(block, "替代验证", args.alternative)

    print_header(
        "Test Result Record",
        {
            "file": args.file,
            "title": args.title,
            "status": args.status,
        },
    )
    for line in block:
        print(f"  {line}")

    if args.dry_run:
        return 0

    heading_idx = ensure_heading(lines, SECTION_HEADING)
    insert_idx = len(lines)
    for idx in range(heading_idx + 1, len(lines)):
        if idx > heading_idx + 1 and lines[idx].startswith("## "):
            insert_idx = idx
            break
    insert_block = []
    if insert_idx > 0 and lines[insert_idx - 1].strip():
        insert_block.append("")
    insert_block.extend(block)
    insert_block.append("")
    lines[insert_idx:insert_idx] = insert_block
    target.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("[PASS] test result appended")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
