#!/usr/bin/env python3
from __future__ import annotations

import argparse
import fnmatch
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
import os

try:
    import tomllib  # py3.11+
except ModuleNotFoundError:  # pragma: no cover
    tomllib = None

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from legacy_context import load_legacy_scan, match_legacy_context, render_legacy_context_lines
from md_board_utils import (
    find_requirement_row,
    format_md_row,
    get_cell,
    normalize,
    parse_table_rows,
    preserve_cell_style,
)
from profile_paths import ProjectPaths
from project_state import build_project_state, write_project_state

from select_next_task import TaskRecord, load_tasks


SCRIPT_DIR = Path(__file__).resolve().parent
LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
DECISION_SIGNAL_RE = re.compile(r"(根因|决定|最终发现|改成|结论)")
CONTINUATION_SIGNAL_RE = re.compile(r"(继续|收口|遗留|上次|延续|接着|未完成)")
BUGFIX_SIGNAL_RE = re.compile(r"(bug|缺陷|测试问题|报错|异常|失败|修复|修一下|问题)", re.IGNORECASE)
HIGH_RISK_WRITEBACK_PATTERNS = {
    "发布/部署": re.compile(r"(发布|上线|部署|release|deploy|上线前|发布前)", re.IGNORECASE),
    "验收/回归": re.compile(r"(验收|回归|UAT|正式测试|测试报告|联调记录|验收材料)", re.IGNORECASE),
    "生产数据": re.compile(r"(生产数据|线上数据|生产环境|线上环境|prod|production)", re.IGNORECASE),
    "SQL/迁移": re.compile(r"(SQL|DDL|DML|数据迁移|数据修复|回滚脚本|sql)", re.IGNORECASE),
    "权限/安全": re.compile(r"(权限|安全|鉴权|认证|授权|auth|permission|security)", re.IGNORECASE),
    "跨模块接口": re.compile(r"(跨模块|外部接口|接口联调|开放接口|API|api)", re.IGNORECASE),
    "客户交付": re.compile(r"(客户|交付|对外交付|客户验收|生产交付)", re.IGNORECASE),
}
FOCUS_CODE_PATTERNS = (
    re.compile(r"\b[A-Z]{1,10}-\d{1,8}\b", re.IGNORECASE),
    re.compile(r"\b[A-Z]{1,6}\d{2,8}\b", re.IGNORECASE),
)


@dataclass
class RequirementContext:
    req_id: str
    title: str
    root_dir: Path | None
    prd_trace: Path | None
    acceptance_files: list[Path]
    test_result_files: list[Path]


@dataclass
class LocalConfigGuard:
    source_file: Path
    enabled: bool
    action: str
    patterns: list[str]


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


def build_context_brief_args(
    project_paths: ProjectPaths,
    selected: TaskRecord,
    req_id: str | None,
    req_title: str,
    task_status: str,
    mode: str,
    summary: str,
    evidence_files: list[str],
    changed_files: list[str],
    verification_lines: list[str],
    blockers: list[str],
    next_steps: list[str],
) -> list[str]:
    helper_args = [
        "--workspace-root",
        str(project_paths.workspace_root),
        "--req-id",
        req_id or selected.req_id or "",
        "--req-title",
        req_title,
        "--task-id",
        selected.task_id,
        "--task-title",
        selected.title,
        "--task-status",
        task_status,
        "--mode",
        mode,
        "--summary",
        summary,
    ]
    for item in changed_files:
        helper_args.extend(["--changed-file", item])
    for item in evidence_files:
        helper_args.extend(["--evidence", item])
    for item in verification_lines:
        helper_args.extend(["--verification", item])
    for item in blockers:
        helper_args.extend(["--blocker", item])
    for item in next_steps:
        helper_args.extend(["--next-step", item])
    return helper_args


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


def _strip_quotes(value: str) -> str:
    text = value.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in {"'", '"'}:
        return text[1:-1]
    return text


