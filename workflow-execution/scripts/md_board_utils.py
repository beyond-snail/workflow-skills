#!/usr/bin/env python3
"""Shared markdown board parsing utilities for workflow-execution.

Localized copy to keep the skill self-contained without relying on _shared/.
"""
from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path


REQ_HEADING_RE = re.compile(r"^#{2,3}\s+(REQ-[A-Za-z0-9\-]+)\b")


@dataclass
class TableRow:
    section_req_id: str | None
    header_map: dict[str, int]
    line_index: int
    raw_cells: list[str]
    cells: list[str]


def clean_cell(text: str) -> str:
    return re.sub(r"^`+|`+$", "", text.strip())


def normalize(text: str) -> str:
    return clean_cell(text).lower()


def split_md_table(line: str) -> list[str]:
    return [clean_cell(c) for c in line.strip().strip("|").split("|")]


def split_md_table_raw(line: str) -> list[str]:
    return [c.strip() for c in line.strip().strip("|").split("|")]


def is_separator_row(cells: list[str]) -> bool:
    return bool(cells) and set("".join(cells)) <= {"-", ":"}


def parse_table_rows(md_path: Path) -> list[TableRow]:
    lines = md_path.read_text(encoding="utf-8").splitlines()
    rows: list[TableRow] = []
    current_req_id: str | None = None
    idx = 0
    while idx < len(lines):
        stripped = lines[idx].strip()
        heading_match = REQ_HEADING_RE.match(stripped)
        if heading_match:
            current_req_id = heading_match.group(1)
            idx += 1
            continue
        if not stripped.startswith("|") or idx + 1 >= len(lines):
            idx += 1
            continue

        header_cells = split_md_table(stripped)
        sep_cells = split_md_table(lines[idx + 1].strip())
        if not is_separator_row(sep_cells):
            idx += 1
            continue

        header_map = {normalize(cell): pos for pos, cell in enumerate(header_cells)}
        idx += 2
        while idx < len(lines):
            row_line = lines[idx].strip()
            if not row_line.startswith("|"):
                break
            raw_cells = split_md_table_raw(row_line)
            cells = [clean_cell(c) for c in raw_cells]
            if is_separator_row(cells):
                idx += 1
                continue
            rows.append(
                TableRow(
                    section_req_id=current_req_id,
                    header_map=header_map,
                    line_index=idx,
                    raw_cells=raw_cells,
                    cells=cells,
                )
            )
            idx += 1
    return rows


def get_cell(cells: list[str], header_map: dict[str, int], aliases: tuple[str, ...]) -> str | None:
    for alias in aliases:
        i = header_map.get(normalize(alias))
        if i is not None and i < len(cells):
            return clean_cell(cells[i])
    return None


def format_md_row(raw_cells: list[str]) -> str:
    return "| " + " | ".join(raw_cells) + " |"


def preserve_cell_style(previous_raw: str | None, new_value: str) -> str:
    if previous_raw:
        raw = previous_raw.strip()
        if raw.startswith("`") and raw.endswith("`"):
            return f"`{new_value}`"
    return new_value


def update_timestamp(lines: list[str], today: str) -> None:
    for i, line in enumerate(lines):
        if line.startswith("更新时间："):
            lines[i] = f"更新时间：{today}"
            return


def find_requirement_row(req_file: Path, req_id: str) -> TableRow | None:
    for row in parse_table_rows(req_file):
        req_value = get_cell(row.cells, row.header_map, ("需求ID", "reqId", "req_id"))
        if req_value == req_id:
            return row
    return None


def find_task_row(task_file: Path, task_id: str) -> TableRow | None:
    for row in parse_table_rows(task_file):
        task_value = get_cell(row.cells, row.header_map, ("任务ID", "taskId", "task_id"))
        if task_value == task_id:
            return row
    return None


def find_section_heading(lines: list[str], heading: str) -> int | None:
    for i, line in enumerate(lines):
        if line.strip() == heading:
            return i
    return None


def find_insert_after_heading(lines: list[str], heading: str) -> int | None:
    heading_idx = find_section_heading(lines, heading)
    if heading_idx is None:
        return None
    idx = heading_idx + 1
    while idx < len(lines) and lines[idx].strip() == "":
        idx += 1
    return idx


def ensure_heading(lines: list[str], heading: str) -> int:
    idx = find_section_heading(lines, heading)
    if idx is not None:
        return idx
    if lines and lines[-1].strip():
        lines.append("")
    lines.extend([heading, ""])
    return len(lines) - 2
