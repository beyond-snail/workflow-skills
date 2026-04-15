#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import find_requirement_row, get_cell
from profile_paths import ProjectPaths


LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
BACKTICK_RE = re.compile(r"`([^`]+)`")

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

SQL_REQUIRED_KEYWORDS = {
    "01-DDL": True,
    "02-DDL-字段修正": False,
    "03-DDL-索引修正": False,
    "01-SQL-历史补全": False,
    "01-SQL-测试样本": True,
}

CODE_TOUCHPOINT_DOC_KEYWORDS = ("详细开发设计", "开发任务拆解", "流程图与实现对齐")
CODE_PATH_PREFIXES = ("server/", "admin/", "miniprogram/", "payment/", "scripts/")
NEW_PATH_HINTS = ("新增", "新建", "待新增", "新页面目录")

PLACEHOLDER_PATTERNS = (
    "待补充",
    "待执行",
    "待实现",
    "待确认",
    "待排期",
    "TODO",
    "子任务一",
    "子任务二",
    "待确认项一",
    "待确认项二",
    "按实际需求补充 SQL",
)

QUALITY_TEXT_MIN_CHARS = 800
QUALITY_PLACEHOLDER_RATIO_MAX = 0.02


def quality_metrics(text: str) -> tuple[int, int, float]:
    char_count = len(text)
    placeholder_count = sum(text.count(marker) for marker in PLACEHOLDER_PATTERNS)
    ratio = placeholder_count / char_count if char_count > 0 else 1.0
    return char_count, placeholder_count, ratio


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


def discover_sql_docs(discovered_design_docs: dict[str, Path]) -> dict[str, Path]:
    discovered: dict[str, Path] = {}
    bundle_dirs: set[Path] = set()
    for path in discovered_design_docs.values():
        if path.parent.name == "design":
            bundle_dirs.add(path.parent.parent)

    for bundle_dir in bundle_dirs:
        sql_dir = bundle_dir / "sql"
        if not sql_dir.exists():
            continue
        for file in sorted(sql_dir.rglob("*.sql")):
            for keyword in SQL_REQUIRED_KEYWORDS.keys():
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

    char_count, placeholder_count, ratio = quality_metrics(text)
    if char_count < QUALITY_TEXT_MIN_CHARS:
        return False, f"正文长度不足（{char_count} 字符），疑似未形成可执行设计"
    if placeholder_count >= 3 and ratio > QUALITY_PLACEHOLDER_RATIO_MAX:
        return False, f"占位词密度过高（{placeholder_count}/{char_count}≈{ratio:.2%}），疑似模板化正文"

    heading_only_lines = [line for line in lines if line.startswith("#") or line in {"---", "```", "```bash", "```sql", "```mermaid", "```text"}]
    if len(lines) > 0 and len(heading_only_lines) / len(lines) > 0.8:
        return False, "章节标题过多、正文过少，疑似空骨架"

    return True, ""


def has_meaningful_sql(path: Path, keyword: str) -> tuple[bool, str]:
    if not path.exists():
        return False, "文件不存在"
    text = path.read_text(encoding="utf-8").strip()
    if not text:
        return False, "文件为空"

    lines = [line.strip() for line in text.splitlines() if line.strip() and not line.strip().startswith("--")]
    if len(lines) < 3:
        return False, "有效 SQL 语句过少"

    if keyword == "01-DDL" and "CREATE TABLE" not in text:
        return False, "未检测到 CREATE TABLE，疑似仍为模板"
    if keyword == "01-SQL-测试样本" and "INSERT INTO" not in text:
        return False, "未检测到 INSERT INTO，疑似仍为模板"
    if keyword == "02-DDL-字段修正" and "ALTER TABLE" not in text:
        return False, "未检测到 ALTER TABLE，疑似仍为模板"
    if keyword == "03-DDL-索引修正" and "CREATE INDEX" not in text:
        return False, "未检测到 CREATE INDEX，疑似仍为模板"

    if "按实际需求补充 SQL" in text:
        return False, "包含模板占位语句，疑似未填充"

    return True, ""


def extract_code_paths_from_doc(path: Path) -> list[tuple[str, bool]]:
    text = path.read_text(encoding="utf-8")
    results: list[tuple[str, bool]] = []
    for line in text.splitlines():
        candidates = BACKTICK_RE.findall(line)
        if not candidates:
            continue
        line_hint_new = any(hint in line for hint in NEW_PATH_HINTS)
        for candidate in candidates:
            value = candidate.strip()
            if not value:
                continue
            if not value.endswith((".java", ".js", ".ts", ".tsx", ".vue", ".sql", ".md", ".json")) and not value.endswith("/"):
                continue
            if not any(value.startswith(prefix) for prefix in CODE_PATH_PREFIXES):
                continue
            results.append((value.rstrip("/"), line_hint_new))
    # de-dup
    uniq: list[tuple[str, bool]] = []
    seen: set[str] = set()
    for item, is_new in results:
        if item in seen:
            continue
        seen.add(item)
        uniq.append((item, is_new))
    return uniq


