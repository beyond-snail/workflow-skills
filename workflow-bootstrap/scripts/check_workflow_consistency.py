#!/usr/bin/env python3
from __future__ import annotations

import argparse
import difflib
import hashlib
import re
from pathlib import Path


REQUIRED_EQUAL_FILES = (
    (
        "workflow-requirement/references/shared-governance.md",
        "workflow-execution/references/shared-governance.md",
    ),
)

WATCHED_SCRIPT_PAIRS = (
    (
        "workflow-requirement/scripts/project_state.py",
        "workflow-execution/scripts/project_state.py",
    ),
    (
        "workflow-requirement/scripts/cli_common.py",
        "workflow-execution/scripts/cli_common.py",
    ),
    (
        "workflow-requirement/scripts/profile_paths.py",
        "workflow-execution/scripts/profile_paths.py",
    ),
)

EXPECTED_DOCS_ROOT_DEFAULT = "docs/workflow"
EXPECTED_PRD_DIR_DEFAULT = "docs/workflow/PRD"

DOCS_ROOT_RE = re.compile(r'docs_root",\s*"([^"]+)"\)')
PRD_DIR_RE = re.compile(r'prd_directory",\s*"([^"]+)"\)')


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="ignore")


def file_sha1(path: Path) -> str:
    return hashlib.sha1(path.read_bytes()).hexdigest()


def diff_lines(a: Path, b: Path) -> int:
    lines = list(
        difflib.unified_diff(
            read_text(a).splitlines(),
            read_text(b).splitlines(),
            fromfile=str(a),
            tofile=str(b),
            lineterm="",
        )
    )
    return max(0, len(lines) - 2) if lines else 0


def parse_defaults(project_state_file: Path) -> tuple[set[str], set[str]]:
    text = read_text(project_state_file)
    docs_root_defaults = set(DOCS_ROOT_RE.findall(text))
    prd_dir_defaults = set(PRD_DIR_RE.findall(text))
    return docs_root_defaults, prd_dir_defaults


def main() -> int:
    parser = argparse.ArgumentParser(description="Check consistency between workflow skills")
    parser.add_argument("--workspace-root", default=".")
    parser.add_argument("--fail-on-warn", action="store_true", help="Return non-zero if warnings exist")
    args = parser.parse_args()

    root = Path(args.workspace_root).resolve()
    errors = 0
    warnings = 0

    print("# Workflow Consistency Check")
    print(f"- workspace_root: {root}")

    for left_rel, right_rel in REQUIRED_EQUAL_FILES:
        left = root / left_rel
        right = root / right_rel
        if not left.exists() or not right.exists():
            errors += 1
            print(f"- error: missing_file_pair={left_rel},{right_rel}")
            continue
        if file_sha1(left) != file_sha1(right):
            errors += 1
            print(f"- error: required_equal_mismatch={left_rel}<>{right_rel}")
        else:
            print(f"- info: required_equal_ok={left_rel}<>{right_rel}")

    req_state = root / "workflow-requirement/scripts/project_state.py"
    exec_state = root / "workflow-execution/scripts/project_state.py"
    if not req_state.exists() or not exec_state.exists():
        errors += 1
        print("- error: project_state_missing")
    else:
        req_docs, req_prd = parse_defaults(req_state)
        exe_docs, exe_prd = parse_defaults(exec_state)

        if req_docs != exe_docs or req_prd != exe_prd:
            errors += 1
            print("- error: project_state_default_mismatch=requirement<>execution")
        else:
            print("- info: project_state_default_match=requirement<>execution")

        if req_docs != {EXPECTED_DOCS_ROOT_DEFAULT}:
            errors += 1
            print(f"- error: docs_root_default_unexpected={sorted(req_docs)}")
        else:
            print(f"- info: docs_root_default={EXPECTED_DOCS_ROOT_DEFAULT}")

        if req_prd != {EXPECTED_PRD_DIR_DEFAULT}:
            errors += 1
            print(f"- error: prd_directory_default_unexpected={sorted(req_prd)}")
        else:
            print(f"- info: prd_directory_default={EXPECTED_PRD_DIR_DEFAULT}")

    for left_rel, right_rel in WATCHED_SCRIPT_PAIRS:
        left = root / left_rel
        right = root / right_rel
        if not left.exists() or not right.exists():
            warnings += 1
            print(f"- warning: watched_pair_missing={left_rel},{right_rel}")
            continue
        if file_sha1(left) == file_sha1(right):
            print(f"- info: watched_pair_match={left_rel}<>{right_rel}")
            continue
        warnings += 1
        changed = diff_lines(left, right)
        print(f"- warning: watched_pair_drift={left_rel}<>{right_rel},changed_lines={changed}")

    print(f"- summary: errors={errors} warnings={warnings}")
    if errors:
        print("- result: FAIL")
        return 1
    if warnings and args.fail_on_warn:
        print("- result: WARN")
        return 1
    print(f"- result: {'WARN' if warnings else 'PASS'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