def _parse_simple_yaml_guard(path: Path) -> dict[str, object]:
    data: dict[str, object] = {}
    current_list_key: str | None = None
    lines = path.read_text(encoding="utf-8").splitlines()
    for raw in lines:
        line = raw.split("#", 1)[0].rstrip()
        if not line.strip():
            continue
        stripped = line.lstrip()
        if stripped.startswith("- "):
            if current_list_key is None:
                continue
            item = _strip_quotes(stripped[2:].strip())
            data.setdefault(current_list_key, [])
            if isinstance(data[current_list_key], list) and item:
                data[current_list_key].append(item)
            continue
        current_list_key = None
        if ":" not in stripped:
            continue
        key, value = stripped.split(":", 1)
        key = key.strip()
        value = value.strip()
        if not value:
            data[key] = []
            current_list_key = key
            continue
        lowered = value.lower()
        if lowered in {"true", "false"}:
            data[key] = lowered == "true"
            continue
        data[key] = _strip_quotes(value)
    return data


def load_local_config_guard(workspace_root: Path, explicit_file: str | None) -> LocalConfigGuard | None:
    candidates: list[Path] = []
    if explicit_file:
        explicit = Path(explicit_file)
        candidates.append(explicit if explicit.is_absolute() else (workspace_root / explicit))
    else:
        candidates.extend(
            [
                workspace_root / ".ai/governance/local-config-guard.toml",
                workspace_root / ".ai/governance/local-config-guard.json",
                workspace_root / ".ai/governance/local-config-guard.yml",
                workspace_root / ".ai/governance/local-config-guard.yaml",
            ]
        )

    config_file = next((path for path in candidates if path.exists()), None)
    if config_file is None:
        return None

    suffix = config_file.suffix.lower()
    raw: dict[str, object]
    if suffix == ".toml":
        if tomllib is None:
            print(f"[WARN] local-config-guard: tomllib unavailable, skip `{config_file}`")
            return None
        raw = tomllib.loads(config_file.read_text(encoding="utf-8"))
    elif suffix == ".json":
        raw = json.loads(config_file.read_text(encoding="utf-8"))
    elif suffix in {".yml", ".yaml"}:
        raw = _parse_simple_yaml_guard(config_file)
    else:
        print(f"[WARN] local-config-guard: unsupported format `{config_file}`")
        return None

    enabled = bool(raw.get("enabled", True))
    action = str(raw.get("action", "warn")).strip().lower()
    if action not in {"warn", "block", "auto-unstage"}:
        print(f"[WARN] local-config-guard: invalid action `{action}`, fallback to `warn`")
        action = "warn"
    patterns = [str(item).strip().replace("\\", "/") for item in (raw.get("patterns") or []) if str(item).strip()]
    return LocalConfigGuard(
        source_file=config_file,
        enabled=enabled,
        action=action,
        patterns=patterns,
    )


def _normalize_repo_path(path_text: str, git_root: Path) -> str:
    text = path_text.strip()
    if not text:
        return text
    if " -> " in text:
        text = text.split(" -> ", 1)[1]
    candidate = Path(text)
    if candidate.is_absolute():
        try:
            text = str(candidate.resolve().relative_to(git_root.resolve()))
        except ValueError:
            text = str(candidate)
    normalized = os.path.normpath(text).replace("\\", "/")
    return normalized


