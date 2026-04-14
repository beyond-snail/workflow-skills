#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from datetime import date
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from profile_paths import ProjectPaths

from create_requirement_bundle import next_req_id, next_task_id
from legacy_context import load_legacy_scan, match_legacy_context, render_legacy_context_lines
from project_state import build_project_state, write_project_state


SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_REVIEW_COMMAND = "开干"
PRD_DIR_KEYWORDS = {"prd", "spec", "specs"}
PRD_FILE_KEYWORDS = ("prd", "需求", "spec")


@dataclass
class PlannedRound:
    docs_root: Path
    req_root: str
    requirements_pool: Path
    task_board: Path
    readme_rel: str | None
    prd_rel: str
    req_id: str
    task_id: str
    bundle_dir: Path
    bundle_rel: str


def detect_req_root(project_paths: ProjectPaths) -> str:
    try:
        return str(project_paths.requirements_dir.resolve().relative_to(project_paths.docs_root.resolve()))
    except ValueError:
        if project_paths.docs_root.name in {"doc", "docs"}:
            return "requirements"
        return project_paths.requirements_dir.name


def detect_readme_rel(project_paths: ProjectPaths, docs_root: Path) -> str | None:
    try:
        return str(project_paths.readme_index.relative_to(docs_root))
    except ValueError:
        return None


def detect_prd_rel(project_paths: ProjectPaths) -> str:
    prd_dir = project_paths.prd_directory
    if prd_dir.exists():
        md_files = sorted(prd_dir.glob("*.md"))
        if len(md_files) == 1:
            return str(md_files[0].relative_to(project_paths.workspace_root))
    return str((project_paths.prd_directory / "待补PRD文档.md").relative_to(project_paths.workspace_root))


def tokenize_theme(theme: str) -> list[str]:
    parts = re.split(r"[\s_\-/]+", theme)
    tokens = [part.strip().lower() for part in parts if part.strip()]
    if theme.strip():
        tokens.append(theme.strip().lower())
    return list(dict.fromkeys(tokens))


def prd_candidate_score(path: Path, workspace_root: Path, theme_tokens: list[str]) -> tuple[int, float]:
    score = 0
    lower_name = path.name.lower()
    lower_stem = path.stem.lower()
    parts = {part.lower() for part in path.parts}

    if any(keyword in lower_name for keyword in PRD_FILE_KEYWORDS):
        score += 30
    if any(keyword in parts for keyword in PRD_DIR_KEYWORDS):
        score += 15

    for token in theme_tokens:
        if token and token in lower_stem:
            score += 50
        elif token and token in lower_name:
            score += 20

    try:
        rel = path.relative_to(workspace_root).as_posix().lower()
        if rel.startswith("doc/prd") or rel.startswith("docs/prd"):
            score += 25
    except ValueError:
        pass

    try:
        text_head = path.read_text(encoding="utf-8", errors="ignore")[:400]
        lower_head = text_head.lower()
        for token in theme_tokens:
            if token and token in lower_head:
                score += 15
        if "产品需求文档" in text_head or "prd" in lower_head:
            score += 15
    except OSError:
        pass

    mtime = path.stat().st_mtime if path.exists() else 0.0
    return score, mtime


def discover_prd_rel(project_paths: ProjectPaths, theme: str) -> str:
    workspace_root = project_paths.workspace_root
    prd_dir = project_paths.prd_directory
    theme_tokens = tokenize_theme(theme)

    candidates: list[Path] = []
    seen: set[Path] = set()

    def add_candidate(path: Path) -> None:
        resolved = path.resolve()
        if resolved.suffix.lower() != ".md":
            return
        if resolved in seen:
            return
        seen.add(resolved)
        candidates.append(resolved)

    if prd_dir.exists():
        for path in prd_dir.rglob("*.md"):
            add_candidate(path)

    root_depth = len(workspace_root.parts)
    for current_root, dirnames, filenames in os.walk(workspace_root):
        current_path = Path(current_root)
        depth = len(current_path.parts) - root_depth
        if depth > 4:
            dirnames[:] = []
            continue
        lowered_parts = {part.lower() for part in current_path.parts}
        interested_dir = any(keyword in lowered_parts for keyword in PRD_DIR_KEYWORDS)
        for filename in filenames:
            lower_name = filename.lower()
            if not lower_name.endswith(".md"):
                continue
            if interested_dir or any(keyword in lower_name for keyword in PRD_FILE_KEYWORDS):
                add_candidate(current_path / filename)

    if not candidates:
        return detect_prd_rel(project_paths)

    if len(candidates) == 1:
        return str(candidates[0].relative_to(workspace_root))

    ranked = sorted(
        candidates,
        key=lambda path: prd_candidate_score(path, workspace_root, theme_tokens),
        reverse=True,
    )
    return str(ranked[0].relative_to(workspace_root))


