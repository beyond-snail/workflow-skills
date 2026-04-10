#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path

from cli_common import add_dry_run_arg, print_header
from md_board_utils import format_md_row, split_md_table, split_md_table_raw


TRACE_SECTION = "## 追溯清单"
TRACE_HEADER = "| PRD章节 | PRD需求描述 | 设计/代码/测试对应位置 | 完成状态 | 备注 |"
TRACE_SEPARATOR = "|----------|-------------|------------------------|----------|------|"
UNIMPLEMENTED_SECTION = "## 未实现需求说明"
UNIMPLEMENTED_HEADER = "| PRD章节 | PRD需求描述 | 未实现原因 | 计划处理时间 |"
UNIMPLEMENTED_SEPARATOR = "|----------|-------------|------------|--------------|"


def ensure_table(lines: list[str], heading: str, header: str, separator: str) -> int:
    heading_idx = next((idx for idx, line in enumerate(lines) if line.strip() == heading), None)
    if heading_idx is None:
        if lines and lines[-1].strip():
            lines.append("")
        lines.extend([heading, "", header, separator])
        return len(lines) - 1

    idx = heading_idx + 1
    while idx < len(lines) and lines[idx].strip() == "":
        idx += 1
    if idx >= len(lines) or lines[idx].strip() != header:
        lines[idx:idx] = [header, separator]
        return idx + 1
    return idx + 1


def upsert_row(lines: list[str], start_idx: int, key: str, row_line: str) -> str:
    idx = start_idx
    while idx < len(lines):
        stripped = lines[idx].strip()
        if not stripped.startswith("|"):
            break
        cells = split_md_table(stripped)
        if cells and cells[0] == key:
            lines[idx] = row_line
            return "update-existing-row"
        idx += 1
    lines.insert(idx, row_line)
    return "append-new-row"


def main() -> int:
    parser = argparse.ArgumentParser(description="Upsert rows into PRD trace markdown tables")
    parser.add_argument("--file", required=True)
    parser.add_argument("--mode", choices=("trace", "unimplemented"), default="trace")
    parser.add_argument("--prd-section", required=True)
    parser.add_argument("--prd-desc", required=True)
    parser.add_argument("--mapping", help="Used in trace mode")
    parser.add_argument("--status", default="待补充", help="Used in trace mode")
    parser.add_argument("--note", default="", help="Used in trace mode")
    parser.add_argument("--reason", help="Used in unimplemented mode")
    parser.add_argument("--plan-date", default="", help="Used in unimplemented mode")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    target = Path(args.file)
    if not target.exists():
        print(f"Error: markdown file not found: {target}")
        return 1

    lines = target.read_text(encoding="utf-8").splitlines()

    if args.mode == "trace":
        start_idx = ensure_table(lines, TRACE_SECTION, TRACE_HEADER, TRACE_SEPARATOR)
        row_line = format_md_row(
            [
                f"`{args.prd_section}`",
                args.prd_desc,
                args.mapping or "",
                f"`{args.status}`",
                args.note,
            ]
        )
    else:
        start_idx = ensure_table(lines, UNIMPLEMENTED_SECTION, UNIMPLEMENTED_HEADER, UNIMPLEMENTED_SEPARATOR)
        row_line = format_md_row(
            [
                f"`{args.prd_section}`",
                args.prd_desc,
                args.reason or "",
                args.plan_date,
            ]
        )

    print_header(
        "PRD Trace Sync",
        {
            "file": args.file,
            "mode": args.mode,
            "prd_section": args.prd_section,
        },
    )
    print(f"- row_line: {row_line}")

    if args.dry_run:
        return 0

    action = upsert_row(lines, start_idx, args.prd_section, row_line)
    target.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print(f"[PASS] {action}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
