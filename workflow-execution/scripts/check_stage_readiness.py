#!/usr/bin/env python3
from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import get_cell, normalize, parse_table_rows
from profile_paths import ProjectPaths


@dataclass
class TaskRow:
    task_id: str
    req_id: str
    status: str
    title: str


@dataclass
class ReqRow:
    req_id: str
    status: str
    title: str


def load_tasks(task_file: Path) -> list[TaskRow]:
    result: list[TaskRow] = []
    for row in parse_table_rows(task_file):
        header_map = row.header_map
        if "任务id" not in header_map and "taskid" not in header_map:
            continue
        if "状态" not in header_map:
            continue
        task_id = get_cell(row.cells, header_map, ("任务ID", "taskId", "task_id"))
        title = get_cell(row.cells, header_map, ("任务标题", "任务名称", "标题"))
        status = get_cell(row.cells, header_map, ("状态",))
        req_id = get_cell(row.cells, header_map, ("需求ID", "reqId", "req_id")) or row.section_req_id
        if not task_id or not title or not status:
            continue
        result.append(TaskRow(task_id=task_id, req_id=req_id or "", status=normalize(status), title=title))
    return result


def load_requirements(req_file: Path) -> dict[str, ReqRow]:
    result: dict[str, ReqRow] = {}
    for row in parse_table_rows(req_file):
        header_map = row.header_map
        if "需求id" not in header_map and "reqid" not in header_map:
            continue
        if "状态" not in header_map:
            continue
        req_id = get_cell(row.cells, header_map, ("需求ID", "reqId", "req_id"))
        title = get_cell(row.cells, header_map, ("标题", "需求标题"))
        status = get_cell(row.cells, header_map, ("状态",))
        if not req_id or not title or not status:
            continue
        result[req_id] = ReqRow(req_id=req_id, title=title, status=normalize(status))
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description="Generic stage readiness check for markdown-based requirement/task boards")
    add_profile_arg(parser)
    parser.add_argument("--task-file", help="Task board markdown path")
    parser.add_argument("--req-file", help="Requirement pool markdown path")
    parser.add_argument("--current-req", required=True)
    parser.add_argument("--next-req", required=True)
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    task_path = Path(args.task_file).resolve() if args.task_file else project_paths.task_board
    req_path = Path(args.req_file).resolve() if args.req_file else project_paths.requirements_pool

    tasks = load_tasks(task_path)
    reqs = load_requirements(req_path)

    current_req = reqs.get(args.current_req)
    next_req = reqs.get(args.next_req)

    print_header(
        "阶段启动检查",
        {
            "task_file": str(task_path),
            "req_file": str(req_path),
            "parsed_tasks": str(len(tasks)),
            "parsed_requirements": str(len(reqs)),
        },
    )

    if args.dry_run:
        print(f"- dry_run_current_req_found: {current_req is not None}")
        print(f"- dry_run_next_req_found: {next_req is not None}")
        sample_tasks = [t for t in tasks if t.req_id == args.current_req][:5]
        for task in sample_tasks:
            print(f"- task_sample: {task.task_id} | {task.title} | {task.req_id} | {task.status}")
        return 0

    if current_req is None or next_req is None:
        print("[FAIL] 未找到当前需求或下一阶段需求")
        return 1

    current_tasks = [t for t in tasks if t.req_id == args.current_req]
    undone = [t for t in current_tasks if t.status != "done"]

    print(f"- 当前需求: {current_req.req_id} {current_req.title} ({current_req.status})")
    print(f"- 下一需求: {next_req.req_id} {next_req.title} ({next_req.status})")
    print(f"- 当前需求任务数: {len(current_tasks)}")
    print(f"- 未完成任务数: {len(undone)}")

    if current_req.status not in {"done", "closed"}:
        print("[BLOCK] 当前阶段需求未完成")
        return 1
    if undone:
        print("[BLOCK] 当前阶段仍有未完成任务")
        for task in undone:
            print(f"- {task.task_id} {task.title} ({task.status})")
        return 1
    if next_req.status not in {"planned", "doing"}:
        print(f"[BLOCK] 下一阶段需求状态异常: {next_req.status}")
        return 1

    print("[PASS] 当前阶段已满足进入下一阶段的基础条件")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