def plan_round(args: argparse.Namespace, project_paths: ProjectPaths) -> PlannedRound:
    docs_root = Path(args.docs_root).resolve() if args.docs_root else project_paths.docs_root
    req_root = args.req_root or detect_req_root(project_paths)
    req_dir = docs_root / req_root
    req_file = req_dir / "需求池.md"
    task_file = req_dir / "任务看板.md"
    req_id = args.req_id or next_req_id(req_file, args.date)
    task_id = args.initial_task_id or next_task_id(task_file, args.date)
    bundle_rel = f"{req_root}/{args.date}-{args.theme}"
    return PlannedRound(
        docs_root=docs_root,
        req_root=req_root,
        requirements_pool=req_file,
        task_board=task_file,
        readme_rel=detect_readme_rel(project_paths, docs_root),
        prd_rel=args.prd_rel or discover_prd_rel(project_paths, args.theme),
        req_id=req_id,
        task_id=task_id,
        bundle_dir=docs_root / f"{args.date}-{args.theme}" if req_root == "." else docs_root / bundle_rel,
        bundle_rel=bundle_rel,
    )


def run_python(script_name: str, script_args: list[str], dry_run: bool) -> int:
    cmd = [sys.executable, str(SCRIPT_DIR / script_name), *script_args]
    print(f"$ {' '.join(cmd)}")
    if dry_run:
        return 0
    proc = subprocess.run(cmd)
    return proc.returncode


def requirement_self_check(project_paths: ProjectPaths, planned: PlannedRound, task_memory_dir_rel: Path | None) -> tuple[list[str], list[str]]:
    infos: list[str] = []
    warnings: list[str] = []
    project_state_path = project_paths.workspace_root / ".ai/runtime/project-state.json"
    if planned.requirements_pool.exists():
        infos.append(f"requirements_pool={planned.requirements_pool}")
    else:
        warnings.append(f"requirements_pool_missing={planned.requirements_pool}")
    if planned.task_board.exists():
        infos.append(f"task_board={planned.task_board}")
    else:
        warnings.append(f"task_board_missing={planned.task_board}")
    if planned.bundle_dir.exists():
        infos.append(f"bundle_dir={planned.bundle_dir}")
    else:
        warnings.append(f"bundle_dir_missing={planned.bundle_dir}")
    if task_memory_dir_rel:
        task_memory_dir = project_paths.workspace_root / task_memory_dir_rel
        if task_memory_dir.exists():
            infos.append(f"task_memory_dir={task_memory_dir}")
        else:
            warnings.append(f"task_memory_dir_missing={task_memory_dir}")
    if project_paths.tasks_index.exists():
        infos.append(f"tasks_index={project_paths.tasks_index}")
    else:
        warnings.append(f"tasks_index_missing={project_paths.tasks_index}")
    if project_state_path.exists():
        infos.append(f"project_state={project_state_path}")
    else:
        warnings.append(f"project_state_missing={project_state_path}")
    return infos, warnings


