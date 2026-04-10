#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import shlex
import shutil
from pathlib import Path


REQUIRED_FILES = (
    "AGENTS.md",
    "PROJECT_CONTEXT.md",
    "doc/开发协作约定.md",
    ".ai/memory/tasks/index.md",
    ".ai/memory/knowledge/README.md",
    ".ai/runtime/profile/project-profile.yml",
    "doc/requirements/需求池.md",
    "doc/requirements/任务看板.md",
)

HOST_FILES = (
    ".ai/governance/codex-host.md",
    ".ai/governance/claude-host.md",
)

LEGACY_PATHS = (
    "doc/tasks",
    "doc/knowledge",
    "CLAUDE.md",
    "CODEX.md",
)

SKILL_NAMES = ("workflow-bootstrap", "workflow-requirement", "workflow-execution")
VERSION_RE = re.compile(r"# Workflow .+ Skill \(v([^)]+)\)")


def scan_markdown(root: Path, needle: str) -> list[str]:
    hits: list[str] = []
    ignored = {".git", "node_modules", "target", "build", ".idea", ".vscode"}
    for path in root.rglob("*.md"):
        if any(part in ignored for part in path.parts):
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        if needle in text:
            hits.append(str(path.relative_to(root)))
    return hits


def extract_skill_version(skill_md: Path) -> str:
    if not skill_md.exists():
        return ""
    text = skill_md.read_text(encoding="utf-8", errors="ignore")
    match = VERSION_RE.search(text)
    return match.group(1) if match else ""


def profile_path(root: Path) -> Path:
    return root / ".ai/runtime/profile/project-profile.yml"


def check_profile_content(root: Path) -> list[str]:
    warnings: list[str] = []
    path = profile_path(root)
    if not path.exists():
        return ["profile_missing"]
    text = path.read_text(encoding="utf-8", errors="ignore")
    for required in (
        'requirements_pool:',
        'task_board:',
        'tasks_index:',
        'tasks_template_dir:',
        'knowledge_dir:',
        'compile:',
        'test:',
    ):
        if required not in text:
            warnings.append(f"profile_key_missing={required.rstrip(':')}")
    return warnings


def parse_profile_commands(root: Path) -> dict[str, str]:
    commands = {"compile": "", "test": ""}
    path = profile_path(root)
    if not path.exists():
        return commands
    text = path.read_text(encoding="utf-8", errors="ignore")
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("compile:"):
            commands["compile"] = stripped.split(":", 1)[1].strip().strip('"')
        elif stripped.startswith("test:"):
            commands["test"] = stripped.split(":", 1)[1].strip().strip('"')
    return commands


def check_command_available(root: Path, command: str) -> str:
    if not command:
        return "empty"
    token = shlex.split(command)[0]
    if token.startswith("./"):
        return "ok" if (root / token).exists() else "missing"
    return "ok" if shutil.which(token) else "missing"


