#!/usr/bin/env python3
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
BOOTSTRAP_DIR = SCRIPT_DIR
ROOT_DIR = BOOTSTRAP_DIR.parent.parent
REQUIREMENT_DIR = ROOT_DIR / "workflow-requirement" / "scripts"
EXECUTION_DIR = ROOT_DIR / "workflow-execution" / "scripts"


COMMAND_ALIASES = {
    "bootstrap": "init",
    "health": "doctor",
    "requirement": "req",
    "execution": "exec",
    "archive": "arc",
}


def run(cmd: list[str], label: str) -> int:
    print(f"# workflow {label}", flush=True)
    proc = subprocess.run(cmd)
    return proc.returncode


def main() -> int:
    parser = argparse.ArgumentParser(description="Unified workflow command entry")
    subparsers = parser.add_subparsers(dest="command", required=True)

    p_bootstrap = subparsers.add_parser("bootstrap", aliases=["init"], help="Run workflow bootstrap init")
    p_bootstrap.add_argument("--workspace-root", default=".")
    p_bootstrap.add_argument("--host", action="append", choices=("codex", "claude"), default=[])
    p_bootstrap.add_argument("--dry-run", action="store_true")

    p_health = subparsers.add_parser("health", aliases=["doctor"], help="Run workflow health check")
    p_health.add_argument("--workspace-root", default=".")

    p_requirement = subparsers.add_parser("requirement", aliases=["req"], help="Run requirement round")
    p_requirement.add_argument("--workspace-root", default=".")
    p_requirement.add_argument("--theme", required=True)
    p_requirement.add_argument("--summary", default="")
    p_requirement.add_argument("--dry-run", action="store_true")
    p_requirement.add_argument("--skip-content-population", action="store_true")
    p_requirement.add_argument("--skip-handoff-check", action="store_true")

    p_execution = subparsers.add_parser("execution", aliases=["exec"], help="Run execution round")
    p_execution.add_argument("--workspace-root", default=".")
    p_execution.add_argument("--req-id", default="")
    p_execution.add_argument("--task-id", default="")
    p_execution.add_argument("--summary", default="")
    p_execution.add_argument("--mode", choices=("auto", "feature", "bugfix", "continuation"), default="auto")
    p_execution.add_argument("--dry-run", action="store_true")
    p_execution.add_argument("--no-commit", action="store_true")
    p_execution.add_argument("--no-push", action="store_true")
    p_execution.add_argument("--no-release-gate", action="store_true")
    p_execution.add_argument("--archive-task-memory", action="store_true")

    p_archive = subparsers.add_parser("archive", aliases=["arc"], help="Archive task memory")
    p_archive.add_argument("--workspace-root", default=".")
    p_archive.add_argument("--task-id", required=True)
    p_archive.add_argument("--task-dir", required=True)
    p_archive.add_argument("--dry-run", action="store_true")

    args = parser.parse_args()

    if args.command in {"bootstrap", "init"}:
        cmd = [
            sys.executable,
            str(BOOTSTRAP_DIR / "init_workflow_bootstrap.py"),
            "--workspace-root",
            args.workspace_root,
        ]
        for host in args.host or ["codex"]:
            cmd.extend(["--host", host])
        if args.dry_run:
            cmd.append("--dry-run")
        return run(cmd, f"{COMMAND_ALIASES['bootstrap']} init")

    if args.command in {"health", "doctor"}:
        return run(
            [
                sys.executable,
                str(BOOTSTRAP_DIR / "check_workflow_health.py"),
                "--workspace-root",
                args.workspace_root,
            ],
            f"{COMMAND_ALIASES['health']} health",
        )

    if args.command in {"requirement", "req"}:
        cmd = [
            sys.executable,
            str(REQUIREMENT_DIR / "run_requirement_round.py"),
            "--theme",
            args.theme,
            "--summary",
            args.summary,
        ]
        if args.skip_content_population:
            cmd.append("--skip-content-population")
        if args.skip_handoff_check:
            cmd.append("--skip-handoff-check")
        if args.dry_run:
            cmd.append("--dry-run")
        return run(["/bin/zsh", "-lc", f"cd {sh_quote(args.workspace_root)} && {' '.join(sh_quote(part) for part in cmd)}"], f"{COMMAND_ALIASES['requirement']} requirement")

    if args.command in {"execution", "exec"}:
        cmd = [
            sys.executable,
            str(EXECUTION_DIR / "run_execution_round.py"),
            "--confirm-start",
            "--summary",
            args.summary,
            "--mode",
            args.mode,
        ]
        if args.req_id:
            cmd.extend(["--req-id", args.req_id])
        if args.task_id:
            cmd.extend(["--task-id", args.task_id])
        if args.dry_run:
            cmd.append("--dry-run")
        if args.no_commit:
            cmd.append("--no-commit")
        if args.no_push:
            cmd.append("--no-push")
        if args.no_release_gate:
            cmd.append("--no-release-gate")
        if args.archive_task_memory:
            cmd.append("--archive-task-memory")
        return run(["/bin/zsh", "-lc", f"cd {sh_quote(args.workspace_root)} && {' '.join(sh_quote(part) for part in cmd)}"], f"{COMMAND_ALIASES['execution']} execution")

    if args.command in {"archive", "arc"}:
        cmd = [
            sys.executable,
            str(EXECUTION_DIR / "archive_task_memory.py"),
            "--task-id",
            args.task_id,
            "--task-dir",
            args.task_dir,
        ]
        if args.dry_run:
            cmd.append("--dry-run")
        return run(["/bin/zsh", "-lc", f"cd {sh_quote(args.workspace_root)} && {' '.join(sh_quote(part) for part in cmd)}"], f"{COMMAND_ALIASES['archive']} archive")

    return 1


def sh_quote(value: str) -> str:
    return "'" + value.replace("'", "'\"'\"'") + "'"


if __name__ == "__main__":
    raise SystemExit(main())
