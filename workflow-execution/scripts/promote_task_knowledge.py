#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from profile_paths import ProjectPaths


def slugify(value: str) -> str:
    value = value.strip()
    value = re.sub(r"[\\/:*?\"<>|]+", "-", value)
    value = re.sub(r"\s+", "-", value)
    value = re.sub(r"-{2,}", "-", value)
    return value.strip("-") or "knowledge"


def main() -> int:
    parser = argparse.ArgumentParser(description="Promote task conclusion into project knowledge skeleton")
    add_profile_arg(parser)
    parser.add_argument("--title", required=True)
    parser.add_argument("--summary", required=True)
    parser.add_argument("--source-task-dir", default="")
    parser.add_argument("--filename", default="")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    filename = args.filename or f"{slugify(args.title)}.md"
    target = project_paths.knowledge_dir / filename

    print_header("Promote Task Knowledge", {"file": str(target)})
    if args.dry_run:
        print(f"- summary: {args.summary}")
        return 0

    if target.exists():
        content = target.read_text(encoding="utf-8")
    else:
        content = f"# {args.title}\n\n## 1. 背景\n\n## 2. 结论\n\n## 3. 适用场景\n\n## 4. 来源\n\n"
    appendix = []
    appendix.append(f"- 结论：{args.summary}")
    if args.source_task_dir:
        appendix.append(f"- 来源任务：`{args.source_task_dir}`")
    if "## 4. 来源" in content:
        content = content.rstrip() + "\n" + "\n".join(appendix) + "\n"
    else:
        content = content.rstrip() + "\n\n## 4. 来源\n\n" + "\n".join(appendix) + "\n"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8")
    print("[PASS] knowledge promoted")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
