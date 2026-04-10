#!/usr/bin/env python3
from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import get_cell, parse_table_rows
from profile_paths import ProjectPaths


@dataclass
class TaskRecord:
    task_id: str
    title: str
    status: str
    req_id: str


def load_tasks(task_file: Path) -> list[TaskRecord]:
    tasks: list[TaskRecord] = []
    for row in parse_table_rows(task_file):
        header_map = row.header_map
        task_id = get_cell(row.cells, header_map, ("任务ID", "taskId", "task_id"))
        title = get_cell(row.cells, header_map, ("任务标题", "任务名称", "标题"))
        status = get_cell(row.cells, header_map, ("状态",))
        req_id = get_cell(row.cells, header_map, ("需求ID", "reqId", "req_id")) or row.section_req_id
        if not task_id or not title or not status:
            continue
        tasks.append(TaskRecord(task_id=task_id, title=title, status=status.lower(), req_id=req_id or ""))
    return tasks


def main() -> int:
    parser = argparse.ArgumentParser(description="Select current doing task or earliest todo task from markdown board")
    add_profile_arg(parser)
    parser.add_argument("--task-file", help="Task board markdown path")
    parser.add_argument("--req-id", help="Optional requirement id filter")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    task_path = Path(args.task_file).resolve() if args.task_file else project_paths.task_board

    if not task_path.exists():
        print(f"Error: task file not found: {task_path}")
        return 1

    tasks = load_tasks(task_path)
    if args.req_id:
        tasks = [task for task in tasks if task.req_id == args.req_id]

    current = next((task for task in tasks if task.status == "doing"), None)
    selected = current or next((task for task in tasks if task.status == "todo"), None)

    print_header(
        "Select Next Task",
        {
            "task_file": str(task_path),
            "req_id": args.req_id,
            "parsed_tasks": str(len(tasks)),
        },
    )

    if selected is None:
        print("[PASS] 当前没有 `doing/todo` 任务，说明该范围可能已收口")
        return 0

    print(f"- selected_task_id: {selected.task_id}")
    print(f"- selected_title: {selected.title}")
    print(f"- selected_status: {selected.status}")
    print(f"- selected_req_id: {selected.req_id}")
    print(f"- selection_reason: {'current doing' if current else 'earliest todo'}")

    if args.dry_run:
        return 0

    print("[PASS] 已选择下一条执行任务")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
