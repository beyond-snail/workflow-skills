#!/usr/bin/env python3
from __future__ import annotations

import argparse
import subprocess
from pathlib import Path
from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import find_requirement_row, get_cell
from profile_paths import ProjectPaths


DEFAULT_EVIDENCE_KEYWORDS = ["阻塞", "替代验证"]


def run_shell(cmd: str, cwd: Path) -> tuple[int, str]:
    proc = subprocess.run(
        cmd,
        cwd=str(cwd),
        shell=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return proc.returncode, proc.stdout.strip()


def find_git_root(start: Path) -> Path:
    current = start.resolve()
    for path in [current, *current.parents]:
        if (path / ".git").exists():
            return path
    return current


def print_result(ok: bool, label: str, detail: str | None = None) -> None:
    tag = "PASS" if ok else "BLOCK"
    print(f"[{tag}] {label}")
    if detail:
        for line in detail.splitlines():
            print(f"  {line}")


def load_req_status(req_file: Path, req_id: str) -> tuple[str | None, str | None]:
    row = find_requirement_row(req_file, req_id)
    if row is None:
        return None, None
    title = get_cell(row.cells, row.header_map, ("标题", "需求标题"))
    status = get_cell(row.cells, row.header_map, ("状态",))
    return title, status.lower() if status else None


def check_evidence_files(files: list[Path], keywords: list[str]) -> tuple[bool, str]:
    if not files:
        return False, "未提供阻塞证明文件"
    checked: list[str] = []
    for file in files:
        if not file.exists():
            checked.append(f"{file}: 文件不存在")
            continue
        content = file.read_text(encoding="utf-8")
        missing = [kw for kw in keywords if kw not in content]
        if missing:
            checked.append(f"{file}: 缺少关键字 {', '.join(missing)}")
            continue
        checked.append(f"{file}: 已包含阻塞与替代验证说明")
        return True, "\n".join(checked)
    return False, "\n".join(checked)


def main() -> int:
    parser = argparse.ArgumentParser(description="Run generic release gate checks for workflow-execution")
    add_profile_arg(parser)
    parser.add_argument("--project-root", help="Project root path")
    parser.add_argument("--req-file", help="Requirement pool markdown path")
    parser.add_argument("--req-id", help="Requirement ID")
    parser.add_argument("--allowed-req-status", action="append", default=["done"], help="Allowed requirement status, repeatable")
    parser.add_argument("--doc-file", action="append", default=[], help="Required doc file, repeatable")
    parser.add_argument("--build-cmd", action="append", default=[], help="Build command, repeatable")
    parser.add_argument("--test-cmd", action="append", default=[], help="Test command, repeatable")
    parser.add_argument("--require-clean-worktree", action="store_true", help="Require clean git worktree")
    parser.add_argument("--allow-test-blocked", action="store_true", help="Allow test command failure if blocker evidence exists")
    parser.add_argument("--evidence-file", action="append", default=[], help="Evidence markdown file for blocked tests, repeatable")
    parser.add_argument("--evidence-keyword", action="append", default=[], help="Required keyword in blocker evidence files, repeatable")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    project_root = Path(args.project_root).resolve() if args.project_root else find_git_root(project_paths.workspace_root)
    req_file = Path(args.req_file).resolve() if args.req_file else project_paths.requirements_pool
    doc_files = [Path(p).resolve() for p in args.doc_file]
    evidence_files = [Path(p).resolve() for p in args.evidence_file]
    allowed_req_status = {status.strip().lower() for status in args.allowed_req_status if status.strip()}
    evidence_keywords = args.evidence_keyword or DEFAULT_EVIDENCE_KEYWORDS

    print_header(
        "Release Gate",
        {
            "project_root": str(project_root),
            "req_id": args.req_id,
            "mode": "dry-run" if args.dry_run else None,
        },
    )

    blocked = False

    if req_file and args.req_id:
        title, status = load_req_status(req_file, args.req_id)
        ok = status in allowed_req_status if status else False
        print_result(ok, "需求状态检查", f"title={title or '(not found)'}\nstatus={status or '(not found)'}\nallowed={', '.join(sorted(allowed_req_status))}")
        blocked = blocked or not ok

    for doc_file in doc_files:
        ok = doc_file.exists()
        print_result(ok, f"文档存在检查: {doc_file}", None if ok else "文件不存在")
        blocked = blocked or not ok

    if args.require_clean_worktree:
        if args.dry_run:
            print_result(True, "工作区检查", "dry-run 未执行 git status --short")
        else:
            code, output = run_shell("git status --short", project_root)
            ok = code == 0 and not output.strip()
            detail = output if output else "工作区干净"
            print_result(ok, "工作区检查", detail)
            blocked = blocked or not ok

    def run_command_group(label: str, commands: list[str], allow_blocked: bool = False) -> None:
        nonlocal blocked
        for cmd in commands:
            if args.dry_run:
                print_result(True, f"{label}: {cmd}", "dry-run 未执行")
                continue
            code, output = run_shell(cmd, project_root)
            ok = code == 0
            if ok:
                print_result(True, f"{label}: {cmd}", output or "命令成功")
                continue
            if allow_blocked:
                evidence_ok, evidence_detail = check_evidence_files(evidence_files, evidence_keywords)
                print_result(evidence_ok, f"{label}: {cmd}", (output or "命令失败") + ("\n" + evidence_detail if evidence_detail else ""))
                blocked = blocked or not evidence_ok
            else:
                print_result(False, f"{label}: {cmd}", output or "命令失败")
                blocked = True

    run_command_group("构建检查", args.build_cmd)
    run_command_group("测试检查", args.test_cmd, allow_blocked=args.allow_test_blocked)

    if blocked:
        print("[BLOCK] 发布闸门未通过")
        return 1

    print("[PASS] 发布闸门通过")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
