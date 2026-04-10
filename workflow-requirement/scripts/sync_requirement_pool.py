#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import date
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args
from md_board_utils import (
    find_requirement_row,
    find_section_heading,
    format_md_row,
    get_cell,
    preserve_cell_style,
    update_timestamp,
)


DEFAULT_REQUIREMENT_POOL = """# 需求池

更新时间：{sync_date}

## 状态定义

- `draft`：需求刚收集，范围未冻结。
- `planned`：需求已冻结，允许进入设计与排期。
- `doing`：需求已启动开发。
- `done`：需求已完成开发并通过验收。
- `closed`：需求终止或并入其他需求处理。

## 需求列表

| 需求ID | 标题 | 状态 | 来源 | 设计文档 | 任务拆解 |
| --- | --- | --- | --- | --- | --- |
"""


def join_design_links(values: list[str]) -> str:
    return "<br>".join(values)


def make_requirement_row(req_id: str, title: str, status: str, source: str, design_docs: list[str], task_board: str) -> str:
    raw_cells = [f"`{req_id}`", title, f"`{status}`", source, join_design_links(design_docs), task_board]
    return format_md_row(raw_cells)


def ensure_requirement_pool(req_path: Path, sync_date: str) -> None:
    if req_path.exists():
        return
    req_path.parent.mkdir(parents=True, exist_ok=True)
    req_path.write_text(DEFAULT_REQUIREMENT_POOL.format(sync_date=sync_date) + "\n", encoding="utf-8")


def sync_requirement_pool_entry(
    req_path: Path,
    req_id: str,
    title: str,
    status: str,
    source: str,
    design_docs: list[str],
    task_board: str,
    sync_date: str,
    dry_run: bool,
) -> int:
    ensure_requirement_pool(req_path, sync_date)
    row = find_requirement_row(req_path, req_id)
    lines = req_path.read_text(encoding="utf-8").splitlines()

    print("# Requirement Pool Sync")
    print(f"- req_id: {req_id}")
    print(f"- title: {title}")
    print(f"- status: {status}")
    print(f"- req_file: {req_path}")

    if row is not None:
        req_idx = row.header_map.get("需求id")
        title_idx = row.header_map.get("标题")
        status_idx = row.header_map.get("状态")
        source_idx = row.header_map.get("来源")
        design_idx = row.header_map.get("设计文档")
        task_idx = row.header_map.get("任务拆解")
        new_raw_cells = list(row.raw_cells)
        if req_idx is not None:
            new_raw_cells[req_idx] = preserve_cell_style(row.raw_cells[req_idx], req_id)
        if title_idx is not None:
            new_raw_cells[title_idx] = title
        if status_idx is not None:
            new_raw_cells[status_idx] = preserve_cell_style(row.raw_cells[status_idx], status)
        if source_idx is not None:
            new_raw_cells[source_idx] = source
        if design_idx is not None:
            new_raw_cells[design_idx] = join_design_links(design_docs)
        if task_idx is not None:
            new_raw_cells[task_idx] = task_board
        new_line = format_md_row(new_raw_cells)
        print("- action: update-existing-row")
        print(f"- new_line: {new_line}")
        if dry_run:
            return 0
        lines[row.line_index] = new_line
        update_timestamp(lines, sync_date)
        req_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print("[PASS] requirement row updated")
        return 0

    print("- action: append-new-row")
    new_line = make_requirement_row(req_id, title, status, source, design_docs, task_board)
    print(f"- new_line: {new_line}")

    if dry_run:
        return 0

    insert_idx = None
    for idx, line in enumerate(lines):
        if line.strip().startswith("| 需求ID |"):
            insert_idx = idx + 2
            while insert_idx < len(lines) and lines[insert_idx].strip().startswith("|"):
                insert_idx += 1
            break

    if insert_idx is None:
        print("Error: requirement table header not found")
        return 1

    lines.insert(insert_idx, new_line)

    req_heading = f"## {req_id}"
    if find_section_heading(lines, req_heading) is None:
        section_block = [
            "",
            req_heading,
            "",
            "### 背景",
            "",
            "待补充。",
            "",
            "### 目标",
            "",
            "待补充。",
            "",
            "### 关联文档",
            "",
        ]
        for doc in design_docs:
            section_block.append(f"1. {doc}")
        if not design_docs:
            section_block.append("1. 待补充")
        lines.extend(section_block)

    update_timestamp(lines, sync_date)
    req_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("[PASS] requirement row appended")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Sync one requirement entry into markdown requirement pool")
    add_profile_arg(parser)
    parser.add_argument("--req-file", help="Path to 需求池.md")
    parser.add_argument("--req-id", required=True)
    parser.add_argument("--title", required=True)
    parser.add_argument("--status", default="planned")
    parser.add_argument("--source", default="-")
    parser.add_argument("--design-doc", action="append", default=[], help="Repeatable design doc link")
    parser.add_argument("--task-board", default="[doc/requirements/任务看板.md](doc/requirements/任务看板.md)")
    parser.add_argument("--sync-date", default=date.today().isoformat())
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    req_default = profile.get("paths", {}).get("requirements_pool", "doc/requirements/需求池.md")
    req_path = Path(args.req_file).resolve() if args.req_file else Path.cwd() / req_default
    return sync_requirement_pool_entry(
        req_path=req_path,
        req_id=args.req_id,
        title=args.title,
        status=args.status,
        source=args.source,
        design_docs=args.design_doc or [],
        task_board=args.task_board,
        sync_date=args.sync_date,
        dry_run=args.dry_run,
    )


if __name__ == "__main__":
    raise SystemExit(main())
