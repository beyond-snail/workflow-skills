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
            "background": ["项目背景", "核心问题"],
            "goal": ["建设目标", "项目范围"],
            "current_state": ["术语与口径定义", "库存单价口径", "数据口径约束"],
            "solution": ["目标解决方案概述", "功能架构图", "总体处理顺序"],
            "dependencies": ["前置业务改造", "应收单明细扩展", "订单销售成本汇总表", "销售成本结转表", "批处理拆分建议"],
            "risks": ["已知缺陷与风险", "异常场景与处理策略", "主要风险", "性能要求"],
            "scenarios": ["典型使用场景", "重点测试场景"],
            "principles": ["核心原则", "结转优先级"],
            "tables": ["库存单价口径", "订单销售成本汇总表", "销售成本结转表"],
            "function_list": ["功能清单"],
            "acceptance": ["功能验收"],
            "test_cases": ["重点测试场景"],
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
            "acceptance_aliases": {
                "F001": ["订单号", "库存单价", "单价校验"],
                "F002": ["应收单明细扩展", "ddno", "应收单明细"],
                "F003": ["汇总表生成", "订单销售成本汇总表", "平均库存单价"],
                "F004": ["销售成本结转表", "结转表"],
                "F006": ["发票类结转", "红字发票冲减", "红字发票"],
                "F007": ["回款类结转", "回款一次性结转", "回款"],
                "F008": ["特殊来源结转", "样书", "捐赠", "配赠", "补数发货", "铺货核减"],
                "F009": ["月结参数", "状态反馈", "关账"],
                "F010": ["月结回退", "回退"],
                "F011": ["报表统计", "统计分析", "报表"],
                "F012": ["报废订单", "报废订单成本结转"],
            },
            "acceptance_items": {
                "F002": ["应收单明细扩展"],
                "F003": ["汇总表生成", "平均库存单价计算"],
                "F006": ["红字发票冲减", "发票类结转"],
                "F007": ["回款类结转"],
                "F008": ["特殊来源结转"],
                "F009": ["月结参数"],
                "F010": ["月结回退"],
                "F011": ["报表统计"],
                "F012": ["报废订单成本结转"],
            },
            "test_case_keywords": {
                "F001": ["订单号缺失", "库存单价缺失"],
                "F006": ["红字发票", "跨月冲减"],
                "F007": ["全部回款", "一次性结转"],
                "F008": ["铺货核减", "混合异动"],
            },
            "acceptance_fallback": {
                "F001": ["订单号缺失和库存单价缺失场景均需阻断并输出异常清单"],
                "F004": ["结转表需完整写入期间、订单、商品、来源、数量、金额与来源单据"],
                "F009": ["月结执行时需展示成功/失败状态，并明确失败节点"],
                "F010": ["回退后需可重新执行当期月结，且累计字段重算一致"],
                "F011": ["报表需支持期间、订单和来源类型维度查询与导出"],
            },
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
