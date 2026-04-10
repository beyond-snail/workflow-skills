#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import date
from pathlib import Path
from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import find_task_row, format_md_row, get_cell, preserve_cell_style
from profile_paths import ProjectPaths


VALID_STATUSES = {"todo", "doing", "blocked", "done"}


def update_timestamp(lines: list[str], today: str) -> None:
    for idx, line in enumerate(lines):
        if line.startswith("更新时间："):
            lines[idx] = f"更新时间：{today}"
            return


def main() -> int:
    parser = argparse.ArgumentParser(description="Update task status in markdown task board")
    add_profile_arg(parser)
    parser.add_argument("--task-file", help="Path to markdown task board")
    parser.add_argument("--task-id", required=True, help="Task ID")
    parser.add_argument("--status", required=True, help="New task status")
    parser.add_argument("--expected-current", help="Optional expected current status")
    parser.add_argument("--sync-date", default=date.today().isoformat(), help="Date used for 更新时间")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    new_status = args.status.strip().lower()
    if new_status not in VALID_STATUSES:
        print(f"Error: unsupported status: {args.status}")
        print(f"Supported: {', '.join(sorted(VALID_STATUSES))}")
        return 1

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    task_path = Path(args.task_file).resolve() if args.task_file else project_paths.task_board
    if not task_path.exists():
        print(f"Error: task file not found: {task_path}")
        return 1

    row = find_task_row(task_path, args.task_id)
    if row is None:
        print(f"Error: task {args.task_id} not found in {args.task_file}")
        return 1

    status_idx = row.header_map.get("状态")
    if status_idx is None or status_idx >= len(row.raw_cells):
        print("Error: 状态列不存在")
        return 1

    current_status = get_cell(row.cells, row.header_map, ("状态",)) or ""
    task_title = get_cell(row.cells, row.header_map, ("任务标题", "任务名称", "标题")) or ""
    req_id = get_cell(row.cells, row.header_map, ("需求ID", "reqId", "req_id")) or row.section_req_id or ""

    if args.expected_current and current_status != args.expected_current.strip().lower():
        print(f"Error: current status mismatch, expected {args.expected_current}, actual {current_status}")
        return 1

    new_raw_cells = list(row.raw_cells)
    new_raw_cells[status_idx] = preserve_cell_style(row.raw_cells[status_idx], new_status)
    new_line = format_md_row(new_raw_cells)

    print_header(
        "Task Status Update",
        {
            "task_id": args.task_id,
            "task_title": task_title,
            "req_id": req_id,
            "old_status": current_status,
            "new_status": new_status,
            "task_file": str(task_path),
        },
    )

    if args.dry_run:
        print("- mode: dry-run")
        print(f"- new_line: {new_line}")
        return 0

    lines = task_path.read_text(encoding="utf-8").splitlines()
    lines[row.line_index] = new_line
    update_timestamp(lines, args.sync_date)
    task_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print("[PASS] task status updated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