def apply_local_config_guard(
    stage_files: list[str],
    guard: LocalConfigGuard,
    git_root: Path,
) -> tuple[list[str], list[str], list[str]]:
    logs: list[str] = []
    blockers: list[str] = []
    if not stage_files:
        return stage_files, logs, blockers
    if not guard.enabled or not guard.patterns:
        logs.append(
            f"- local-config-guard: disabled or empty patterns ({guard.source_file})"
        )
        return stage_files, logs, blockers

    normalized_map = {path: _normalize_repo_path(path, git_root) for path in stage_files}
    matched: list[tuple[str, str]] = []
    for original, normalized in normalized_map.items():
        for pattern in guard.patterns:
            if fnmatch.fnmatch(normalized, pattern):
                matched.append((original, pattern))
                break

    logs.append(
        f"- local-config-guard: source={guard.source_file} action={guard.action} patterns={len(guard.patterns)} hits={len(matched)}"
    )
    if not matched:
        return stage_files, logs, blockers

    hits = ", ".join(f"{_normalize_repo_path(path, git_root)} <= {pattern}" for path, pattern in matched)
    if guard.action == "warn":
        logs.append(f"[WARN] local-config-guard: protected files detected: {hits}")
        return stage_files, logs, blockers

    if guard.action == "block":
        blockers.append(f"命中本地配置保护策略，禁止提交：{hits}")
        logs.append(f"[BLOCK] local-config-guard: blocked protected files: {hits}")
        return stage_files, logs, blockers

    # auto-unstage
    blocked_set = {item[0] for item in matched}
    filtered = [path for path in stage_files if path not in blocked_set]
    logs.append(f"[WARN] local-config-guard: auto-unstage protected files: {hits}")
    if not filtered:
        blockers.append("本地配置保护过滤后无可提交文件")
    return filtered, logs, blockers


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
    record_format: str = "audit",
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
        selected_verifications = verification_lines if record_format == "audit" else verification_lines[:2]
        for item in selected_verifications:
            helper_args.extend(["--verification", item])
        for item in artifacts:
            helper_args.extend(["--artifact", item])
        selected_blockers = blockers if record_format == "audit" else blockers[:2]
        selected_next_steps = next_steps if record_format == "audit" else next_steps[:1]
        for item in selected_blockers:
            helper_args.extend(["--blocker", item])
        for item in selected_next_steps:
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
    steps: list[dict[str, str]],
    conclusions: list[str],
    blockers: list[str],
    alternatives: list[str],
    dry_run: bool,
    record_format: str = "compact",
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
            "--format",
            record_format,
        ]
        if summary:
            helper_args.extend(["--summary", summary])
        for item in commands:
            helper_args.extend(["--command", item])
        selected_results = results if record_format == "audit" else results[:2]
        selected_steps = steps if record_format == "audit" else []
        for item in selected_results:
            helper_args.extend(["--result", item])
        for item in selected_steps:
            helper_args.extend(["--step", json.dumps(item, ensure_ascii=False)])
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


def derive_focus_keywords(
    task: TaskRecord,
    summary: str,
    extra_keywords: list[str],
    req_id: str = "",
    req_title: str = "",
) -> list[str]:
    raw_keywords = [item.strip() for item in extra_keywords if item and item.strip()]
    source_text = " ".join([task.task_id or "", task.title or "", summary or "", req_id or "", req_title or ""])

    detected_codes: list[str] = []
    for pattern in FOCUS_CODE_PATTERNS:
        detected_codes.extend(match.group(0).upper() for match in pattern.finditer(source_text))
    candidates = [*raw_keywords, *detected_codes, task.task_id.strip(), task.title.strip(), req_id.strip(), req_title.strip()]

    keywords: list[str] = []
    seen: set[str] = set()
    for candidate in candidates:
        token = re.sub(r"\s+", " ", candidate).strip()
        if len(token) < 2:
            continue
        key = token.lower()
        if key in seen:
            continue
        seen.add(key)
        keywords.append(token)
    return keywords


def _header_index(header_map: dict[str, int], aliases: tuple[str, ...]) -> int | None:
    for alias in aliases:
        idx = header_map.get(normalize(alias))
        if idx is not None:
            return idx
    return None


def _match_score(text: str, keywords: list[str]) -> int:
    lowered = text.lower()
    score = 0
    for keyword in keywords:
        token = keyword.lower().strip()
        if not token:
            continue
        if token in lowered:
            score += max(3, len(token))
    return score


def to_status_cn(status: str) -> str:
    if status == "pass":
        return "通过"
    if status == "blocked":
        return "阻塞"
    if status == "fail":
        return "失败"
    return status


