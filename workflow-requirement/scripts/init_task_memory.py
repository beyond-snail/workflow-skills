#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import shutil
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from profile_paths import ProjectPaths


def slugify(value: str) -> str:
    value = value.strip()
    value = re.sub(r"[\\/:*?\"<>|]+", "-", value)
    value = re.sub(r"\s+", "-", value)
    value = re.sub(r"-{2,}", "-", value)
    return value.strip("-") or "task-memory"


def write_if_missing(target: Path, template: Path | None, replacements: dict[str, str], dry_run: bool) -> bool:
    if target.exists():
        return False
    if template and template.exists():
        content = template.read_text(encoding="utf-8")
    else:
        content = ""
    for key, value in replacements.items():
        content = content.replace(key, value)
    if not dry_run:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description="Initialize task memory directory from task templates")
    add_profile_arg(parser)
    parser.add_argument("--task-id", required=True)
    parser.add_argument("--title", required=True)
    parser.add_argument("--date", required=True)
    parser.add_argument("--type", default="feature")
    parser.add_argument("--req-id", default="")
    parser.add_argument("--status", default="todo")
    parser.add_argument("--task-dir-name", help="Explicit task directory name under task memory root")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    task_dir_name = args.task_dir_name or f"{args.date}-{slugify(args.title)}"
    task_dir = project_paths.tasks_index.parent / task_dir_name
    template_dir = project_paths.tasks_template_dir
    replacements = {
        "任务ID：": f"任务ID：`{args.task_id}`",
        "任务名称：": f"任务名称：{args.title}",
        "类型：`feature / bugfix / continuation / ops`": f"类型：`{args.type}`",
        "关联需求：": f"关联需求：`{args.req_id}`" if args.req_id else "关联需求：",
        "当前状态：`todo / doing / blocked / done / archived`": f"当前状态：`{args.status}`",
        "最后更新：": f"最后更新：`{args.date}`",
    }

    print_header(
        "Init Task Memory",
        {
            "task_id": args.task_id,
            "title": args.title,
            "task_dir": str(task_dir),
            "mode": "dry-run" if args.dry_run else "live",
        },
    )

    files = {
        "README.md": template_dir / "README.md",
        "inbox.md": template_dir / "inbox.md",
        "issues.md": template_dir / "issues.md",
        "decisions.md": template_dir / "decisions.md",
        "verify.md": template_dir / "verify.md",
    }
    created: list[str] = []
    if not args.dry_run:
        task_dir.mkdir(parents=True, exist_ok=True)
    for name, template in files.items():
        if write_if_missing(task_dir / name, template, replacements, args.dry_run):
            created.append(name)

    print(f"- created: {', '.join(created) if created else '(none)'}")
    print(f"- task_dir_rel: {task_dir.relative_to(project_paths.workspace_root)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
