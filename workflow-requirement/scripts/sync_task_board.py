#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import date
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args
from md_board_utils import (
    find_section_heading,
    find_task_row,
    format_md_row,
    preserve_cell_style,
    update_timestamp,
)


TABLE_HEADER = "| 任务ID | 任务标题 | 状态 | 验收标准 | 关联文档 |"
TABLE_SEPARATOR = "| --- | --- | --- | --- | --- |"
DEFAULT_TASK_BOARD = """# 任务看板

更新时间：{sync_date}

## 状态定义

- `todo`：待开始。
- `doing`：进行中。
- `blocked`：被依赖或问题阻塞。
- `done`：已完成。
"""


def build_task_row(task_id: str, title: str, status: str, acceptance: str, doc_link: str) -> str:
    raw_cells = [f"`{task_id}`", title, f"`{status}`", acceptance, doc_link]
    return format_md_row(raw_cells)


def ensure_task_board(task_path: Path, sync_date: str) -> None:
    if task_path.exists():
        return
    task_path.parent.mkdir(parents=True, exist_ok=True)
    task_path.write_text(DEFAULT_TASK_BOARD.format(sync_date=sync_date) + "\n", encoding="utf-8")


def sync_task_board_entry(
    task_path: Path,
    req_id: str,
    req_title: str,
    task_id: str,
    task_title: str,
    status: str,
    acceptance: str,
    doc_link: str,
    sync_date: str,
    dry_run: bool,
) -> int:
    ensure_task_board(task_path, sync_date)
    lines = task_path.read_text(encoding="utf-8").splitlines()
    row = find_task_row(task_path, task_id)

    print("# Task Board Sync")
    print(f"- req_id: {req_id}")
    print(f"- task_id: {task_id}")
    print(f"- task_title: {task_title}")
    print(f"- status: {status}")
    print(f"- task_file: {task_path}")

    if row is not None:
        task_idx = row.header_map.get("任务id")
        title_idx = row.header_map.get("任务标题")
        status_idx = row.header_map.get("状态")
        acceptance_idx = row.header_map.get("验收标准")
        doc_idx = row.header_map.get("关联文档")
        new_raw_cells = list(row.raw_cells)
        if task_idx is not None:
            new_raw_cells[task_idx] = preserve_cell_style(row.raw_cells[task_idx], task_id)
        if title_idx is not None:
            new_raw_cells[title_idx] = task_title
        if status_idx is not None:
            new_raw_cells[status_idx] = preserve_cell_style(row.raw_cells[status_idx], status)
        if acceptance_idx is not None:
            new_raw_cells[acceptance_idx] = acceptance
        if doc_idx is not None:
            new_raw_cells[doc_idx] = doc_link
        new_line = format_md_row(new_raw_cells)
        print("- action: update-existing-row")
        print(f"- new_line: {new_line}")
        if dry_run:
            return 0
        lines[row.line_index] = new_line
        update_timestamp(lines, sync_date)
        task_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print("[PASS] task row updated")
        return 0

    section_heading = f"## {req_id} {req_title}"
    heading_idx = find_section_heading(lines, section_heading)
    new_line = build_task_row(task_id, task_title, status, acceptance, doc_link)
    print("- action: append-new-row")
    print(f"- new_line: {new_line}")

    if dry_run:
        return 0

    if heading_idx is None:
        if lines and lines[-1].strip():
            lines.append("")
        lines.extend([section_heading, "", TABLE_HEADER, TABLE_SEPARATOR, new_line])
    else:
        insert_idx = heading_idx + 1
        found_table = False
        while insert_idx < len(lines):
            if lines[insert_idx].strip() == TABLE_HEADER:
                found_table = True
                insert_idx += 2
                while insert_idx < len(lines) and lines[insert_idx].strip().startswith("|"):
                    insert_idx += 1
                break
            if lines[insert_idx].startswith("## "):
                break
            insert_idx += 1

        if not found_table:
            block = ["", TABLE_HEADER, TABLE_SEPARATOR, new_line]
            lines[heading_idx + 1:heading_idx + 1] = block
        else:
            lines.insert(insert_idx, new_line)

    update_timestamp(lines, sync_date)
    task_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("[PASS] task row appended")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Sync one task row into markdown task board")
    add_profile_arg(parser)
    parser.add_argument("--task-file", help="Path to 任务看板.md")
    parser.add_argument("--req-id", required=True)
    parser.add_argument("--req-title", required=True)
    parser.add_argument("--task-id", required=True)
    parser.add_argument("--task-title", required=True)
    parser.add_argument("--status", default="todo")
    parser.add_argument("--acceptance", default="待补充")
    parser.add_argument("--doc-link", default="-")
    parser.add_argument("--sync-date", default=date.today().isoformat())
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    task_default = profile.get("paths", {}).get("task_board", "doc/requirements/任务看板.md")
    task_path = Path(args.task_file).resolve() if args.task_file else Path.cwd() / task_default
    return sync_task_board_entry(
        task_path=task_path,
        req_id=args.req_id,
        req_title=args.req_title,
        task_id=args.task_id,
        task_title=args.task_title,
        status=args.status,
        acceptance=args.acceptance,
        doc_link=args.doc_link,
        sync_date=args.sync_date,
        dry_run=args.dry_run,
    )


if __name__ == "__main__":
    raise SystemExit(main())