def main() -> int:
    parser = argparse.ArgumentParser(
        description="One-line requirement governance entrypoint: create bundle, sync governance files, and stop at manual review gate"
    )
    add_profile_arg(parser)
    parser.add_argument("--docs-root", help="Docs root path. Defaults to project-profile paths.docs_root")
    parser.add_argument("--req-root", help="Requirement root under docs-root. Defaults to profile requirements dir")
    parser.add_argument("--date", default=date.today().isoformat())
    parser.add_argument("--theme", required=True, help="Requirement theme or title")
    parser.add_argument("--summary", default="", help="Short request summary for output only")
    parser.add_argument("--prd-rel", help="PRD path relative to workspace root")
    parser.add_argument("--req-id", help="Optional fixed requirement id")
    parser.add_argument("--initial-task-id", help="Optional fixed initial task id")
    parser.add_argument("--initial-task-title", help="Optional initial task title")
    parser.add_argument("--initial-task-acceptance", default="技术设计文档与开发任务拆解初稿已建立，可进入细化评审")
    parser.add_argument("--layout", choices=("layered", "flat"), default="layered")
    parser.add_argument("--export-html", action="store_true", help="Also export 需求看板.html after governance sync")
    parser.add_argument("--skip-content-population", action="store_true", help="Skip PRD-driven正文填充 and only keep generated skeletons")
    parser.add_argument("--skip-handoff-check", action="store_true", help="Skip readiness check at the end")
    parser.add_argument("--skip-task-memory-init", action="store_true", help="Skip initializing .ai/memory/tasks memory directory")
    parser.add_argument("--task-memory-type", default="feature", help="Task memory type: feature|bugfix|continuation|ops")
    parser.add_argument("--task-memory-status", default="todo", help="Initial task memory status")
    parser.add_argument("--review-command", default=DEFAULT_REVIEW_COMMAND, help="Recommended explicit command after manual review")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    planned = plan_round(args, project_paths)
    legacy_scan = load_legacy_scan(project_paths.workspace_root)
    legacy_context = match_legacy_context(legacy_scan, args.theme, args.summary, planned.prd_rel)

    print_header(
        "Requirement Round",
        {
            "theme": args.theme,
            "summary": args.summary or None,
            "req_id": planned.req_id,
            "initial_task_id": planned.task_id,
            "bundle": planned.bundle_rel,
            "prd_rel": planned.prd_rel,
            "mode": "dry-run" if args.dry_run else "live",
        },
    )
    for line in render_legacy_context_lines(legacy_context):
        print(line)

    prd_file = (project_paths.workspace_root / planned.prd_rel).resolve()
    if not args.skip_content_population and not prd_file.exists():
        print(f"[BLOCK] PRD 文件不存在，无法自动填充正文: {prd_file}")
        print("- hint: 传入有效 --prd-rel，或显式加 --skip-content-population 仅生成骨架")
        return 1

    task_dir_name = f"{args.date}-{args.theme}"
    task_memory_dir_rel = project_paths.tasks_index.parent.relative_to(project_paths.workspace_root) / task_dir_name

    if args.dry_run:
        print("- action: create requirement governance bundle and sync 需求池/任务看板")
        if args.skip_task_memory_init:
            print("- action: skip task memory initialization")
        else:
            print(f"- action: initialize task memory under `{task_memory_dir_rel}` and sync task index")
        if args.skip_content_population:
            print("- action: skip PRD-driven content population, keep skeleton docs")
        else:
            print(f"- action: populate docs and task board from PRD `{planned.prd_rel}`")
        if args.export_html:
            print("- action: export 需求看板.html")
        if args.skip_handoff_check:
            print("- action: skip handoff readiness check")
        else:
            print("- action: run handoff readiness check and stop at manual review gate")
        infos, warnings = requirement_self_check(
            project_paths,
            planned,
            None if args.skip_task_memory_init else task_memory_dir_rel,
        )
        preview_state = build_project_state(
            project_paths.workspace_root,
            profile,
            requirements_pool=planned.requirements_pool,
            task_board=planned.task_board,
            stage="requirement",
            gate_status="待人工审核",
            health="待审核",
            risk="观察中" if warnings else "低",
            sync_source="requirement",
            sync_status="preview",
            current_req_id=planned.req_id,
            current_req_title=args.theme,
            current_task_id=planned.task_id,
            current_task_title=args.initial_task_title or args.theme,
            current_task_status=args.task_memory_status,
            summary=(args.summary or f"{planned.req_id} 需求已入池，等待人工审核") + f"；{legacy_context['summary']}",
            blockers=warnings,
            evidence_refs=legacy_context["evidence_refs"],
        )
        preview_path = write_project_state(project_paths.workspace_root, preview_state, dry_run=True)
        print(f"- action: update project-state preview at `{preview_path}`")
        for item in infos:
            print(f"- selfcheck-info: {item}")
        for item in warnings:
            print(f"- selfcheck-warning: {item}")
        print(f"- next_action: 人工审核通过后，显式执行 `{args.review_command}` 再进入 workflow-execution")
        return 0

    create_args = [
        "--docs-root",
        str(planned.docs_root),
        "--req-root",
        planned.req_root,
        "--date",
        args.date,
        "--theme",
        args.theme,
        "--layout",
        args.layout,
        "--prd-rel",
        planned.prd_rel,
        "--req-id",
        planned.req_id,
        "--initial-task-id",
        planned.task_id,
        "--initial-task-acceptance",
        args.initial_task_acceptance,
    ]
    if args.profile:
        create_args = ["--profile", args.profile, *create_args]
    if planned.readme_rel:
        create_args.extend(["--readme", planned.readme_rel])
    if args.initial_task_title:
        create_args.extend(["--initial-task-title", args.initial_task_title])

    if run_python("create_requirement_bundle.py", create_args, dry_run=False) != 0:
        return 1

    task_memory_title = args.initial_task_title or args.theme

    if not args.skip_task_memory_init:
        memory_args = [
            "--task-id",
            planned.task_id,
            "--title",
            task_memory_title,
            "--date",
            args.date,
            "--type",
            args.task_memory_type,
            "--status",
            args.task_memory_status,
            "--task-dir-name",
            task_dir_name,
        ]
        if planned.req_id:
            memory_args.extend(["--req-id", planned.req_id])
        if args.profile:
            memory_args = ["--profile", args.profile, *memory_args]
        if run_python("init_task_memory.py", memory_args, dry_run=False) != 0:
            return 1

        index_args = [
            "--task-id",
            planned.task_id,
            "--title",
            task_memory_title,
            "--type",
            args.task_memory_type,
            "--keywords",
            f"`{planned.req_id}` / {args.theme}",
            "--status",
            args.task_memory_status,
            "--latest",
            "已初始化任务记忆目录，待继续补充分析/验证/决策",
            "--directory",
            str(task_memory_dir_rel).replace("\\", "/"),
            "--updated-at",
            args.date,
        ]
        if args.profile:
            index_args = ["--profile", args.profile, *index_args]
        if run_python("sync_task_index.py", index_args, dry_run=False) != 0:
            return 1

    if not args.skip_content_population:
        populate_args = [
            "--req-file",
            str(planned.requirements_pool),
            "--task-file",
            str(planned.task_board),
            "--req-id",
            planned.req_id,
            "--initial-task-id",
            planned.task_id,
            "--theme",
            args.theme,
            "--date",
            args.date,
            "--bundle-dir",
            str(planned.bundle_dir),
            "--prd-file",
            str(prd_file),
        ]
        if args.profile:
            populate_args = ["--profile", args.profile, *populate_args]
        if run_python("populate_requirement_content.py", populate_args, dry_run=False) != 0:
            return 1

    if args.export_html:
        export_args = [
            "--requirements",
            str(planned.requirements_pool),
            "--tasks",
            str(planned.task_board),
        ]
        if run_python("markdown_to_html.py", export_args, dry_run=False) != 0:
            return 1

    if not args.skip_handoff_check:
        handoff_args = [
            "--req-file",
            str(planned.requirements_pool),
            "--task-file",
            str(planned.task_board),
            "--req-id",
            planned.req_id,
            "--docs-root",
            str(project_paths.workspace_root),
        ]
        if args.profile:
            handoff_args = ["--profile", args.profile, *handoff_args]
        if run_python("check_handoff_readiness.py", handoff_args, dry_run=False) != 0:
            return 1

    infos, warnings = requirement_self_check(
        project_paths,
        planned,
        None if args.skip_task_memory_init else task_memory_dir_rel,
    )
    state = build_project_state(
        project_paths.workspace_root,
        profile,
        requirements_pool=planned.requirements_pool,
        task_board=planned.task_board,
        stage="requirement",
        gate_status="待人工审核",
        health="待审核",
        risk="观察中" if warnings else "低",
        sync_source="requirement",
        sync_status="fresh",
        current_req_id=planned.req_id,
        current_req_title=args.theme,
        current_task_id=planned.task_id,
        current_task_title=task_memory_title,
        current_task_status=args.task_memory_status,
        summary=(args.summary or f"{planned.req_id} 需求已入池，等待人工审核") + f"；{legacy_context['summary']}",
        blockers=warnings,
        evidence_refs=legacy_context["evidence_refs"],
    )
    state_path = write_project_state(project_paths.workspace_root, state, dry_run=False)
    print(f"- project_state: {state_path}")
    for item in infos:
        print(f"- selfcheck-info: {item}")
    for item in warnings:
        print(f"- selfcheck-warning: {item}")

    print("- review_gate: manual-review-required")
    print(f"- recommended_command: {args.review_command}")
    if not args.skip_task_memory_init:
        print(f"- task_memory_dir: {task_memory_dir_rel}")
    print(f"- next_action: 人工审核通过后，显式执行 `{args.review_command}` 进入 workflow-execution")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
