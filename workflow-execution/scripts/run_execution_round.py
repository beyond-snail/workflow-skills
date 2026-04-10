#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import find_requirement_row, get_cell
from profile_paths import ProjectPaths
from project_state import build_project_state, write_project_state

from select_next_task import TaskRecord, load_tasks


SCRIPT_DIR = Path(__file__).resolve().parent
LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
DECISION_SIGNAL_RE = re.compile(r"(根因|决定|最终发现|改成|结论)")
CONTINUATION_SIGNAL_RE = re.compile(r"(继续|收口|遗留|上次|延续|接着|未完成)")
BUGFIX_SIGNAL_RE = re.compile(r"(bug|缺陷|测试问题|报错|异常|失败|修复|修一下|问题)", re.IGNORECASE)


@dataclass
class RequirementContext:
    req_id: str
    title: str
    root_dir: Path | None
    prd_trace: Path | None
    acceptance_files: list[Path]
    test_result_files: list[Path]


def extract_links(cell: str) -> list[str]:
    return [match.group(2) for match in LINK_RE.finditer(cell)]


def select_task(task_file: Path, task_id: str | None, req_id: str | None) -> TaskRecord | None:
    tasks = load_tasks(task_file)
    if task_id:
        return next((task for task in tasks if task.task_id == task_id), None)

    if req_id:
        tasks = [task for task in tasks if task.req_id == req_id]

    current = next((task for task in tasks if task.status == "doing"), None)
    return current or next((task for task in tasks if task.status == "todo"), None)


def resolve_requirement_context(project_paths: ProjectPaths, req_id: str | None) -> RequirementContext | None:
    if not req_id or not project_paths.requirements_pool.exists():
        return None

    row = find_requirement_row(project_paths.requirements_pool, req_id)
    if row is None:
        return None

    title = get_cell(row.cells, row.header_map, ("标题", "需求标题")) or ""
    design_cell = get_cell(row.cells, row.header_map, ("设计文档",)) or ""
    links = extract_links(design_cell)

    root_dir: Path | None = None
    prd_trace: Path | None = None
    for rel in links:
        resolved = (project_paths.workspace_root / rel).resolve()
        if "PRD追溯" in resolved.name:
            prd_trace = resolved
        if resolved.parent.name == "design":
            root_dir = resolved.parent.parent

    acceptance_files: list[Path] = []
    test_result_files: list[Path] = []
    if root_dir:
        testing_dir = root_dir / "testing"
        if testing_dir.exists():
            acceptance_files = sorted(testing_dir.glob("*联调验收记录*.md"))
            test_result_files = sorted(testing_dir.glob("*测试结果*.md"))

    return RequirementContext(
        req_id=req_id,
        title=title,
        root_dir=root_dir,
        prd_trace=prd_trace,
        acceptance_files=acceptance_files,
        test_result_files=test_result_files,
    )