def check_code_touchpoints(docs: dict[str, Path], workspace_root: Path) -> tuple[list[str], list[str], list[str], list[str]]:
    passes: list[str] = []
    infos: list[str] = []
    warns: list[str] = []
    blocks: list[str] = []
    checked = 0

    states: dict[str, str] = {}
    # state: exists | planned_new_missing | missing_unmarked
    for keyword in CODE_TOUCHPOINT_DOC_KEYWORDS:
        doc = docs.get(keyword)
        if doc is None or not doc.exists():
            continue
        touchpoints = extract_code_paths_from_doc(doc)
        for rel_path, hinted_new in touchpoints:
            checked += 1
            abs_path = workspace_root / rel_path
            if abs_path.exists():
                states[rel_path] = "exists"
                continue
            prev = states.get(rel_path)
            if hinted_new:
                if prev is None:
                    states[rel_path] = "planned_new_missing"
            else:
                states[rel_path] = "missing_unmarked"

    for rel_path, state in sorted(states.items()):
        if state == "exists":
            passes.append(f"{rel_path}（已存在）")
        elif state == "planned_new_missing":
            infos.append(f"{rel_path}（标记为新增，当前不存在，符合预期）")
        elif state == "missing_unmarked":
            blocks.append(f"{rel_path}（未标记新增但文件不存在，疑似路径错误）")

    if checked == 0:
        warns.append("未在设计文档中识别到代码落点路径")
    return passes, infos, warns, blocks


def main() -> int:
    parser = argparse.ArgumentParser(description="Check whether a requirement is ready to hand off into workflow-execution")
    add_profile_arg(parser)
    parser.add_argument("--req-file", help="Requirement pool markdown path")
    parser.add_argument("--task-file", help="Task board markdown path")
    parser.add_argument("--req-id", required=True)
    parser.add_argument("--docs-root", help="Docs root path")
    parser.add_argument("--strict", action="store_true", help="Treat all warnings as blockers")
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
            "strict": "on" if args.strict else "off",
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
    discovered_sql_docs = discover_sql_docs(discovered_docs)
    quality_warnings: list[str] = []
    for keyword, required in REQUIRED_DESIGN_KEYWORDS.items():
        path = discovered_docs.get(keyword)
        if path is None:
            if required:
                print(f"[BLOCK] 缺少必需文档类型: {keyword}")
                blocked = True
            else:
                level = "BLOCK" if args.strict else "WARN"
                print(f"[{level}] 缺少推荐文档类型: {keyword}")
                if args.strict:
                    blocked = True
            continue

        display = str(path.relative_to(docs_root)) if path.is_absolute() and path.is_relative_to(docs_root) else str(path)
        print(f"[PASS] 文档存在: {display}")
        if keyword in CONTENT_REQUIRED_KEYWORDS:
            ok, reason = has_meaningful_content(path)
            if ok:
                print(f"[PASS] 文档内容已非空骨架: {display}")
                text = path.read_text(encoding="utf-8")
                char_count, placeholder_count, ratio = quality_metrics(text)
                print(f"- quality: chars={char_count}, placeholders={placeholder_count}, ratio={ratio:.2%}")
                if placeholder_count >= 6:
                    quality_warnings.append(f"{display} 占位词出现较多（{placeholder_count}次），建议人工复核语义质量")
            elif required:
                print(f"[BLOCK] 文档存在但内容未完成: {display} ({reason})")
                blocked = True
            else:
                level = "BLOCK" if args.strict else "WARN"
                print(f"[{level}] 文档存在但内容未完成: {display} ({reason})")
                if args.strict:
                    blocked = True

    for keyword, required in SQL_REQUIRED_KEYWORDS.items():
        path = discovered_sql_docs.get(keyword)
        if path is None:
            if required:
                print(f"[BLOCK] 缺少必需 SQL 文档类型: {keyword}")
                blocked = True
            else:
                level = "BLOCK" if args.strict else "WARN"
                print(f"[{level}] 缺少推荐 SQL 文档类型: {keyword}")
                if args.strict:
                    blocked = True
            continue

        display = str(path.relative_to(docs_root)) if path.is_absolute() and path.is_relative_to(docs_root) else str(path)
        print(f"[PASS] SQL 文档存在: {display}")
        ok, reason = has_meaningful_sql(path, keyword)
        if ok:
            print(f"[PASS] SQL 文档已非空模板: {display}")
        elif required:
            print(f"[BLOCK] SQL 文档存在但内容未完成: {display} ({reason})")
            blocked = True
        else:
            level = "BLOCK" if args.strict else "WARN"
            print(f"[{level}] SQL 文档存在但内容未完成: {display} ({reason})")
            if args.strict:
                blocked = True

    touch_pass, touch_info, touch_warn, touch_block = check_code_touchpoints(discovered_docs, project_paths.workspace_root)
    for item in touch_pass:
        print(f"[PASS] 代码落点校验: {item}")
    for item in touch_info:
        print(f"[INFO] 代码落点校验: {item}")
    for item in touch_warn:
        level = "BLOCK" if args.strict else "WARN"
        print(f"[{level}] 代码落点校验: {item}")
        if args.strict:
            blocked = True
    for item in touch_block:
        print(f"[BLOCK] 代码落点校验: {item}")
        blocked = True

    task_content = task_path.read_text(encoding="utf-8") if task_path.exists() else ""
    if args.req_id in task_content and "| `TASK-" in task_content:
        print("[PASS] 任务看板已存在关联任务")
    else:
        print("[BLOCK] 任务看板未找到关联任务")
        blocked = True

    for warn in quality_warnings:
        level = "BLOCK" if args.strict else "WARN"
        print(f"[{level}] {warn}")
        if args.strict:
            blocked = True

    if blocked:
        print("[BLOCK] 尚不满足切换到 workflow-execution 的前置条件")
        return 1

    print("[PASS] 已满足开发交接前置条件")
    print("- next_action: 默认停在人工审核门；审核通过后，再由用户显式触发 `workflow-execution` / `开干` 进入开发执行")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
