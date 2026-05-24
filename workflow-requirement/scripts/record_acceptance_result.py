#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import datetime
from pathlib import Path

from cli_common import add_dry_run_arg, print_header
from md_board_utils import ensure_heading


SECTION_HEADING = "## 自动回写记录"
COMPACT_TABLE_HEADER = "| 时间 | 标题 | 状态 | 范围/验证 | 结论/下一步 |"
COMPACT_TABLE_SEPARATOR = "| --- | --- | --- | --- | --- |"
MAX_CELL_TEXT = 120


def add_list_block(block: list[str], label: str, values: list[str]) -> None:
    if not values:
        return
    block.append(f"- {label}：")
    for value in values:
        block.append(f"  - {value}")


def compact_text(value: str, max_chars: int = MAX_CELL_TEXT) -> str:
    text = " ".join((value or "").split())
    if not text:
        return ""
    if len(text) <= max_chars:
        return text
    return text[: max(0, max_chars - 3)].rstrip() + "..."


def compact_items(label: str, values: list[str], limit: int = 2, max_chars: int = 80) -> str:
    cleaned = [compact_text(value, max_chars) for value in values if compact_text(value, max_chars)]
    if not cleaned:
        return ""
    suffix = f"; +{len(cleaned) - limit}" if len(cleaned) > limit else ""
    return f"{label}: {'; '.join(cleaned[:limit])}{suffix}"


def table_cell(value: str) -> str:
    return compact_text(value).replace("|", "\\|")


def find_section_end(lines: list[str], heading_idx: int) -> int:
    for idx in range(heading_idx + 1, len(lines)):
        if idx > heading_idx + 1 and lines[idx].startswith("## "):
            return idx
    return len(lines)


def ensure_compact_table(lines: list[str], heading_idx: int) -> int:
    section_end = find_section_end(lines, heading_idx)
    for idx in range(heading_idx + 1, section_end):
        if lines[idx].strip() == COMPACT_TABLE_HEADER:
            return idx

    insert_idx = heading_idx + 1
    while insert_idx < section_end and lines[insert_idx].strip() == "":
        insert_idx += 1
    table_block = [COMPACT_TABLE_HEADER, COMPACT_TABLE_SEPARATOR, ""]
    lines[insert_idx:insert_idx] = table_block
    return insert_idx


def insert_compact_row(lines: list[str], heading_idx: int, row: str) -> None:
    header_idx = ensure_compact_table(lines, heading_idx)
    insert_idx = header_idx + 2
    while insert_idx < len(lines) and lines[insert_idx].strip().startswith("|"):
        insert_idx += 1
    lines[insert_idx:insert_idx] = [row]


def build_compact_row(args: argparse.Namespace) -> str:
    scope_parts = [
        compact_items("范围", args.scope, limit=2),
        compact_items("验证", args.verification, limit=2),
    ]
    conclusion_parts = [
        compact_items("结论", args.conclusion, limit=2),
        compact_items("阻塞", args.blocker, limit=2),
        compact_items("下一步", args.next_step, limit=1),
    ]
    scope_verification = "；".join(part for part in scope_parts if part) or compact_text(args.summary) or "未记录"
    conclusion = "；".join(part for part in conclusion_parts if part) or "待确认"
    return (
        f"| {table_cell(args.timestamp)} | {table_cell(args.title)} | {table_cell(args.status)} | "
        f"{table_cell(scope_verification)} | {table_cell(conclusion)} |"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Append standardized acceptance/interlock block into markdown doc")
    parser.add_argument("--file", required=True)
    parser.add_argument("--title", default="联调验收回写")
    parser.add_argument("--status", default="pass", help="pass/blocked/fail")
    parser.add_argument("--format", choices=("compact", "audit"), default="compact", dest="record_format")
    parser.add_argument("--summary", default="")
    parser.add_argument("--scope", action="append", default=[])
    parser.add_argument("--verification", action="append", default=[])
    parser.add_argument("--conclusion", action="append", default=[])
    parser.add_argument("--blocker", action="append", default=[])
    parser.add_argument("--next-step", action="append", default=[])
    parser.add_argument("--timestamp", default=datetime.now().strftime("%Y-%m-%d %H:%M:%S"))
    add_dry_run_arg(parser)
    args = parser.parse_args()

    target = Path(args.file)
    if not target.exists():
        print(f"Error: markdown file not found: {target}")
        return 1

    lines = target.read_text(encoding="utf-8").splitlines()
    ensure_heading(lines, SECTION_HEADING)

    if args.record_format == "audit":
        block = [f"### {args.timestamp} {args.title}", f"- 状态：{args.status}"]
        if args.summary:
            block.append(f"- 摘要：{args.summary}")
        add_list_block(block, "范围", args.scope)
        add_list_block(block, "验证", args.verification)
        add_list_block(block, "结论", args.conclusion)
        add_list_block(block, "阻塞", args.blocker)
        add_list_block(block, "下一步", args.next_step)
    else:
        block = [build_compact_row(args)]

    print_header(
        "Acceptance Record",
        {
            "file": args.file,
            "title": args.title,
            "status": args.status,
            "format": args.record_format,
        },
    )
    for line in block:
        print(f"  {line}")

    if args.dry_run:
        return 0

    heading_idx = ensure_heading(lines, SECTION_HEADING)
    if args.record_format == "compact":
        insert_compact_row(lines, heading_idx, block[0])
        target.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
        print("[PASS] compact acceptance record appended")
        return 0

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
    print("[PASS] acceptance record appended")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