def run_shell(cmd: str, cwd: Path, dry_run: bool = False) -> tuple[int, str]:
    print(f"$ {cmd}")
    if dry_run:
        return 0, "dry-run skipped"
    proc = subprocess.run(
        cmd,
        cwd=str(cwd),
        shell=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return proc.returncode, proc.stdout.strip()


def run_exec(cmd: list[str], cwd: Path | None = None, dry_run: bool = False) -> tuple[int, str]:
    print(f"$ {' '.join(cmd)}")
    if dry_run:
        return 0, "dry-run skipped"
    proc = subprocess.run(
        cmd,
        cwd=str(cwd) if cwd else None,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return proc.returncode, proc.stdout.strip()


def run_helper(script_name: str, args: list[str], dry_run: bool) -> tuple[int, str]:
    cmd = [sys.executable, str(SCRIPT_DIR / script_name), *args]
    return run_exec(cmd, dry_run=dry_run)


def find_git_root(start: Path) -> Path:
    current = start.resolve()
    for path in [current, *current.parents]:
        if (path / ".git").exists():
            return path
    return current


def parse_git_status_output(output: str) -> list[str]:
    files: list[str] = []
    for line in output.splitlines():
        stripped = line.rstrip()
        if not stripped:
            continue
        if len(stripped) > 3 and stripped[2] == " ":
            path = stripped[3:]
        else:
            path = stripped.split(maxsplit=1)[-1]
        if " -> " in path:
            path = path.split(" -> ", 1)[1]
        files.append(path)
    return files


def build_commit_message(task: TaskRecord, commit_type: str) -> str:
    if task.req_id:
        return f"{commit_type}(task): {task.task_id} {task.title}\n\nRefs: {task.req_id}"
    return f"{commit_type}(task): {task.task_id} {task.title}"


def find_task_memory_dir(project_paths: ProjectPaths, task_id: str) -> Path | None:
    for index_file in candidate_task_indices(project_paths):
        if not index_file.exists():
            continue
        index_text = index_file.read_text(encoding="utf-8", errors="ignore")
        for line in index_text.splitlines():
            if f"`{task_id}`" in line:
                for rel in extract_backtick_paths(line):
                    candidate = (project_paths.workspace_root / rel.rstrip("/")).resolve()
                    if candidate.exists() and candidate.is_dir():
                        return candidate
    return None


def find_task_memory_dir_by_req(project_paths: ProjectPaths, req_id: str | None) -> Path | None:
    if not req_id:
        return None
    for index_file in candidate_task_indices(project_paths):
        if not index_file.exists():
            continue
        index_text = index_file.read_text(encoding="utf-8", errors="ignore")
        for line in index_text.splitlines():
            if req_id in line:
                for rel in extract_backtick_paths(line):
                    candidate = (project_paths.workspace_root / rel.rstrip("/")).resolve()
                    if candidate.exists() and candidate.is_dir():
                        return candidate
        for task_dir in sorted(index_file.parent.iterdir() if index_file.parent.exists() else []):
            if not task_dir.is_dir() or task_dir.name.startswith("_"):
                continue
            readme = task_dir / "README.md"
            if readme.exists() and req_id in readme.read_text(encoding="utf-8", errors="ignore"):
                return task_dir
    return None


def extract_backtick_paths(line: str) -> list[str]:
    return [match for match in re.findall(r"`([^`]+)`", line) if "/" in match]


def candidate_task_indices(project_paths: ProjectPaths) -> list[Path]:
    candidates = [project_paths.tasks_index]
    legacy = [
        project_paths.workspace_root / "doc/tasks/index.md",
        project_paths.workspace_root / "docs/tasks/index.md",
        project_paths.workspace_root / "tasks/index.md",
    ]
    for path in legacy:
        if path.exists() and path.resolve() not in [item.resolve() for item in candidates]:
            candidates.append(path)
    return candidates


def execution_self_check(
    project_paths: ProjectPaths,
    task_file: Path,
    build_commands: list[str],
    test_commands: list[str],
    req_id: str | None,
    task_memory_dir: Path | None,
) -> tuple[list[str], list[str]]:
    infos: list[str] = []
    warnings: list[str] = []
    project_state_path = project_paths.workspace_root / ".ai/runtime/project-state.json"
    if task_file.exists():
        infos.append(f"task_board={task_file}")
    else:
        warnings.append(f"task_board_missing={task_file}")
    if project_paths.requirements_pool.exists():
        infos.append(f"requirements_pool={project_paths.requirements_pool}")
    else:
        warnings.append(f"requirements_pool_missing={project_paths.requirements_pool}")
    if project_paths.tasks_index.exists():
        infos.append(f"tasks_index={project_paths.tasks_index}")
    else:
        warnings.append(f"tasks_index_missing={project_paths.tasks_index}")
    if project_paths.knowledge_dir.exists():
        infos.append(f"knowledge_dir={project_paths.knowledge_dir}")
    else:
        warnings.append(f"knowledge_dir_missing={project_paths.knowledge_dir}")
    if build_commands:
        infos.append(f"build_commands={'; '.join(build_commands)}")
    else:
        warnings.append("build_commands=empty")
    if test_commands:
        infos.append(f"test_commands={'; '.join(test_commands)}")
    else:
        warnings.append("test_commands=empty")
    if req_id:
        infos.append(f"req_id={req_id}")
    if task_memory_dir:
        infos.append(f"task_memory_dir={task_memory_dir}")
    else:
        warnings.append("task_memory_dir=not_found")
    if project_state_path.exists():
        infos.append(f"project_state={project_state_path}")
    else:
        warnings.append(f"project_state_missing={project_state_path}")
    return infos, warnings


def suggestion_flags(
    mode: str,
    summary: str,
    blockers: list[str],
    knowledge_hits: int = 0,
) -> tuple[bool, bool, bool]:
    suggest_issue = mode in {"bugfix", "continuation"} or len(blockers) >= 2
    suggest_decision = bool(summary and DECISION_SIGNAL_RE.search(summary))
    suggest_knowledge = knowledge_hits >= 2 or ("高频" in summary or "复用" in summary)
    return suggest_issue, suggest_decision, suggest_knowledge


def infer_execution_mode(
    requested_mode: str,
    summary: str,
    task: TaskRecord,
    blockers: list[str],
    task_memory_dir: Path | None,
) -> str:
    if requested_mode != "auto":
        return requested_mode

    text = " ".join(filter(None, [summary, task.title, task.task_id]))
    if CONTINUATION_SIGNAL_RE.search(text):
        return "continuation"
    if task.status in {"doing", "blocked"} and task_memory_dir is not None:
        return "continuation"
    if blockers:
        return "bugfix"
    if BUGFIX_SIGNAL_RE.search(text):
        return "bugfix"
    return "feature"


def should_suggest_archive(task_status: str, mode: str, blockers: list[str], task_memory_dir: Path | None) -> bool:
    if not task_memory_dir or "archived" in task_memory_dir.parts:
        return False
    if blockers:
        return False
    return task_status == "done" or mode == "continuation"


def print_suggestion(level: str, message: str) -> None:
    print(f"- {level}: {message}")


def append_task_evidence(
    files: list[Path],
    task: TaskRecord,
    status: str,
    summary: str,
    verification_lines: list[str],
    artifacts: list[str],
    blockers: list[str],
    next_steps: list[str],
    dry_run: bool,
) -> int:
    for file in files:
        helper_args = [
            "--file",
            str(file),
            "--task-id",
            task.task_id,
            "--status",
            status,
        ]
        if summary:
            helper_args.extend(["--summary", summary])
        for item in verification_lines:
            helper_args.extend(["--verification", item])
        for item in artifacts:
            helper_args.extend(["--artifact", item])
        for item in blockers:
            helper_args.extend(["--blocker", item])
        for item in next_steps:
            helper_args.extend(["--next-step", item])
        if dry_run:
            helper_args.append("--dry-run")
        code, output = run_helper("record_task_evidence.py", helper_args, dry_run=dry_run)
        if output:
            print(output)
        if code != 0:
            return code
    return 0


def append_test_results(
    files: list[Path],
    status: str,
    title: str,
    summary: str,
    commands: list[str],
    results: list[str],
    conclusions: list[str],
    blockers: list[str],
    alternatives: list[str],
    dry_run: bool,
) -> int:
    if not files:
        return 0

    requirement_skill_scripts = SCRIPT_DIR.parent.parent / "workflow-requirement" / "scripts"
    helper = requirement_skill_scripts / "record_test_result.py"
    for file in files:
        helper_args = [
            "--file",
            str(file),
            "--title",
            title,
            "--status",
            status,
        ]
        if summary:
            helper_args.extend(["--summary", summary])
        for item in commands:
            helper_args.extend(["--command", item])
        for item in results:
            helper_args.extend(["--result", item])
        for item in conclusions:
            helper_args.extend(["--conclusion", item])
        for item in blockers:
            helper_args.extend(["--blocker", item])
        for item in alternatives:
            helper_args.extend(["--alternative", item])
        cmd = [sys.executable, str(helper), *helper_args]
        if dry_run:
            cmd.append("--dry-run")
        code, output = run_exec(cmd, dry_run=dry_run)
        if output:
            print(output)
        if code != 0:
            return code
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Explicit execution entrypoint: requires manual review approval, then auto-runs validation, evidence, commit/push, and release gate"
    )
    add_profile_arg(parser)
    parser.add_argument("--task-file", help="Task board path")
    parser.add_argument("--req-id", help="Requirement id filter")
    parser.add_argument("--task-id", help="Explicit task id; otherwise select doing or earliest todo")
    parser.add_argument("--confirm-start", action="store_true", help="Confirm manual review is complete and execution is explicitly authorized")
    parser.add_argument("--summary", default="", help="Execution summary for evidence and reporting")
    parser.add_argument("--record-file", action="append", default=[], help="Markdown file to append task evidence into, repeatable")
    parser.add_argument("--test-result-file", action="append", default=[], help="Markdown file to append test result into, repeatable")
    parser.add_argument("--build-cmd", action="append", default=[], help="Build/compile command, repeatable")
    parser.add_argument("--test-cmd", action="append", default=[], help="Test command, repeatable")
    parser.add_argument("--verification", action="append", default=[], help="Extra verification line for task evidence")
    parser.add_argument("--memory-keyword", action="append", default=[], help="Keyword for memory context lookup, repeatable")
    parser.add_argument("--mode", choices=("auto", "feature", "bugfix", "continuation"), default="auto", help="Execution mode for memory handling; defaults to auto inference")
    parser.add_argument("--issue-note", default="", help="Issue summary to record into task memory when needed")
    parser.add_argument("--decision-note", default="", help="Decision summary to record into task memory when needed")
    parser.add_argument("--promote-knowledge", action="store_true", help="Promote summary into project knowledge after execution")
    parser.add_argument("--archive-task-memory", action="store_true", help="Archive task memory after final status done")
    parser.add_argument("--artifact", action="append", default=[], help="Artifact path for task evidence")
    parser.add_argument("--blocker", action="append", default=[], help="Known blocker item, repeatable")
    parser.add_argument("--alternative", action="append", default=[], help="Alternative verification item, repeatable")
    parser.add_argument("--next-step", action="append", default=[], help="Next-step item for task evidence")
    parser.add_argument("--doc-file", action="append", default=[], help="Document file required by release gate, repeatable")
    parser.add_argument("--gate-evidence-file", action="append", default=[], help="Evidence file for blocked tests in release gate, repeatable")
    parser.add_argument("--allow-test-blocked", action="store_true", help="Allow test failure when blockers and alternative validation are documented")
    parser.add_argument("--stage-file", action="append", default=[], help="Specific file to stage for git commit, repeatable")
    parser.add_argument("--commit-type", default="feat", help="Commit type: feat|fix|docs|refactor|test|chore")
    parser.add_argument("--no-commit", action="store_true", help="Skip git add/commit")
    parser.add_argument("--no-push", action="store_true", help="Skip git push")
    parser.add_argument("--no-release-gate", action="store_true", help="Skip release gate")
    parser.add_argument("--require-clean-worktree", action="store_true", help="Pass --require-clean-worktree into release gate")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    git_root = find_git_root(project_paths.workspace_root)
    task_file = Path(args.task_file).resolve() if args.task_file else project_paths.task_board

    if not task_file.exists():
        print(f"Error: task file not found: {task_file}")
        return 1

    selected = select_task(task_file, args.task_id, args.req_id)
    if selected is None:
        print("Error: no executable task found in task board")
        return 1

    req_id = args.req_id or selected.req_id or None
    req_ctx = resolve_requirement_context(project_paths, req_id)
    if req_id and not selected.req_id:
        selected.req_id = req_id

    record_files = [Path(p).resolve() for p in args.record_file]
    if not record_files and req_ctx:
        record_files = req_ctx.acceptance_files[:1]

    test_result_files = [Path(p).resolve() for p in args.test_result_file]
    if not test_result_files and req_ctx:
        test_result_files = req_ctx.test_result_files[:1]

    build_commands = [cmd for cmd in args.build_cmd if cmd.strip()]
    if not build_commands and profile.get("build", {}).get("compile"):
        build_commands.append(profile["build"]["compile"])

    test_commands = [cmd for cmd in args.test_cmd if cmd.strip()]
    if not test_commands and profile.get("build", {}).get("test"):
        test_commands.append(profile["build"]["test"])

    gate_doc_files = [Path(p).resolve() for p in args.doc_file]
    if not gate_doc_files:
        gate_doc_files.append(task_file)
        if req_ctx and req_ctx.prd_trace:
            gate_doc_files.append(req_ctx.prd_trace)

    gate_evidence_files = [Path(p).resolve() for p in args.gate_evidence_file]
    if not gate_evidence_files:
        gate_evidence_files = test_result_files[:]

    if not args.confirm_start:
        print("[BLOCK] 缺少显式开工确认，请在人工审核通过后追加 --confirm-start")
        return 1

    pre_task_memory_dir = find_task_memory_dir(project_paths, selected.task_id) or find_task_memory_dir_by_req(project_paths, req_id)
    resolved_mode = infer_execution_mode(args.mode, args.summary or "", selected, args.blocker, pre_task_memory_dir)

    print_header(
        "Execution Round",
        {
            "task_id": selected.task_id,
            "task_title": selected.title,
            "task_status": selected.status,
            "req_id": req_id,
            "req_title": req_ctx.title if req_ctx else None,
            "exec_mode": resolved_mode,
            "mode_source": "auto" if args.mode == "auto" else "explicit",
            "mode": "dry-run" if args.dry_run else "live",
        },
    )

    if args.dry_run:
        memory_args = []
        if args.profile:
            memory_args.extend(["--profile", args.profile])
        if req_id:
            memory_args.extend(["--req-id", req_id])
        memory_args.extend(["--task-id", selected.task_id])
        for keyword in args.memory_keyword or [selected.title]:
            memory_args.extend(["--keyword", keyword])
        code, output = run_helper("load_memory_context.py", memory_args, dry_run=True)
        if output:
            print(output)
        knowledge_count = 0
        for line in output.splitlines() if output else []:
            if line.startswith("- knowledge:"):
                payload = line.split(":", 1)[1].strip()
                if payload and payload != "(none)":
                    knowledge_count = len([item for item in payload.split(", ") if item])
        task_memory_dir = pre_task_memory_dir
        print(f"- resolved_mode: {resolved_mode}")
        suggest_issue, suggest_decision, suggest_knowledge = suggestion_flags(resolved_mode, args.summary or "", args.blocker, knowledge_count)
        suggest_archive = should_suggest_archive(selected.status, resolved_mode, args.blocker, task_memory_dir)
        if task_memory_dir:
            print(f"- task_memory_dir: {task_memory_dir}")
            print(f"- planned_verify_file: {task_memory_dir / 'verify.md'}")
            if resolved_mode in {'bugfix', 'continuation'}:
                print(f"- planned_inbox_file: {task_memory_dir / 'inbox.md'}")
            if args.issue_note:
                print(f"- planned_issue_file: {task_memory_dir / 'issues.md'}")
                print(f"- planned_issue_summary: {args.issue_note}")
            elif suggest_issue:
                print_suggestion("should", "planned_issue: continuation/bugfix 场景建议补 issue")
            if args.decision_note:
                print(f"- planned_decision_file: {task_memory_dir / 'decisions.md'}")
                print(f"- planned_decision_summary: {args.decision_note}")
            elif suggest_decision:
                print_suggestion("should", "planned_decision: summary 命中决策信号，建议补 decision")
            if args.promote_knowledge:
                print(f"- planned_knowledge_dir: {project_paths.knowledge_dir}")
                print(f"- planned_knowledge_title: {selected.title}")
            elif suggest_knowledge:
                print_suggestion("optional", "planned_knowledge: 命中复用/多知识信号，建议提升到 knowledge")
            if args.archive_task_memory:
                print(f"- planned_archive_dir: {task_memory_dir.parent / 'archived' / task_memory_dir.name}")
            elif suggest_archive:
                print_suggestion("should", "planned_archive: 当前任务已收口或属于 continuation，建议补 `--archive-task-memory`")
        print(f"- record_files: {', '.join(str(p) for p in record_files) or '(none)'}")
        print(f"- test_result_files: {', '.join(str(p) for p in test_result_files) or '(none)'}")
        print(f"- build_commands: {', '.join(build_commands) or '(none)'}")
        print(f"- test_commands: {', '.join(test_commands) or '(none)'}")
        print(f"- gate_doc_files: {', '.join(str(p) for p in gate_doc_files) or '(none)'}")
        print(f"- stage_files: {', '.join(args.stage_file) or '(none)'}")
        print(f"- commit_enabled: {'no' if args.no_commit else 'yes'}")
        print(f"- push_enabled: {'no' if args.no_push else 'yes'}")
        print(f"- release_gate_enabled: {'no' if args.no_release_gate else 'yes'}")
        infos, warnings = execution_self_check(project_paths, task_file, build_commands, test_commands, req_id, task_memory_dir)
        preview_state = build_project_state(
            project_paths.workspace_root,
            profile,
            requirements_pool=project_paths.requirements_pool,
            task_board=task_file,
            stage="execution",
            gate_status="预览",
            health="预览",
            risk="观察中" if warnings or args.blocker else "低",
            sync_source="execution",
            sync_status="preview",
            current_req_id=req_id or selected.req_id or "",
            current_req_title=req_ctx.title if req_ctx else "",
            current_task_id=selected.task_id,
            current_task_title=selected.title,
            current_task_status=selected.status,
            current_mode=resolved_mode,
            summary=args.summary or f"{selected.task_id} 执行预览",
            blockers=list(args.blocker) or warnings,
            evidence_refs=[str(p) for p in (*record_files, *test_result_files)],
        )
        preview_path = write_project_state(project_paths.workspace_root, preview_state, dry_run=True)
        print(f"- action: update project-state preview at `{preview_path}`")
        for item in infos:
            print(f"- selfcheck-info: {item}")
        for item in warnings:
            print(f"- selfcheck-warning: {item}")
        return 0

    memory_args = []
    if args.profile:
        memory_args.extend(["--profile", args.profile])
    if req_id:
        memory_args.extend(["--req-id", req_id])
    memory_args.extend(["--task-id", selected.task_id])
    for keyword in args.memory_keyword or [selected.title]:
        memory_args.extend(["--keyword", keyword])
    code, output = run_helper("load_memory_context.py", memory_args, dry_run=False)
    if output:
        print(output)
    if code != 0:
        return code
    knowledge_count = 0
    for line in output.splitlines() if output else []:
        if line.startswith("- knowledge:"):
            payload = line.split(":", 1)[1].strip()
            if payload and payload != "(none)":
                knowledge_count = len([item for item in payload.split(", ") if item])

    task_memory_dir = pre_task_memory_dir
    infos, warnings = execution_self_check(project_paths, task_file, build_commands, test_commands, req_id, task_memory_dir)
    for item in infos:
        print(f"- selfcheck-info: {item}")
    for item in warnings:
        print(f"- selfcheck-warning: {item}")
    print(f"- resolved_mode: {resolved_mode}")
    suggest_issue, suggest_decision, suggest_knowledge = suggestion_flags(resolved_mode, args.summary or "", args.blocker, knowledge_count)
    suggest_archive = should_suggest_archive(selected.status, resolved_mode, args.blocker, task_memory_dir)
    if any((suggest_issue, suggest_decision, suggest_knowledge, suggest_archive)):
        print("- suggestion_summary: must=0 should="
              f"{int(suggest_issue) + int(suggest_decision) + int(suggest_archive)} optional={int(suggest_knowledge)}")
    if suggest_issue and not args.issue_note:
        print_suggestion("should", "建议补 `--issue-note`，当前回合命中 bugfix/continuation 或重复阻塞信号")
    if suggest_decision and not args.decision_note:
        print_suggestion("should", "建议补 `--decision-note`，summary 命中根因/决策信号")
    if suggest_knowledge and not args.promote_knowledge:
        print_suggestion("optional", "建议补 `--promote-knowledge`，当前结论具备复用信号")
    if suggest_archive and not args.archive_task_memory:
        print_suggestion("should", "建议补 `--archive-task-memory`，当前任务记忆已具备归档条件")

    task_status = selected.status
    if task_status == "todo":
        code, output = run_helper(
            "update_task_status.py",
            [
                "--task-file",
                str(task_file),
                "--task-id",
                selected.task_id,
                "--status",
                "doing",
                "--expected-current",
                "todo",
            ],
            dry_run=False,
        )
        if output:
            print(output)
        if code != 0:
            return code
        task_status = "doing"

    verification_lines = list(args.verification)
    result_lines: list[str] = []
    blockers = list(args.blocker)
    next_steps = list(args.next_step)
    alternatives = list(args.alternative)
    task_blocked = False
    fatal_failure = False

    for cmd in build_commands:
        code, output = run_shell(cmd, git_root)
        verification_lines.append(f"build `{cmd}` -> {'PASS' if code == 0 else 'FAIL'}")
        result_lines.append(f"[build] {cmd}: {output or ('PASS' if code == 0 else 'FAIL')}")
        if code != 0:
            blockers.append(f"构建失败：{cmd}")
            fatal_failure = True
            break

    if not fatal_failure:
        for cmd in test_commands:
            code, output = run_shell(cmd, git_root)
            is_blocked = code != 0 and args.allow_test_blocked and (blockers or alternatives or gate_evidence_files)
            if code == 0:
                verification_lines.append(f"test `{cmd}` -> PASS")
                result_lines.append(f"[test] {cmd}: {output or 'PASS'}")
                continue

            if is_blocked:
                verification_lines.append(f"test `{cmd}` -> BLOCKED")
                result_lines.append(f"[test] {cmd}: {output or 'BLOCKED'}")
                blockers.append(f"测试阻塞：{cmd}")
                task_blocked = True
                continue

            verification_lines.append(f"test `{cmd}` -> FAIL")
            result_lines.append(f"[test] {cmd}: {output or 'FAIL'}")
            blockers.append(f"测试失败：{cmd}")
            fatal_failure = True
            break

    test_status = "blocked" if task_blocked else ("fail" if fatal_failure else "pass")
    test_title = f"{selected.task_id} 自动执行"
    test_summary = args.summary or f"{selected.task_id} 自动验证结果"
    conclusions = ["自动执行已完成" if not fatal_failure else "自动执行存在失败项"]
    if task_blocked:
        conclusions = ["自动执行存在测试阻塞，已保留阻塞说明与替代验证"]

    if append_test_results(
        test_result_files,
        test_status,
        test_title,
        test_summary,
        build_commands + test_commands,
        result_lines,
        conclusions,
        blockers,
        alternatives,
        dry_run=False,
    ) != 0:
        return 1

    commit_message = build_commit_message(selected, args.commit_type)
    commit_failed = False
    if not fatal_failure and not args.no_commit:
        code, output = run_shell("git status --short", git_root)
        if output:
            print(output)
        if code != 0:
            fatal_failure = True
            commit_failed = True
            blockers.append("git status --short 执行失败")
        else:
            stage_files = args.stage_file[:] if args.stage_file else parse_git_status_output(output)
            if not stage_files:
                print("[BLOCK] 当前没有可提交变更，自动提交已跳过")
                fatal_failure = True
                commit_failed = True
                blockers.append("未检测到可提交变更")
            else:
                add_cmd = ["git", "add", "--", *stage_files]
                code, output = run_exec(add_cmd, cwd=git_root)
                if output:
                    print(output)
                if code != 0:
                    fatal_failure = True
                    commit_failed = True
                    blockers.append("git add 失败")
                else:
                    code, output = run_exec(
                        ["git", "commit", "-m", commit_message],
                        cwd=git_root,
                    )
                    if output:
                        print(output)
                    if code != 0:
                        fatal_failure = True
                        commit_failed = True
                        blockers.append("git commit 失败")
                    elif not args.no_push:
                        code, output = run_exec(["git", "push"], cwd=git_root)
                        if output:
                            print(output)
                        if code != 0:
                            fatal_failure = True
                            commit_failed = True
                            blockers.append("git push 失败")

    gate_blocked = False
    gate_output = ""
    if not fatal_failure and not args.no_release_gate and req_id:
        gate_args = [
            "--project-root",
            str(git_root),
            "--req-file",
            str(project_paths.requirements_pool),
            "--req-id",
            req_id,
        ]
        if args.profile:
            gate_args = ["--profile", args.profile, *gate_args]
        for path in gate_doc_files:
            gate_args.extend(["--doc-file", str(path)])
        if args.require_clean_worktree:
            gate_args.append("--require-clean-worktree")
        if args.allow_test_blocked:
            gate_args.append("--allow-test-blocked")
            for path in gate_evidence_files:
                gate_args.extend(["--evidence-file", str(path)])
        code, gate_output = run_helper("run_release_gate.py", gate_args, dry_run=False)
        if gate_output:
            print(gate_output)
        gate_blocked = code != 0

    final_task_status = "blocked" if (fatal_failure or task_blocked or commit_failed) else "done"
    if gate_blocked:
        next_steps.append("发布闸门未通过，需继续收口后再部署")
    elif not args.no_release_gate and req_id:
        verification_lines.append("release gate -> PASS")

    if append_task_evidence(
        record_files,
        selected,
        final_task_status,
        args.summary or f"{selected.task_id} 自动收口",
        verification_lines,
        args.artifact,
        blockers,
        next_steps,
        dry_run=False,
    ) != 0:
        return 1

    if task_memory_dir:
        verify_file = task_memory_dir / "verify.md"
        inbox_file = task_memory_dir / "inbox.md"
        issues_file = task_memory_dir / "issues.md"
        decisions_file = task_memory_dir / "decisions.md"
        for cmd in build_commands:
            code, output = run_helper(
                "record_task_verify.py",
                [
                    "--file",
                    str(verify_file),
                    "--action",
                    cmd,
                    "--result",
                    "PASS" if all("FAIL" not in item for item in verification_lines if cmd in item) else "FAIL",
                    "--coverage",
                    "自动执行构建校验",
                    "--risk",
                    "; ".join(blockers),
                ],
                dry_run=False,
            )
            if output:
                print(output)
            if code != 0:
                return code
        for cmd in test_commands:
            matched = next((item for item in verification_lines if cmd in item), "")
            result = "PASS"
            if "BLOCKED" in matched:
                result = "BLOCKED"
            elif "FAIL" in matched:
                result = "FAIL"
            code, output = run_helper(
                "record_task_verify.py",
                [
                    "--file",
                    str(verify_file),
                    "--action",
                    cmd,
                    "--result",
                    result,
                    "--coverage",
                    "自动执行测试校验",
                    "--risk",
                    "; ".join(blockers),
                ],
                dry_run=False,
            )
            if output:
                print(output)
            if code != 0:
                return code
        if resolved_mode in {"bugfix", "continuation"} and inbox_file.exists():
            code, output = run_helper(
                "append_memory_inbox.py",
                [
                    "--file",
                    str(inbox_file),
                    "--input",
                    args.summary or f"{selected.task_id} 自动执行",
                    "--judgement",
                    "执行回合自动收件",
                    "--action",
                    "build/test/release gate",
                    "--result",
                    final_task_status,
                    "--promote",
                    "任务记忆" if (blockers or task_blocked or fatal_failure) else "否",
                ],
                dry_run=False,
            )
            if output:
                print(output)
            if code != 0:
                return code
        if args.issue_note and issues_file.exists():
            code, output = run_helper(
                "record_task_issue.py",
                [
                    "--file",
                    str(issues_file),
                    "--phenomenon",
                    args.issue_note,
                    "--trigger",
                    args.summary or selected.title,
                    "--root-cause",
                    "待补充" if final_task_status == "done" and not blockers else "; ".join(blockers) or "待补充",
                    "--resolution",
                    "自动执行回合已留痕",
                    "--verification",
                    "; ".join(verification_lines),
                    "--status",
                    "fixed" if final_task_status == "done" else "open",
                    "--note",
                    resolved_mode,
                ],
                dry_run=False,
            )
            if output:
                print(output)
            if code != 0:
                return code
        if args.decision_note and decisions_file.exists():
            code, output = run_helper(
                "record_task_decision.py",
                [
                    "--file",
                    str(decisions_file),
                    "--decision",
                    args.decision_note,
                    "--reason",
                    args.summary or selected.title,
                    "--rejected",
                    "待补充",
                    "--risk",
                    "; ".join(blockers),
                    "--evidence",
                    "; ".join(verification_lines),
                ],
                dry_run=False,
            )
            if output:
                print(output)
            if code != 0:
                return code
        if args.promote_knowledge:
            code, output = run_helper(
                "promote_task_knowledge.py",
                [
                    *(["--profile", args.profile] if args.profile else []),
                    "--title",
                    selected.title,
                    "--summary",
                    args.summary or selected.title,
                    "--source-task-dir",
                    str(task_memory_dir.relative_to(project_paths.workspace_root)),
                ],
                dry_run=False,
            )
            if output:
                print(output)
            if code != 0:
                return code
        if args.archive_task_memory and final_task_status == "done":
            code, output = run_helper(
                "archive_task_memory.py",
                [
                    *(["--profile", args.profile] if args.profile else []),
                    "--task-id",
                    selected.task_id,
                    "--task-dir",
                    str(task_memory_dir.relative_to(project_paths.workspace_root)),
                ],
                dry_run=False,
            )
            if output:
                print(output)
            if code != 0:
                return code

    if task_status != final_task_status:
        code, output = run_helper(
            "update_task_status.py",
            [
                "--task-file",
                str(task_file),
                "--task-id",
                selected.task_id,
                "--status",
                final_task_status,
            ],
            dry_run=False,
        )
        if output:
            print(output)
        if code != 0:
            return code

    state = build_project_state(
        project_paths.workspace_root,
        profile,
        requirements_pool=project_paths.requirements_pool,
        task_board=task_file,
        stage="execution",
        gate_status="已完成" if final_task_status == "done" else ("阻塞中" if final_task_status == "blocked" else "执行中"),
        health="异常" if blockers or task_blocked or fatal_failure or commit_failed else "正常",
        risk="高" if blockers or task_blocked or fatal_failure or commit_failed else "低",
        sync_source="execution",
        sync_status="fresh",
        current_req_id=req_id or selected.req_id or "",
        current_req_title=req_ctx.title if req_ctx else "",
        current_task_id=selected.task_id,
        current_task_title=selected.title,
        current_task_status=final_task_status,
        current_mode=resolved_mode,
        summary=args.summary or f"{selected.task_id} 执行回合已{final_task_status}",
        blockers=blockers,
        evidence_refs=[str(p) for p in (*record_files, *test_result_files, *gate_doc_files)],
    )
    state_path = write_project_state(project_paths.workspace_root, state, dry_run=False)
    print(f"- project_state: {state_path}")

    print(f"- final_task_status: {final_task_status}")
    if final_task_status == "done" and task_memory_dir and not args.archive_task_memory and "archived" not in task_memory_dir.parts:
        print_suggestion("optional", "当前任务已完成，若短期不再续做，建议下一轮执行时补 `--archive-task-memory`")
    print(f"- commit_enabled: {'no' if args.no_commit else 'yes'}")
    print(f"- release_gate: {'BLOCK' if gate_blocked else ('SKIP' if args.no_release_gate or not req_id else 'PASS')}")
    if blockers:
        print(f"- blockers: {' | '.join(blockers)}")
    return 0 if not fatal_failure else 1


if __name__ == "__main__":
    raise SystemExit(main())