def shorten_text(text: str, limit: int = 120) -> str:
    compact = re.sub(r"\s+", " ", text).strip()
    if len(compact) <= limit:
        return compact
    return compact[: max(0, limit - 1)] + "…"


def summarize_command_output(text: str, limit: int = 96) -> str:
    if not text.strip():
        return "命令无输出"
    first_line = next((line.strip() for line in text.splitlines() if line.strip()), "")
    if not first_line:
        first_line = "命令输出为空行"
    return shorten_text(first_line, limit)


def build_row_actual_text(
    status_cn: str,
    summary: str,
    self_test_notes: list[str],
    override_value: str,
) -> str:
    if override_value.strip():
        return override_value.strip()
    payload: list[str] = []
    if summary.strip():
        payload.append(summary.strip())
    for note in self_test_notes[:2]:
        note_text = note.strip()
        if note_text:
            payload.append(note_text)
    if not payload:
        payload.append("自动执行验证")
    return f"{status_cn}：{shorten_text('；'.join(payload), 160)}"


def build_row_evidence_text(
    artifacts: list[str],
    commands: list[str],
    results: list[str],
    override_value: str,
) -> str:
    if override_value.strip():
        return override_value.strip()
    segments: list[str] = []
    if artifacts:
        segments.append("产物:" + ", ".join(shorten_text(item, 48) for item in artifacts[:2]))
    if commands:
        segments.append("命令:" + " | ".join(shorten_text(item, 48) for item in commands[:2]))
    if results:
        segments.append("结果:" + shorten_text(results[0], 72))
    if not segments:
        segments.append("自动回写记录")
    return "；".join(segments)


def update_execution_table_rows(
    files: list[Path],
    keywords: list[str],
    actual_text: str,
    evidence_text: str,
    status_text: str,
    dry_run: bool,
) -> None:
    if not files:
        return

    for file in files:
        if not file.exists():
            continue
        rows = parse_table_rows(file)
        if not rows:
            continue

        best_row = None
        best_score = 0
        for row in rows:
            actual_idx = _header_index(row.header_map, ("实际", "实际结果"))
            evidence_idx = _header_index(row.header_map, ("证据",))
            status_idx = _header_index(row.header_map, ("状态",))
            if actual_idx is None or evidence_idx is None or status_idx is None:
                continue
            row_text = " ".join(cell for cell in row.cells if cell)
            score = _match_score(row_text, keywords)
            if score > best_score:
                best_score = score
                best_row = (row, actual_idx, evidence_idx, status_idx)

        if best_row is None:
            print(f"[WARN] table writeback skipped (no eligible row): {file}")
            continue
        if best_score <= 0:
            print(f"[WARN] table writeback skipped (no keyword hit): {file}")
            continue

        row, actual_idx, evidence_idx, status_idx = best_row
        lines = file.read_text(encoding="utf-8").splitlines()
        raw_cells = list(row.raw_cells)
        target_len = max(actual_idx, evidence_idx, status_idx) + 1
        if len(raw_cells) < target_len:
            raw_cells.extend([""] * (target_len - len(raw_cells)))

        raw_cells[actual_idx] = preserve_cell_style(raw_cells[actual_idx], actual_text)
        raw_cells[evidence_idx] = preserve_cell_style(raw_cells[evidence_idx], evidence_text)
        raw_cells[status_idx] = preserve_cell_style(raw_cells[status_idx], status_text)
        lines[row.line_index] = format_md_row(raw_cells)

        if dry_run:
            print(f"[DRY-RUN] table writeback: {file}:{row.line_index + 1}")
            continue

        file.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
        print(f"[PASS] table writeback: {file}:{row.line_index + 1}")


def writeback_enabled(writeback: str) -> bool:
    return writeback != "none"


def audit_writeback(writeback: str) -> bool:
    return writeback == "audit"


