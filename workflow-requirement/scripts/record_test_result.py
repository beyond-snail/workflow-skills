#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from datetime import datetime
from pathlib import Path

from cli_common import add_dry_run_arg, print_header
from md_board_utils import ensure_heading


SECTION_HEADING = "## 自动回写记录"
COMPACT_TABLE_HEADER = "| 时间 | 标题 | 状态 | 验证 | 结论 |"
COMPACT_TABLE_SEPARATOR = "| --- | --- | --- | --- | --- |"
MAX_CELL_TEXT = 120


def add_list_block(block: list[str], label: str, values: list[str]) -> None:
    if not values:
        return
    block.append(f"- {label}：")
    for value in values:
        block.append(f"  - {value}")


def add_step_table(block: list[str], step_rows: list[dict[str, str]]) -> None:
    if not step_rows:
        return
    block.append("- 详细步骤：")
    block.append("")
    block.append("| 步骤ID | 动作 | 预期 | 实际 | 证据 |")
    block.append("| --- | --- | --- | --- | --- |")
    for idx, row in enumerate(step_rows, start=1):
        step_id = row.get("id") or f"STEP-{idx:02d}"
        action = row.get("action") or "待补充"
        expected = row.get("expected") or "待补充"
        actual = row.get("actual") or "待补充"
        evidence = row.get("evidence") or "自动回写记录"
        block.append(f"| {step_id} | {action} | {expected} | {actual} | {evidence} |")


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
    verification_parts = [
        compact_items("命令", args.command, limit=2),
        compact_items("结果", args.result, limit=2),
    ]
    conclusion_parts = [
        compact_items("结论", args.conclusion, limit=2),
        compact_items("阻塞", args.blocker, limit=2),
        compact_items("替代", args.alternative, limit=1),
    ]
    verification = "；".join(part for part in verification_parts if part) or compact_text(args.summary) or "未记录"
    conclusion = "；".join(part for part in conclusion_parts if part) or "待确认"
    return (
        f"| {table_cell(args.timestamp)} | {table_cell(args.title)} | {table_cell(args.status)} | "
        f"{table_cell(verification)} | {table_cell(conclusion)} |"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Append standardized test result block into markdown doc")
    parser.add_argument("--file", required=True)
    parser.add_argument("--title", default="测试结果回写")
    parser.add_argument("--status", default="pass", help="pass/fail/blocked")
    parser.add_argument("--format", choices=("compact", "audit"), default="compact", dest="record_format")
    parser.add_argument("--summary", default="")
    parser.add_argument("--command", action="append", default=[])
    parser.add_argument("--result", action="append", default=[])
    parser.add_argument("--conclusion", action="append", default=[])
    parser.add_argument("--blocker", action="append", default=[])
    parser.add_argument("--alternative", action="append", default=[])
    parser.add_argument(
        "--step",
        action="append",
        default=[],
        help="Step row JSON string, keys: id/action/expected/actual/evidence",
    )
    parser.add_argument("--timestamp", default=datetime.now().strftime("%Y-%m-%d %H:%M:%S"))
    add_dry_run_arg(parser)
    args = parser.parse_args()

    target = Path(args.file)
    if not target.exists():
        print(f"Error: markdown file not found: {target}")
        return 1

    lines = target.read_text(encoding="utf-8").splitlines()
    ensure_heading(lines, SECTION_HEADING)

    step_rows: list[dict[str, str]] = []
    for raw in args.step:
        payload = raw.strip()
        if not payload:
            continue
        try:
            item = json.loads(payload)
        except json.JSONDecodeError as exc:
            print(f"Error: invalid --step JSON: {payload} ({exc})")
            return 1
        if not isinstance(item, dict):
            print(f"Error: --step must be JSON object: {payload}")
            return 1
        step_rows.append({str(k): str(v) for k, v in item.items()})

    if args.record_format == "audit":
        block = [f"### {args.timestamp} {args.title}", f"- 状态：{args.status}"]
        if args.summary:
            block.append(f"- 摘要：{args.summary}")
        add_list_block(block, "命令", args.command)
        add_list_block(block, "结果", args.result)
        add_step_table(block, step_rows)
        add_list_block(block, "结论", args.conclusion)
        add_list_block(block, "阻塞", args.blocker)
        add_list_block(block, "替代验证", args.alternative)
    else:
        block = [build_compact_row(args)]

    print_header(
        "Test Result Record",
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
        print("[PASS] compact test result appended")
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
    print("[PASS] test result appended")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
