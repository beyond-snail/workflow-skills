#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import find_requirement_row, get_cell
from profile_paths import ProjectPaths


LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")

REQUIRED_DESIGN_KEYWORDS = {
    "技术设计": True,
    "详细开发设计": False,
    "开发任务拆解": True,
    "PRD追溯": True,
    "产品确认清单": False,
    "流程图与实现对齐": False,
}

CONTENT_REQUIRED_KEYWORDS = {
    "技术设计",
    "详细开发设计",
    "开发任务拆解",
    "PRD追溯",
    "产品确认清单",
    "流程图与实现对齐",
}

PLACEHOLDER_PATTERNS = (
    "待补充",
    "子任务一",
    "子任务二",
    "待确认项一",
    "待确认项二",
    "按实际需求补充 SQL",
)


def extract_links(cell: str) -> list[str]:
    return [match.group(2) for match in LINK_RE.finditer(cell)]


def discover_design_docs(design_links: list[str], docs_root: Path) -> dict[str, Path]:
    discovered: dict[str, Path] = {}
    design_dirs: set[Path] = set()

    for rel in design_links:
        resolved = docs_root / rel
        if resolved.parent.name == "design":
            design_dirs.add(resolved.parent)
        for keyword in REQUIRED_DESIGN_KEYWORDS.keys():
            if keyword in resolved.name and keyword not in discovered:
                discovered[keyword] = resolved

    for design_dir in design_dirs:
        if not design_dir.exists():
            continue
        for file in sorted(design_dir.glob("*.md")):
            for keyword in REQUIRED_DESIGN_KEYWORDS.keys():
                if keyword in file.name and keyword not in discovered:
                    discovered[keyword] = file

    return discovered


def has_meaningful_content(path: Path) -> tuple[bool, str]:
    if not path.exists():
        return False, "文件不存在"

    text = path.read_text(encoding="utf-8").strip()
    if not text:
        return False, "文件为空"

    lines = [line.strip() for line in text.splitlines() if line.strip()]
    if len(lines) < 8:
        return False, "有效内容过少，疑似空骨架"

    if "PRD追溯" in path.name:
        if "| PRD章节 | PRD需求描述 | 设计/代码/测试对应位置 | 完成状态 | 备注 |" in text and "| `" in text:
            return True, ""

    placeholder_hits = sum(1 for marker in PLACEHOLDER_PATTERNS if marker in text)
    if placeholder_hits >= 2:
        return False, "包含多个占位词，疑似未填充正文"

    heading_only_lines = [line for line in lines if line.startswith("#") or line in {"---", "```", "```bash", "```sql", "```mermaid", "```text"}]
    if len(lines) > 0 and len(heading_only_lines) / len(lines) > 0.8:
        return False, "章节标题过多、正文过少，疑似空骨架"

    return True, ""


def main() -> int:
    parser = argparse.ArgumentParser(description="Check whether a requirement is ready to hand off into workflow-execution")
    add_profile_arg(parser)
    parser.add_argument("--req-file", help="Requirement pool markdown path")
    parser.add_argument("--task-file", help="Task board markdown path")
    parser.add_argument("--req-id", required=True)
    parser.add_argument("--docs-root", help="Docs root path")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    req_path = Path(args.req_file).resolve() if args.req_file else project_paths.requirements_pool
    task_path = Path(args.task_file).resolve() if args.task_file else project_paths.task_board
    docs_root = Path(args.docs_root).resolve() if args.docs_root else project_paths.workspace_root

    print_header(
        "Handoff Readiness",
        {
            "req_file": str(req_path),
            "task_file": str(task_path),
            "req_id": args.req_id,
            "mode": "dry-run" if args.dry_run else None,
        },
    )

    row = find_requirement_row(req_path, args.req_id)
    blocked = False

    if row is None:
        print("[BLOCK] 需求池中未找到目标需求")
        return 1

    title = get_cell(row.cells, row.header_map, ("标题", "需求标题")) or ""
    status = (get_cell(row.cells, row.header_map, ("状态",)) or "").lower()
    design_cell = get_cell(row.cells, row.header_map, ("设计文档",)) or ""
    design_links = extract_links(design_cell)

    print(f"- title: {title}")
    print(f"- status: {status}")

    if status not in {"planned", "doing", "done"}:
        print(f"[BLOCK] 需求状态不允许交接: {status}")
        blocked = True
    else:
        print("[PASS] 需求状态检查")

    discovered_docs = discover_design_docs(design_links, docs_root)
    for keyword, required in REQUIRED_DESIGN_KEYWORDS.items():
        path = discovered_docs.get(keyword)
        if path is None:
            if required:
                print(f"[BLOCK] 缺少必需文档类型: {keyword}")
                blocked = True
            else:
                print(f"[WARN] 缺少推荐文档类型: {keyword}")
            continue

        display = str(path.relative_to(docs_root)) if path.is_absolute() and path.is_relative_to(docs_root) else str(path)
        print(f"[PASS] 文档存在: {display}")
        if keyword in CONTENT_REQUIRED_KEYWORDS:
            ok, reason = has_meaningful_content(path)
            if ok:
                print(f"[PASS] 文档内容已非空骨架: {display}")
            elif required:
                print(f"[BLOCK] 文档存在但内容未完成: {display} ({reason})")
                blocked = True
            else:
                print(f"[WARN] 文档存在但内容未完成: {display} ({reason})")

    task_content = task_path.read_text(encoding="utf-8") if task_path.exists() else ""
    if args.req_id in task_content and "| `TASK-" in task_content:
        print("[PASS] 任务看板已存在关联任务")
    else:
        print("[BLOCK] 任务看板未找到关联任务")
        blocked = True

    if blocked:
        print("[BLOCK] 尚不满足切换到 workflow-execution 的前置条件")
        return 1

    print("[PASS] 已满足开发交接前置条件")
    print("- next_action: 默认停在人工审核门；审核通过后，再由用户显式触发 `workflow-execution` / `开干` 进入开发执行")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