def inspect_task_index(root: Path) -> tuple[list[str], list[str]]:
    infos: list[str] = []
    warnings: list[str] = []
    index_file = root / ".ai/memory/tasks/index.md"
    if not index_file.exists():
        return infos, ["task_index_missing"]

    seen_ids: set[str] = set()
    counts = {"todo": 0, "doing": 0, "blocked": 0, "done": 0, "archived": 0}
    for line in index_file.read_text(encoding="utf-8", errors="ignore").splitlines():
        if not line.startswith("| `TASK-"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 8:
            warnings.append("task_index_row_malformed")
            continue
        task_id = cells[0].strip("`")
        status = cells[4]
        directory = cells[6].strip("`")
        if task_id in seen_ids:
            warnings.append(f"duplicate_task_id={task_id}")
        seen_ids.add(task_id)
        if status in counts:
            counts[status] += 1
        task_dir = root / directory
        if not task_dir.exists():
            warnings.append(f"task_dir_missing[{task_id}]={directory}")
        in_archived = "archived" in Path(directory).parts
        if status == "archived" and not in_archived:
            warnings.append(f"archived_status_path_mismatch[{task_id}]={directory}")
        if status != "archived" and in_archived:
            warnings.append(f"active_status_path_mismatch[{task_id}]={directory}")
    infos.append("task_index_counts=" + ",".join(f"{key}:{value}" for key, value in counts.items()))
    return infos, warnings


def main() -> int:
    parser = argparse.ArgumentParser(description="Check workflow + memory health")
    parser.add_argument("--workspace-root", default=".")
    args = parser.parse_args()

    root = Path(args.workspace_root).resolve()
    missing = [rel for rel in REQUIRED_FILES if not (root / rel).exists()]
    host_missing = [rel for rel in HOST_FILES if not (root / rel).exists()]
    legacy_existing = [rel for rel in LEGACY_PATHS if (root / rel).exists()]

    reference_hits: dict[str, list[str]] = {}
    for legacy in ("doc/tasks", "doc/knowledge"):
        hits = scan_markdown(root, legacy)
        if hits:
            reference_hits[legacy] = hits

    profile_warnings = check_profile_content(root)
    profile_commands = parse_profile_commands(root)
    index_infos, index_warnings = inspect_task_index(root)
    codex_versions: dict[str, str] = {}
    claude_versions: dict[str, str] = {}
    for skill in SKILL_NAMES:
        codex_versions[skill] = extract_skill_version(Path.home() / ".codex/skills" / skill / "SKILL.md")
        claude_versions[skill] = extract_skill_version(Path.home() / ".claude/skills" / skill / "SKILL.md")

    warnings = 0
    print("# Workflow Health Check")
    print(f"- workspace_root: {root}")
    if missing:
        warnings += 1
        print(f"- warning: missing_required={', '.join(missing)}")
    else:
        print("- info: required_files=ok")
    if host_missing:
        warnings += 1
        print(f"- warning: missing_host_files={', '.join(host_missing)}")
    else:
        print("- info: host_files=ok")
    if legacy_existing:
        warnings += 1
        print(f"- warning: legacy_paths_exist={', '.join(legacy_existing)}")
    else:
        print("- info: legacy_paths=clean")
    if reference_hits:
        warnings += 1
        for legacy, hits in reference_hits.items():
            print(f"- warning: legacy_reference[{legacy}]={', '.join(hits)}")
    else:
        print("- info: legacy_references=clean")
    if profile_warnings:
        warnings += 1
        for item in profile_warnings:
            print(f"- warning: {item}")
    else:
        print(f"- info: profile_file={profile_path(root)}")
    for name, command in profile_commands.items():
        status = check_command_available(root, command)
        if status == "ok":
            print(f"- info: profile_command[{name}]={command}")
        else:
            warnings += 1
            print(f"- warning: profile_command[{name}]={status}:{command or '(empty)'}")
    for item in index_infos:
        print(f"- info: {item}")
    if index_warnings:
        warnings += 1
        for item in index_warnings:
            print(f"- warning: {item}")
    version_mismatch = False
    for skill in SKILL_NAMES:
        codex_version = codex_versions.get(skill, "")
        claude_version = claude_versions.get(skill, "")
        print(f"- info: skill_version[{skill}]=codex:{codex_version or '(missing)'}, claude:{claude_version or '(missing)'}")
        if not codex_version or not claude_version or codex_version != claude_version:
            version_mismatch = True
    if version_mismatch:
        warnings += 1
        print("- warning: skill_version_mismatch_detected")
    print(
        "- summary: "
        f"warnings={warnings} "
        f"required={'ok' if not missing else 'missing'} "
        f"hosts={'ok' if not host_missing else 'missing'} "
        f"legacy={'clean' if not legacy_existing and not reference_hits else 'dirty'}"
    )
    print(f"- result: {'WARN' if warnings else 'PASS'}")
    return 0 if warnings == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
