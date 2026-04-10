#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path
from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import find_task_row, get_cell
from profile_paths import ProjectPaths


def find_task(task_file: Path, task_id: str) -> tuple[str | None, str | None]:
    row = find_task_row(task_file, task_id)
    if row is None:
        return None, None
    title = get_cell(row.cells, row.header_map, ("任务标题", "任务名称", "标题"))
    req_id = get_cell(row.cells, row.header_map, ("需求ID", "reqId", "req_id")) or row.section_req_id
    return title, req_id


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate standardized commit message for a task")
    add_profile_arg(parser)
    parser.add_argument("--task-file", help="Path to task board, e.g. doc/requirements/任务看板.md")
    parser.add_argument("--task-id", required=True, help="Task ID, e.g. TASK-2026-03-19-01")
    parser.add_argument("--type", default="feat", help="Commit type: feat|fix|docs|refactor|test|chore")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    task_path = Path(args.task_file).resolve() if args.task_file else project_paths.task_board
    if not task_path.exists():
        print(f"Error: task file not found: {task_path}")
        return 1

    task_title, req_id = find_task(task_path, args.task_id)
    if not task_title:
        print(f"Error: task {args.task_id} not found in {args.task_file}")
        return 1

    if req_id:
        commit_msg = f"{args.type}(task): {args.task_id} {task_title}\n\nRefs: {req_id}"
    else:
        commit_msg = f"{args.type}(task): {args.task_id} {task_title}"

    if args.dry_run:
        print("# Dry Run")
        print(f"- task_id: {args.task_id}")
        print(f"- task_title: {task_title}")
        print(f"- req_id: {req_id or '(not found)'}")

    print("# Generated commit message:")
    print("-" * 60)
    print(commit_msg)
    print("-" * 60)
    print("\nRecommended staged commit flow:")
    print("1. git status --short")
    print("2. git add <specific-files>")
    print(f'3. git commit -m "{commit_msg}"')

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
