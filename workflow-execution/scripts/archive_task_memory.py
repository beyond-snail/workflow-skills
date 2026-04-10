#!/usr/bin/env python3
from __future__ import annotations

import argparse
import shutil
from datetime import date
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from profile_paths import ProjectPaths


def update_index(index_file: Path, task_id: str, new_dir: str, updated_at: str, dry_run: bool) -> str:
    if not index_file.exists():
        return "skipped"
    lines = index_file.read_text(encoding="utf-8", errors="ignore").splitlines()
    changed = False
    for idx, line in enumerate(lines):
        if f"`{task_id}`" not in line:
            continue
        cells = [item.strip() for item in line.split("|")]
        if len(cells) >= 9:
            cells[5] = "archived"
            cells[7] = f"`{new_dir}`"
            cells[8] = updated_at
            lines[idx] = " | ".join(cells)
            changed = True
            break
        if "| done |" in line:
            line = line.replace("| done |", "| archived |", 1)
        elif "| blocked |" in line:
            line = line.replace("| blocked |", "| archived |", 1)
        elif "| doing |" in line:
            line = line.replace("| doing |", "| archived |", 1)
        line = line.replace("`" + task_id + "`", f"`{task_id}`", 1)
        if ".ai/memory/tasks/" in line:
            start = line.find("`.ai/memory/tasks/")
            end = line.find("`", start + 1)
            if start != -1 and end != -1:
                line = line[: start + 1] + new_dir + line[end:]
        lines[idx] = line
        changed = True
        break
    if changed and not dry_run:
        index_file.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
        return "updated"
    return "skipped" if not changed else "updated"


def main() -> int:
    parser = argparse.ArgumentParser(description="Archive a task memory directory and sync index")
    add_profile_arg(parser)
    parser.add_argument("--task-id", required=True)
    parser.add_argument("--task-dir", required=True, help="Task memory dir relative to workspace root")
    parser.add_argument("--updated-at", default=date.today().isoformat())
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    source_dir = (project_paths.workspace_root / args.task_dir).resolve()
    archive_root = project_paths.tasks_index.parent / "archived"
    target_dir = archive_root / source_dir.name

    print_header(
        "Archive Task Memory",
        {
            "task_id": args.task_id,
            "source": str(source_dir),
            "target": str(target_dir),
            "mode": "dry-run" if args.dry_run else "live",
        },
    )

    if not source_dir.exists():
        print("[BLOCK] task memory dir not found")
        return 1

    if source_dir.parent.name == "archived":
        print("- info: task already archived")
        return 0

    if target_dir.exists():
        print("[BLOCK] archive target already exists")
        return 1

    new_dir_rel = str(target_dir.relative_to(project_paths.workspace_root)).replace("\\", "/")
    if args.dry_run:
        print(f"- action: move `{source_dir}` -> `{target_dir}`")
        print(f"- action: sync index to archived with directory `{new_dir_rel}`")
        return 0

    archive_root.mkdir(parents=True, exist_ok=True)
    shutil.move(str(source_dir), str(target_dir))
    update_status = update_index(project_paths.tasks_index, args.task_id, new_dir_rel, args.updated_at, dry_run=False)
    print(f"- index_sync: {update_status}")
    print("[PASS] task memory archived")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
