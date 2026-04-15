#!/usr/bin/env python3
"""Load project-profile.yml for workflow-requirement scripts.

Localized copy to keep the skill self-contained without relying on _shared/.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:
    yaml = None  # type: ignore[assignment]


_DEFAULTS: dict[str, Any] = {
    "version": "1.0",
    "project": {"name": "", "language": "", "build_tool": ""},
    "paths": {
        "docs_root": "docs/workflow",
        "requirements_pool": "docs/workflow/requirements/需求池.md",
        "task_board": "docs/workflow/requirements/任务看板.md",
        "prd_directory": "docs/workflow/PRD",
        "readme_index": "README.md",
        "tasks_index": ".ai/memory/tasks/index.md",
        "tasks_template_dir": ".ai/memory/tasks/_template",
        "knowledge_dir": ".ai/memory/knowledge",
    },
    "build": {"compile": "", "test": "", "settings_file": ""},
    "features": {
        "database": True,
        "mcp_database": True,
        "private_registry": False,
        "lightweight_mode": False,
    },
    "code_style": {"rules": []},
    "commit": {
        "format": "{type}(task): {task_id} {task_title}\n\nRefs: {req_id}",
        "types": ["feat", "fix", "docs", "refactor", "test", "chore"],
    },
    "prd_parsing": {
        "section_titles": {
            "background": ["背景与目标", "项目背景", "背景", "背景说明"],
            "goal": ["背景与目标", "产品目标", "目标", "建设目标", "项目范围", "范围定义"],
            "current_state": ["当前代码与数据基线", "当前事实基线", "现状分析", "现状", "当前系统能力"],
            "solution": ["功能概览", "详细需求", "功能详情", "方案设计", "设计方案"],
            "dependencies": ["项目范围", "范围定义", "依赖与前置", "跨需求依赖", "范围边界"],
            "risks": ["风险与待确认", "风险与依赖", "风险", "待确认事项"],
            "scenarios": ["用户与场景", "关键场景", "用户故事", "用户分析", "重点测试场景"],
            "principles": ["核心原则", "设计原则", "原则"],
            "tables": ["数据建议", "数据模型建议", "表结构设计", "当前缺失模型"],
            "function_list": ["功能清单", "功能概览", "功能列表", "功能模块"],
            "acceptance": ["验收标准", "验收口径", "功能验收"],
            "test_cases": ["关键场景", "用户故事", "测试场景", "重点测试场景"],
        },
        "table_columns": {
            "function_list": {
                "code": ["编号", "功能编号", "ID", "Code"],
                "module": ["模块", "功能模块", "Module"],
                "name": ["功能名称", "功能", "Feature", "Name"],
                "description": ["描述", "说明", "Description"],
                "priority": ["优先级", "Priority"],
            },
            "acceptance": {
                "item": ["验收项", "Acceptance Item", "Item"],
                "standard": ["验收标准", "Acceptance Criteria", "Criteria"],
            },
        },
        "feature_rules": {
            "acceptance_aliases": {},
            "acceptance_items": {},
            "test_case_keywords": {},
            "acceptance_fallback": {},
        },
    },
}


def _deep_merge(base: dict, override: dict) -> dict:
    result = dict(base)
    for key, val in override.items():
        if key in result and isinstance(result[key], dict) and isinstance(val, dict):
            result[key] = _deep_merge(result[key], val)
        else:
            result[key] = val
    return result


def _parse_yaml_simple(text: str) -> dict[str, Any]:
    records: list[tuple[int, str]] = []
    for raw_line in text.splitlines():
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        indent = len(raw_line) - len(raw_line.lstrip())
        records.append((indent, stripped))

    def parse_block(start: int, indent: int) -> tuple[Any, int]:
        if start >= len(records):
            return {}, start
        if records[start][1].startswith("- "):
            return parse_list(start, indent)
        return parse_mapping(start, indent)

    def parse_mapping(start: int, indent: int) -> tuple[dict[str, Any], int]:
        result: dict[str, Any] = {}
        idx = start
        while idx < len(records):
            current_indent, stripped = records[idx]
            if current_indent < indent:
                break
            if current_indent > indent:
                idx += 1
                continue
            if stripped.startswith("- ") or ":" not in stripped:
                break

            key, _, value = stripped.partition(":")
            key = key.strip()
            value = value.strip().strip('"').strip("'")

            if value:
                result[key] = _coerce(value)
                idx += 1
                continue

            next_idx = idx + 1
            if next_idx >= len(records) or records[next_idx][0] <= current_indent:
                result[key] = {}
                idx = next_idx
                continue

            child, idx = parse_block(next_idx, records[next_idx][0])
            result[key] = child
        return result, idx

    def parse_list(start: int, indent: int) -> tuple[list[Any], int]:
        result: list[Any] = []
        idx = start
        while idx < len(records):
            current_indent, stripped = records[idx]
            if current_indent < indent:
                break
            if current_indent != indent or not stripped.startswith("- "):
                break

            value = stripped[2:].strip().strip('"').strip("'")
            if value:
                result.append(_coerce(value))
                idx += 1
                continue

            next_idx = idx + 1
            if next_idx >= len(records) or records[next_idx][0] <= current_indent:
                result.append(None)
                idx = next_idx
                continue

            child, idx = parse_block(next_idx, records[next_idx][0])
            result.append(child)
        return result, idx

    parsed, _ = parse_mapping(0, records[0][0] if records else 0)
    return parsed


def _coerce(value: str) -> Any:
    if value.lower() in ("true", "yes"):
        return True
    if value.lower() in ("false", "no"):
        return False
    if value.lower() in ("null", "~", ""):
        return None
    try:
        return int(value)
    except ValueError:
        pass
    try:
        return float(value)
    except ValueError:
        pass
    return value


def _find_profile_path(start: Path | None = None) -> Path | None:
    if start is None:
        start = Path.cwd()
    if start:
        for candidate in (
            start / ".ai/runtime/profile/project-profile.yml",
            start / "project-profile.yml",
        ):
            if candidate.is_file():
                return candidate

    skills_root = Path(__file__).resolve().parent.parent
    candidate = skills_root / "project-profile.yml"
    return candidate if candidate.is_file() else None


def get_profile(profile_path: str | Path | None = None) -> dict[str, Any]:
    path = Path(profile_path) if profile_path else _find_profile_path()

    if path is None or not path.is_file():
        return dict(_DEFAULTS)

    text = path.read_text(encoding="utf-8")

    if yaml is not None:
        loaded = yaml.safe_load(text) or {}
    else:
        loaded = _parse_yaml_simple(text)

    return _deep_merge(_DEFAULTS, loaded)