def high_risk_writeback_reasons(*values: str) -> list[str]:
    text = " ".join(value for value in values if value)
    if not text:
        return []
    return [label for label, pattern in HIGH_RISK_WRITEBACK_PATTERNS.items() if pattern.search(text)]


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Explicit execution entrypoint: requires manual review approval, then runs validation with compact writeback by default"
    )
    add_profile_arg(parser)
    parser.add_argument("--task-file", help="Task board path")
    parser.add_argument("--req-id", help="Requirement id filter")
    parser.add_argument("--task-id", help="Explicit task id; otherwise select doing or earliest todo")
    parser.add_argument("--confirm-start", action="store_true", help="Confirm manual review is complete and execution is explicitly authorized")
    parser.add_argument("--summary", default="", help="Execution summary for evidence and reporting")
    parser.add_argument("--writeback", choices=("none", "compact", "audit"), default="compact", help="Writeback detail. compact is default; audit keeps detailed evidence records.")
    parser.add_argument("--record-file", action="append", default=[], help="Markdown file to append task evidence into, repeatable")
    parser.add_argument("--test-result-file", action="append", default=[], help="Markdown file to append test result into, repeatable")
    parser.add_argument("--build-cmd", action="append", default=[], help="Build/compile command, repeatable")
    parser.add_argument("--test-cmd", action="append", default=[], help="Test command, repeatable")
    parser.add_argument("--no-default-build-cmd", action="store_true", help="Do not fallback to profile build.compile when --build-cmd is empty")
    parser.add_argument("--no-default-test-cmd", action="store_true", help="Do not fallback to profile build.test when --test-cmd is empty")
    parser.add_argument("--self-test-note", action="append", default=[], help="Manual self-test note (for partial execution), repeatable")
    parser.add_argument("--focus-keyword", action="append", default=[], help="Keyword used to locate target table row for auto writeback, repeatable")
    parser.add_argument("--row-actual", default="", help="Override value for table '实际/实际结果' cell")
    parser.add_argument("--row-evidence", default="", help="Override value for table '证据' cell")
    parser.add_argument("--skip-table-row-writeback", action="store_true", help="Skip updating table row cells; keep appendix-only writeback")
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
    parser.add_argument("--local-config-guard-file", help="Repo local config guard file (toml/json/yaml); default auto-detect under .ai/governance/")
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
    legacy_scan = load_legacy_scan(project_paths.workspace_root)
    legacy_context = match_legacy_context(
        legacy_scan,
        selected.title,
        args.summary,
        req_id or "",
        req_ctx.title if req_ctx else "",
    )
    if req_id and not selected.req_id:
        selected.req_id = req_id

    record_files = [Path(p).resolve() for p in args.record_file]
    if not record_files and req_ctx:
        record_files = req_ctx.acceptance_files[:1]

    test_result_files = [Path(p).resolve() for p in args.test_result_file]
    if not test_result_files and req_ctx:
        test_result_files = req_ctx.test_result_files[:1]

    build_commands = [cmd for cmd in args.build_cmd if cmd.strip()]
    if not build_commands and not args.no_default_build_cmd and profile.get("build", {}).get("compile"):
        build_commands.append(profile["build"]["compile"])

    test_commands = [cmd for cmd in args.test_cmd if cmd.strip()]
    if not test_commands and not args.no_default_test_cmd and profile.get("build", {}).get("test"):
        test_commands.append(profile["build"]["test"])
    self_test_notes = [item.strip() for item in args.self_test_note if item and item.strip()]
    focus_keywords = derive_focus_keywords(
        selected,
        args.summary,
        args.focus_keyword,
        req_id=req_id or "",
        req_title=req_ctx.title if req_ctx else "",
    )

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
    high_risk_reasons = high_risk_writeback_reasons(
        selected.title,
        selected.task_id,
        req_id or "",
        req_ctx.title if req_ctx else "",
        args.summary,
        " ".join(args.doc_file),
        " ".join(args.gate_evidence_file),
        " ".join(args.build_cmd),
        " ".join(args.test_cmd),
    )
    writeback_upgraded = False
    if args.writeback == "compact" and high_risk_reasons:
        args.writeback = "audit"
        writeback_upgraded = True
        print(f"- writeback_guard: high-risk {', '.join(high_risk_reasons)} -> audit")

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
            "writeback": args.writeback,
            "writeback_guard": "risk-upgrade" if writeback_upgraded else None,
            "mode": "dry-run" if args.dry_run else "live",
        },
    )
    for line in render_legacy_context_lines(legacy_context):
        print(line)

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
        print(f"- default_build_fallback: {'disabled' if args.no_default_build_cmd else 'enabled'}")
        print(f"- default_test_fallback: {'disabled' if args.no_default_test_cmd else 'enabled'}")
        print(f"- self_test_notes: {', '.join(self_test_notes) or '(none)'}")
        print(f"- focus_keywords: {', '.join(focus_keywords) or '(none)'}")
        table_row_writeback = audit_writeback(args.writeback) and not args.skip_table_row_writeback
        print(f"- writeback: {args.writeback}")
        print(f"- table_row_writeback: {'enabled' if table_row_writeback else 'disabled'}")
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
            summary=(args.summary or f"{selected.task_id} 执行预览") + f"；{legacy_context['summary']}",
            blockers=list(args.blocker) or warnings,
            evidence_refs=[*(str(p) for p in (*record_files, *test_result_files)), *legacy_context["evidence_refs"]],
            legacy_context=legacy_context,
        )
        preview_path = write_project_state(project_paths.workspace_root, preview_state, dry_run=True)
        print(f"- action: update project-state preview at `{preview_path}`")
        print(f"- action: update context-brief preview at `{project_paths.workspace_root / '.ai/memory/context-brief.md'}`")
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
    step_rows: list[dict[str, str]] = []
    blockers = list(args.blocker)
    next_steps = list(args.next_step)
    alternatives = list(args.alternative)
    task_blocked = False
    fatal_failure = False
    self_step_no = 0
    build_step_no = 0
    test_step_no = 0

    for note in self_test_notes:
        verification_lines.append(f"self-test `{note}` -> PASS")
        result_lines.append(f"[self-test] {note}")
        self_step_no += 1
        step_rows.append(
            {
                "id": f"SELF-{self_step_no:02d}",
                "action": f"手工自测：{shorten_text(note, 72)}",
                "expected": "自测过程可复核，结论与目标一致",
                "actual": "PASS（已记录自测结论）",
                "evidence": "见本次自动回写记录与联调证据",
            }
        )
    if not build_commands and not test_commands and not self_test_notes:
        blockers.append("未提供构建/测试命令，且缺少 --self-test-note 自测记录")
        task_blocked = True
        step_rows.append(
            {
                "id": "CHECK-01",
                "action": "验证入口检查",
                "expected": "至少存在构建/测试命令或手工自测记录",
                "actual": "BLOCKED（缺少可复核测试输入）",
                "evidence": "build/test/self-test 均为空",
            }
        )

    for cmd in build_commands:
        code, output = run_shell(cmd, git_root)
        build_step_no += 1
        build_status = "PASS" if code == 0 else "FAIL"
        verification_lines.append(f"build `{cmd}` -> {'PASS' if code == 0 else 'FAIL'}")
        result_lines.append(f"[build] {cmd}: {output or ('PASS' if code == 0 else 'FAIL')}")
        step_rows.append(
            {
                "id": f"BUILD-{build_step_no:02d}",
                "action": f"执行构建命令：`{shorten_text(cmd, 64)}`",
                "expected": "命令执行成功（退出码=0）",
                "actual": f"{build_status}（exit={code}）",
                "evidence": summarize_command_output(output),
            }
        )
        if code != 0:
            blockers.append(f"构建失败：{cmd}")
            fatal_failure = True
            break

    if not fatal_failure:
        for cmd in test_commands:
            code, output = run_shell(cmd, git_root)
            is_blocked = code != 0 and args.allow_test_blocked and (blockers or alternatives or gate_evidence_files)
            test_step_no += 1
            if code == 0:
                verification_lines.append(f"test `{cmd}` -> PASS")
                result_lines.append(f"[test] {cmd}: {output or 'PASS'}")
                step_rows.append(
                    {
                        "id": f"TEST-{test_step_no:02d}",
                        "action": f"执行测试命令：`{shorten_text(cmd, 64)}`",
                        "expected": "命令执行成功（退出码=0）",
                        "actual": "PASS（exit=0）",
                        "evidence": summarize_command_output(output),
                    }
                )
                continue

            if is_blocked:
                verification_lines.append(f"test `{cmd}` -> BLOCKED")
                result_lines.append(f"[test] {cmd}: {output or 'BLOCKED'}")
                blockers.append(f"测试阻塞：{cmd}")
                task_blocked = True
                step_rows.append(
                    {
                        "id": f"TEST-{test_step_no:02d}",
                        "action": f"执行测试命令：`{shorten_text(cmd, 64)}`",
                        "expected": "命令执行成功；若阻塞需记录阻塞与替代验证",
                        "actual": f"BLOCKED（exit={code}）",
                        "evidence": summarize_command_output(output),
                    }
                )
                continue

            verification_lines.append(f"test `{cmd}` -> FAIL")
            result_lines.append(f"[test] {cmd}: {output or 'FAIL'}")
            blockers.append(f"测试失败：{cmd}")
            step_rows.append(
                {
                    "id": f"TEST-{test_step_no:02d}",
                    "action": f"执行测试命令：`{shorten_text(cmd, 64)}`",
                    "expected": "命令执行成功（退出码=0）",
                    "actual": f"FAIL（exit={code}）",
                    "evidence": summarize_command_output(output),
                }
            )
            fatal_failure = True
            break

    test_status = "blocked" if task_blocked else ("fail" if fatal_failure else "pass")
    test_title = f"{selected.task_id} 自动执行"
    test_summary = args.summary or f"{selected.task_id} 自动验证结果"
    conclusions = ["自动执行已完成" if not fatal_failure else "自动执行存在失败项"]
    if self_test_notes:
        conclusions.append("包含手工自测记录")
    if task_blocked:
        conclusions = ["自动执行存在测试阻塞，已保留阻塞说明与替代验证"]

    if writeback_enabled(args.writeback):
        if append_test_results(
            test_result_files,
            test_status,
            test_title,
            test_summary,
            build_commands + test_commands,
            result_lines,
            step_rows,
            conclusions,
            blockers,
            alternatives,
            dry_run=False,
            record_format="audit" if audit_writeback(args.writeback) else "compact",
        ) != 0:
            return 1

    if audit_writeback(args.writeback) and not args.skip_table_row_writeback:
        status_cn = to_status_cn(test_status)
        row_actual_text = build_row_actual_text(status_cn, test_summary, self_test_notes, args.row_actual)
        row_evidence_text = build_row_evidence_text(
            args.artifact,
            build_commands + test_commands,
            result_lines,
            args.row_evidence,
        )
        table_files: list[Path] = []
        seen_files: set[str] = set()
        for candidate in [*test_result_files, *record_files]:
            key = str(candidate.resolve())
            if key in seen_files:
                continue
            seen_files.add(key)
            table_files.append(candidate)
        update_execution_table_rows(
            table_files,
            focus_keywords,
            row_actual_text,
            row_evidence_text,
            status_cn,
            dry_run=False,
        )

    commit_message = build_commit_message(selected, args.commit_type)
    commit_failed = False
    changed_files_for_brief = [item for item in args.stage_file if item.strip()]
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
            if stage_files:
                changed_files_for_brief = stage_files[:]
            guard = load_local_config_guard(project_paths.workspace_root, args.local_config_guard_file)
            if guard:
                stage_files, guard_logs, guard_blockers = apply_local_config_guard(stage_files, guard, git_root)
                if stage_files:
                    changed_files_for_brief = stage_files[:]
                for line in guard_logs:
                    print(line)
                blockers.extend(guard_blockers)
            if not stage_files:
                print("[BLOCK] 当前没有可提交变更，自动提交已跳过")
                fatal_failure = True
                commit_failed = True
                if not any("无可提交文件" in item or "未检测到可提交变更" in item for item in blockers):
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

    if audit_writeback(args.writeback):
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
            record_format="audit",
        ) != 0:
            return 1

    if task_memory_dir and writeback_enabled(args.writeback):
        verify_file = task_memory_dir / "verify.md"
        inbox_file = task_memory_dir / "inbox.md"
        issues_file = task_memory_dir / "issues.md"
        decisions_file = task_memory_dir / "decisions.md"
        verify_entries: list[tuple[str, str, str]] = []
        for cmd in build_commands:
            verify_entries.append(
                (
                    cmd,
                    "PASS" if all("FAIL" not in item for item in verification_lines if cmd in item) else "FAIL",
                    "自动执行构建校验",
                )
            )
        for cmd in test_commands:
            matched = next((item for item in verification_lines if cmd in item), "")
            result = "PASS"
            if "BLOCKED" in matched:
                result = "BLOCKED"
            elif "FAIL" in matched:
                result = "FAIL"
            verify_entries.append((cmd, result, "自动执行测试校验"))

        if not audit_writeback(args.writeback) and verify_entries:
            compact_results = {result for _, result, _ in verify_entries}
            if "FAIL" in compact_results:
                compact_result = "FAIL"
            elif "BLOCKED" in compact_results:
                compact_result = "BLOCKED"
            else:
                compact_result = "PASS"
            compact_action = "；".join(shorten_text(cmd, 48) for cmd, _, _ in verify_entries[:3])
            if len(verify_entries) > 3:
                compact_action += f"；+{len(verify_entries) - 3}"
            code, output = run_helper(
                "record_task_verify.py",
                [
                    "--file",
                    str(verify_file),
                    "--action",
                    compact_action,
                    "--result",
                    compact_result,
                    "--coverage",
                    "自动执行验证摘要",
                    "--risk",
                    shorten_text("; ".join(blockers), 160),
                ],
                dry_run=False,
            )
            if output:
                print(output)
            if code != 0:
                return code

        for cmd, result, coverage in verify_entries if audit_writeback(args.writeback) else []:
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
                    coverage,
                    "--risk",
                    "; ".join(blockers),
                ],
                dry_run=False,
            )
            if output:
                print(output)
            if code != 0:
                return code
        if audit_writeback(args.writeback) and resolved_mode in {"bugfix", "continuation"} and inbox_file.exists():
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
        summary=(args.summary or f"{selected.task_id} 执行回合已{final_task_status}") + f"；{legacy_context['summary']}",
        blockers=blockers,
        evidence_refs=[*(str(p) for p in (*record_files, *test_result_files, *gate_doc_files)), *legacy_context["evidence_refs"]],
        legacy_context=legacy_context,
    )
    state_path = write_project_state(project_paths.workspace_root, state, dry_run=False)
    print(f"- project_state: {state_path}")
    context_brief_args = build_context_brief_args(
        project_paths,
        selected,
        req_id,
        req_ctx.title if req_ctx else "",
        final_task_status,
        resolved_mode,
        (args.summary or f"{selected.task_id} 执行回合已{final_task_status}") + f"；{legacy_context['summary']}",
        [*(str(p) for p in (*record_files, *test_result_files, *gate_doc_files)), *legacy_context["evidence_refs"]],
        changed_files_for_brief,
        verification_lines,
        blockers,
        next_steps,
    )
    code, output = run_helper("update_context_brief.py", context_brief_args, dry_run=False)
    if output:
        print(output)
    if code != 0:
        return code

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
