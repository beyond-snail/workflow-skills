#!/usr/bin/env python3
"""Write a compact recovery brief for Codex context compaction."""
from __future__ import annotations

import argparse
import subprocess
from datetime import datetime
from pathlib import Path

from cli_common import add_dry_run_arg, print_header


MAX_TEXT = 360
MAX_ITEMS = 12


def compact_text(value: str, max_chars: int = MAX_TEXT) -> str:
    text = " ".join((value or "").split())
    if not text:
        return ""
    if len(text) <= max_chars:
        return text
    return text[:max_chars].rstrip() + f"... <truncated {len(text) - max_chars} chars>"


def unique(items: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for item in items:
        text = item.strip()
        if not text or text in seen:
            continue
        seen.add(text)
        result.append(text)
    return result


def display_path(path_text: str, root: Path) -> str:
    text = path_text.strip()
    if not text:
        return ""
    path = Path(text)
    if not path.is_absolute():
        return text
    try:
        return str(path.resolve().relative_to(root))
    except ValueError:
        return str(path)


def git_status_paths(root: Path) -> list[str]:
    try:
        proc = subprocess.run(
            ["git", "status", "--short"],
            cwd=str(root),
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=5,
            check=False,
        )
    except Exception:
        return []
    if proc.returncode != 0:
        return []
    paths: list[str] = []
    for line in proc.stdout.splitlines():
        if len(line) < 4:
            continue
        payload = line[3:].strip()
        if " -> " in payload:
            payload = payload.split(" -> ", 1)[1].strip()
        if payload:
            paths.append(payload)
    return paths


def bullet_lines(items: list[str], empty: str = "- 暂无") -> list[str]:
    cleaned = unique([compact_text(item) for item in items])[:MAX_ITEMS]
    if not cleaned:
        return [empty]
    return [f"- {item}" for item in cleaned]


def build_brief(args: argparse.Namespace, root: Path) -> str:
    updated_at = datetime.now().astimezone().isoformat(timespec="seconds")
    changed_files = unique(
        [display_path(item, root) for item in args.changed_file]
        + [display_path(item, root) for item in git_status_paths(root)]
    )[:MAX_ITEMS]
    evidence = unique([display_path(item, root) for item in args.evidence])[:MAX_ITEMS]

    lines = [
        "# Context Brief",
        "",
        "## Snapshot",
        f"- updated_at: {updated_at}",
        f"- workspace: {root}",
        f"- mode: {args.mode or 'unknown'}",
        f"- task_status: {args.task_status or 'unknown'}",
        "",
        "## Current Focus",
        f"- requirement: `{args.req_id or 'N/A'}` {compact_text(args.req_title)}",
        f"- task: `{args.task_id or 'N/A'}` {compact_text(args.task_title)}",
        f"- summary: {compact_text(args.summary) or '待补充'}",
        "",
        "## Key Files",
        *bullet_lines(changed_files),
        "",
        "## Evidence",
        *bullet_lines(evidence),
        "",
        "## Verification",
        *bullet_lines(args.verification),
        "",
        "## Blockers",
        *bullet_lines(args.blocker),
        "",
        "## Next Steps",
        *bullet_lines(args.next_step),
        "",
        "## Recovery Rules",
        "- 新窗口或压缩恢复后，先读 `AGENTS.md`、本文件、`.ai/memory/tasks/index.md` 和 `.ai/runtime/project-state.json`。",
        "- 默认只读取 `AGENTS.md` 与本文件；除非需要事实结论、代码改动、SQL/接口判断或验证提交，否则不得扩展读取历史文件、完整 transcript、完整 checkpoint 或大文档。",
        "- 完整 transcript、compact checkpoint、conversation 冷归档只在追溯证据时按关键词局部读取。",
        "- 如果本文件与任务看板冲突，以任务看板和最新代码状态为准，并在收口时覆盖更新本文件。",
        "",
    ]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Update .ai/memory/context-brief.md for compact recovery")
    parser.add_argument("--workspace-root", default=".", help="Project workspace root")
    parser.add_argument("--req-id", default="")
    parser.add_argument("--req-title", default="")
    parser.add_argument("--task-id", default="")
    parser.add_argument("--task-title", default="")
    parser.add_argument("--task-status", default="")
    parser.add_argument("--mode", default="")
    parser.add_argument("--summary", default="")
    parser.add_argument("--changed-file", action="append", default=[])
    parser.add_argument("--evidence", action="append", default=[])
    parser.add_argument("--verification", action="append", default=[])
    parser.add_argument("--blocker", action="append", default=[])
    parser.add_argument("--next-step", action="append", default=[])
    add_dry_run_arg(parser)
    args = parser.parse_args()

    root = Path(args.workspace_root).expanduser().resolve()
    target = root / ".ai/memory/context-brief.md"
    content = build_brief(args, root)

    print_header(
        "Context Brief Update",
        {
            "target": str(target),
            "mode": "dry-run" if args.dry_run else "live",
            "bytes": str(len(content.encode("utf-8"))),
        },
    )
    if args.dry_run:
        print(content)
        return 0

    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8")
    print(f"- context_brief: {target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
