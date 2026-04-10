#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from profile_paths import ProjectPaths

CODE_PATH_RE = re.compile(r"`([^`]+)`")


def resolve_task_dir_from_index_line(project_paths: ProjectPaths, line: str) -> Path | None:
    for match in CODE_PATH_RE.findall(line):
        candidate = (project_paths.workspace_root / match.rstrip("/")).resolve()
        if candidate.exists() and candidate.is_dir():
            return candidate
    return None


def is_archived_task_dir(path: Path) -> bool:
    return "archived" in path.parts


def append_unique(paths: list[Path], candidate: Path | None) -> None:
    if candidate and candidate not in paths:
        paths.append(candidate)


def candidate_task_indices(project_paths: ProjectPaths) -> list[Path]:
    candidates = [project_paths.tasks_index]
    legacy = [
        project_paths.workspace_root / "doc/tasks/index.md",
        project_paths.workspace_root / "docs/tasks/index.md",
        project_paths.workspace_root / "tasks/index.md",
    ]
    for path in legacy:
        resolved = path.resolve()
        if resolved not in [item.resolve() for item in candidates] and path.exists():
            candidates.append(path)
    return candidates


def candidate_knowledge_dirs(project_paths: ProjectPaths) -> list[Path]:
    candidates = [project_paths.knowledge_dir]
    legacy = [
        project_paths.workspace_root / "doc/knowledge",
        project_paths.workspace_root / "docs/knowledge",
        project_paths.workspace_root / "knowledge",
    ]
    for path in legacy:
        resolved = path.resolve()
        if resolved not in [item.resolve() for item in candidates] and path.exists():
            candidates.append(path)
    return candidates


def main() -> int:
    parser = argparse.ArgumentParser(description="Load task memory and knowledge context")
    add_profile_arg(parser)
    parser.add_argument("--req-id", default="")
    parser.add_argument("--task-id", default="")
    parser.add_argument("--keyword", action="append", default=[])
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    keywords = [k.strip() for k in args.keyword if k.strip()]

    active_tasks: list[Path] = []
    archived_tasks: list[Path] = []
    matched_knowledge: list[Path] = []

    for index_file in candidate_task_indices(project_paths):
        index_text = index_file.read_text(encoding="utf-8") if index_file.exists() else ""
        if args.task_id and args.task_id in index_text:
            for line in index_text.splitlines():
                if args.task_id in line:
                    task_dir = resolve_task_dir_from_index_line(project_paths, line)
                    if is_archived_task_dir(task_dir) if task_dir else False:
                        append_unique(archived_tasks, task_dir)
                    else:
                        append_unique(active_tasks, task_dir)

        if not active_tasks and args.req_id:
            for line in index_text.splitlines():
                if args.req_id in line:
                    task_dir = resolve_task_dir_from_index_line(project_paths, line)
                    if is_archived_task_dir(task_dir) if task_dir else False:
                        append_unique(archived_tasks, task_dir)
                    else:
                        append_unique(active_tasks, task_dir)
            for task_dir in sorted(index_file.parent.iterdir() if index_file.parent.exists() else []):
                if not task_dir.is_dir() or task_dir.name.startswith("_"):
                    continue
                readme = task_dir / "README.md"
                if not readme.exists():
                    continue
                if args.req_id not in readme.read_text(encoding="utf-8", errors="ignore"):
                    continue
                if is_archived_task_dir(task_dir):
                    append_unique(archived_tasks, task_dir)
                else:
                    append_unique(active_tasks, task_dir)

    matched_tasks = active_tasks or archived_tasks
    if keywords:
        for knowledge_dir in candidate_knowledge_dirs(project_paths):
            if not knowledge_dir.exists():
                continue
            for md in sorted(knowledge_dir.glob("*.md")):
                text = md.read_text(encoding="utf-8", errors="ignore")
                if any(keyword in md.name or keyword in text for keyword in keywords):
                    if md not in matched_knowledge:
                        matched_knowledge.append(md)

    print_header(
        "Memory Context",
        {
            "req_id": args.req_id or None,
            "task_id": args.task_id or None,
        },
    )
    print(f"- tasks: {', '.join(str(p.relative_to(project_paths.workspace_root)) for p in matched_tasks) or '(none)'}")
    if archived_tasks and not active_tasks:
        print("- task_match_mode: archived-fallback")
    elif active_tasks:
        print("- task_match_mode: active-priority")
    print(f"- knowledge: {', '.join(str(p.relative_to(project_paths.workspace_root)) for p in matched_knowledge) or '(none)'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
