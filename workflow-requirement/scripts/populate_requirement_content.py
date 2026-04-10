#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import find_requirement_row, find_section_heading, get_cell
from profile_paths import ProjectPaths

from sync_requirement_pool import sync_requirement_pool_entry
from sync_task_board import sync_task_board_entry


HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
LIST_ITEM_RE = re.compile(r"^\s*(?:[-*]|\d+\.)\s+(.*)$")
TREE_TOP_RE = re.compile(r"^[├└]──\s*(.+?)\s*$")
TREE_CHILD_RE = re.compile(r"^[│\s]+[├└]──\s*(.+?)\s*$")
BACKTICK_RE = re.compile(r"`([^`]+)`")
BOOK_RE = re.compile(r"《([^》]+)》")
PLACEHOLDER_TEXTS = {"待补充", "待补充。"}
NUMERIC_TITLE_RE = re.compile(r"^\d+(?:\.\d+)*\s+")


@dataclass
class Section:
    level: int
    title: str
    lines: list[str]

    @property
    def text(self) -> str:
        return "\n".join(self.lines).strip()


@dataclass
class GeneratedTask:
    title: str
    acceptance: str
    doc_link: str


@dataclass
class FunctionItem:
    code: str
    module: str
    name: str
    description: str
    priority: str
    detail_points: list[str]
    acceptance_points: list[str]


DEFAULT_FEATURE_ACCEPTANCE_ALIASES: dict[str, list[str]] = {
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
}

DEFAULT_FEATURE_ACCEPTANCE_ITEMS: dict[str, list[str]] = {
    "F002": ["应收单明细扩展"],
    "F003": ["汇总表生成", "平均库存单价计算"],
    "F006": ["红字发票冲减", "发票类结转"],
    "F007": ["回款类结转"],
    "F008": ["特殊来源结转"],
    "F009": ["月结参数"],
    "F010": ["月结回退"],
    "F011": ["报表统计"],
    "F012": ["报废订单成本结转"],
}

DEFAULT_FEATURE_TEST_CASE_KEYWORDS: dict[str, list[str]] = {
    "F001": ["订单号缺失", "库存单价缺失"],
    "F006": ["红字发票", "跨月冲减"],
    "F007": ["全部回款", "一次性结转"],
    "F008": ["铺货核减", "混合异动"],
}

DEFAULT_FEATURE_ACCEPTANCE_FALLBACK: dict[str, list[str]] = {
    "F001": ["订单号缺失和库存单价缺失场景均需阻断并输出异常清单"],
    "F004": ["结转表需完整写入期间、订单、商品、来源、数量、金额与来源单据"],
    "F009": ["月结执行时需展示成功/失败状态，并明确失败节点"],
    "F010": ["回退后需可重新执行当期月结，且累计字段重算一致"],
    "F011": ["报表需支持期间、订单和来源类型维度查询与导出"],
}

DEFAULT_SECTION_TITLES: dict[str, tuple[str, ...]] = {
    "background": ("项目背景", "核心问题"),
    "goal": ("建设目标", "项目范围"),
    "current_state": ("术语与口径定义", "库存单价口径", "数据口径约束"),
    "solution": ("目标解决方案概述", "功能架构图", "总体处理顺序"),
    "dependencies": ("前置业务改造", "应收单明细扩展", "订单销售成本汇总表", "销售成本结转表", "批处理拆分建议"),
    "risks": ("已知缺陷与风险", "异常场景与处理策略", "主要风险", "性能要求"),
    "scenarios": ("典型使用场景", "重点测试场景"),
    "principles": ("核心原则", "结转优先级"),
    "tables": ("库存单价口径", "订单销售成本汇总表", "销售成本结转表"),
    "function_list": ("功能清单",),
    "acceptance": ("功能验收",),
    "test_cases": ("重点测试场景",),
    "objects": ("术语与口径定义", "订单销售成本汇总表", "销售成本结转表", "应收单明细扩展"),
}

DEFAULT_TABLE_COLUMN_ALIASES: dict[str, dict[str, tuple[str, ...]]] = {
    "function_list": {
        "code": ("编号", "功能编号", "ID", "Code"),
        "module": ("模块", "功能模块", "Module"),
        "name": ("功能名称", "功能", "Feature", "Name"),
        "description": ("描述", "说明", "Description"),
        "priority": ("优先级", "Priority"),
    },
    "acceptance": {
        "item": ("验收项", "Acceptance Item", "Item"),
        "standard": ("验收标准", "Acceptance Criteria", "Criteria"),
    },
}


def parse_sections(text: str) -> list[Section]:
    sections: list[Section] = []
    current: Section | None = None
    in_code_block = False

    for raw_line in text.splitlines():
        line = raw_line.rstrip()
        stripped = line.strip()
        if stripped.startswith("```"):
            in_code_block = not in_code_block
        heading = None if in_code_block else HEADING_RE.match(stripped)
        if heading:
            if current is not None:
                sections.append(current)
            current = Section(level=len(heading.group(1)), title=heading.group(2).strip(), lines=[])
            continue
        if current is not None:
            current.lines.append(line)

    if current is not None:
        sections.append(current)
    return sections


def uniq(values: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for value in values:
        item = value.strip()
        if not item or item in seen:
            continue
        seen.add(item)
        result.append(item)
    return result


def normalized_title(title: str) -> str:
    return NUMERIC_TITLE_RE.sub("", title.strip())


def normalize_header(text: str) -> str:
    return text.strip().lower().replace(" ", "")


def tuple_from_config(value: Any, default: tuple[str, ...]) -> tuple[str, ...]:
    if isinstance(value, list):
        items = [str(item).strip() for item in value if str(item).strip()]
        return tuple(items) if items else default
    if isinstance(value, str) and value.strip():
        return (value.strip(),)
    return default


def dict_of_list_from_config(value: Any, default: dict[str, list[str]]) -> dict[str, list[str]]:
    if not isinstance(value, dict):
        return default
    result: dict[str, list[str]] = {key: list(values) for key, values in default.items()}
    for key, raw in value.items():
        if isinstance(raw, list):
            result[str(key)] = [str(item).strip() for item in raw if str(item).strip()]
        elif isinstance(raw, str) and raw.strip():
            result[str(key)] = [raw.strip()]
    return result


def get_section_title_map(profile: dict[str, Any]) -> dict[str, tuple[str, ...]]:
    raw = profile.get("prd_parsing", {}).get("section_titles", {})
    result: dict[str, tuple[str, ...]] = {}
    for key, default in DEFAULT_SECTION_TITLES.items():
        result[key] = tuple_from_config(raw.get(key), default)
    return result


def get_table_column_aliases(profile: dict[str, Any]) -> dict[str, dict[str, tuple[str, ...]]]:
    raw = profile.get("prd_parsing", {}).get("table_columns", {})
    result: dict[str, dict[str, tuple[str, ...]]] = {}
    for table_key, field_defaults in DEFAULT_TABLE_COLUMN_ALIASES.items():
        table_cfg = raw.get(table_key, {}) if isinstance(raw, dict) else {}
        result[table_key] = {}
        for field_key, default in field_defaults.items():
            cfg_value = table_cfg.get(field_key) if isinstance(table_cfg, dict) else None
            result[table_key][field_key] = tuple_from_config(cfg_value, default)
    return result


def get_feature_rule_maps(profile: dict[str, Any]) -> tuple[dict[str, list[str]], dict[str, list[str]], dict[str, list[str]], dict[str, list[str]]]:
    rules = profile.get("prd_parsing", {}).get("feature_rules", {})
    acceptance_aliases = dict_of_list_from_config(rules.get("acceptance_aliases"), DEFAULT_FEATURE_ACCEPTANCE_ALIASES)
    acceptance_items = dict_of_list_from_config(rules.get("acceptance_items"), DEFAULT_FEATURE_ACCEPTANCE_ITEMS)
    test_case_keywords = dict_of_list_from_config(rules.get("test_case_keywords"), DEFAULT_FEATURE_TEST_CASE_KEYWORDS)
    acceptance_fallback = dict_of_list_from_config(rules.get("acceptance_fallback"), DEFAULT_FEATURE_ACCEPTANCE_FALLBACK)
    return acceptance_aliases, acceptance_items, test_case_keywords, acceptance_fallback


def section_matches(section: Section, keywords: tuple[str, ...]) -> bool:
    title = section.title.replace(" ", "")
    return any(keyword.replace(" ", "") in title for keyword in keywords)


def find_sections(sections: list[Section], keywords: tuple[str, ...]) -> list[Section]:
    return [section for section in sections if section_matches(section, keywords)]


def find_section_by_titles(sections: list[Section], titles: tuple[str, ...]) -> list[Section]:
    normalized_targets = {title.strip() for title in titles}
    return [section for section in sections if normalized_title(section.title) in normalized_targets]


def find_feature_sections(sections: list[Section], feature_code: str) -> list[Section]:
    feature_code = feature_code.strip().upper()
    matched: list[Section] = []
    for section in sections:
        title = normalized_title(section.title).upper()
        if title.startswith(feature_code):
            matched.append(section)
    return matched


def list_items_from_sections(sections: list[Section], limit: int = 8) -> list[str]:
    items: list[str] = []
    for section in sections:
        for line in section.lines:
            match = LIST_ITEM_RE.match(line.strip())
            if match:
                items.append(match.group(1).strip())
    return uniq(items)[:limit]


def summary_points(sections: list[Section], limit: int = 6) -> list[str]:
    items = list_items_from_sections(sections, limit=limit)
    if items:
        return items[:limit]

    points: list[str] = []
    for section in sections:
        for line in section.lines:
            stripped = line.strip()
            if not stripped or stripped.startswith("|") or stripped.startswith("```"):
                continue
            points.append(stripped)
            if len(points) >= limit:
                return uniq(points)
    return uniq(points)


def compact_point(text: str, max_len: int = 48) -> str:
    cleaned = re.sub(r"\s+", " ", text.strip().strip("-").replace("`", ""))
    if len(cleaned) <= max_len:
        return cleaned
    for sep in ("。", "；", "，", "：", ":"):
        idx = cleaned.find(sep)
        if 0 < idx <= max_len:
            return cleaned[:idx].strip()
    return cleaned[:max_len].rstrip("，；：: ") + "..."


def extract_feature_detail_points(detail_sections: list[Section], limit: int = 4) -> list[str]:
    points: list[str] = []
    in_code_block = False

    for section in detail_sections:
        for raw_line in section.lines:
            stripped = raw_line.strip()
            if stripped.startswith("```"):
                in_code_block = not in_code_block
                continue
            if in_code_block or not stripped:
                continue
            if stripped.startswith("|") or stripped.startswith("!["):
                continue
            if stripped.startswith("**") and stripped.endswith("**"):
                continue
            if stripped.endswith("：") or stripped.endswith(":"):
                continue
            if stripped.startswith("```"):
                continue

            match = LIST_ITEM_RE.match(stripped)
            candidate = match.group(1).strip() if match else stripped
            if any(keyword in candidate for keyword in ("线框图", "计算公式", "回写规则")):
                continue
            if candidate.startswith(("`", "1.", "2.", "3.", "4.")) and "=" in candidate:
                continue
            compacted = compact_point(candidate)
            if compacted:
                points.append(compacted)

    return uniq(points)[:limit]


def render_bullets(items: list[str], fallback: str) -> str:
    if not items:
        return f"- {fallback}"
    return "\n".join(f"- {item}" for item in items)


def render_sections(sections: list[Section], fallback: str, max_sections: int = 3) -> str:
    meaningful_sections = [section for section in sections if section.text and section.text not in PLACEHOLDER_TEXTS]
    if not meaningful_sections:
        return fallback

    blocks: list[str] = []
    for section in meaningful_sections[:max_sections]:
        body = section.text
        blocks.append(f"### {section.title}\n\n{body}")
    return "\n\n".join(blocks)


def parse_first_table(section: Section | None) -> list[dict[str, str]]:
    if section is None:
        return []

    lines = [line.strip() for line in section.lines]
    for idx in range(len(lines) - 1):
        if not lines[idx].startswith("|") or not lines[idx + 1].startswith("|"):
            continue
        header_cells = [cell.strip() for cell in lines[idx].strip("|").split("|")]
        separator = lines[idx + 1].replace("|", "").replace("-", "").replace(":", "").strip()
        if separator:
            continue
        rows: list[dict[str, str]] = []
        row_idx = idx + 2
        while row_idx < len(lines) and lines[row_idx].startswith("|"):
            row_cells = [cell.strip() for cell in lines[row_idx].strip("|").split("|")]
            if len(row_cells) < len(header_cells):
                row_cells.extend([""] * (len(header_cells) - len(row_cells)))
            rows.append({header_cells[col]: row_cells[col] for col in range(len(header_cells))})
            row_idx += 1
        return rows
    return []


def section_has_table_columns(section: Section | None, aliases: dict[str, tuple[str, ...]]) -> bool:
    rows = parse_first_table(section)
    if not rows:
        return False
    header_keys = {normalize_header(key) for key in rows[0].keys()}
    for field_aliases in aliases.values():
        if not any(normalize_header(alias) in header_keys for alias in field_aliases):
            return False
    return True


def find_section_by_table_columns(sections: list[Section], aliases: dict[str, tuple[str, ...]]) -> Section | None:
    for section in sections:
        if section_has_table_columns(section, aliases):
            return section
    return None


def get_row_value(row: dict[str, str], aliases: tuple[str, ...]) -> str:
    normalized_aliases = {normalize_header(alias) for alias in aliases}
    for key, value in row.items():
        if normalize_header(key) in normalized_aliases:
            return value.strip()
    return ""


def extract_architecture(sections: list[Section]) -> tuple[list[str], dict[str, list[str]]]:
    arch_sections = find_sections(sections, ("功能架构图", "功能架构"))
    top_modules: list[str] = []
    child_map: dict[str, list[str]] = {}

    for section in arch_sections:
        current_top: str | None = None
        for raw_line in section.lines:
            line = raw_line.rstrip()
            top_match = TREE_TOP_RE.match(line)
            child_match = TREE_CHILD_RE.match(line)
            if top_match:
                current_top = top_match.group(1).strip()
                top_modules.append(current_top)
                child_map.setdefault(current_top, [])
                continue
            if child_match and current_top:
                child_map.setdefault(current_top, []).append(child_match.group(1).strip())

    top_modules = uniq(top_modules)
    for key, values in list(child_map.items()):
        child_map[key] = uniq(values)

    if top_modules:
        return top_modules, child_map

    fallback_titles = [
        section.title
        for section in sections
        if section.level >= 3 and any(keyword in section.title for keyword in ("改造", "汇总", "结转", "集成", "报表", "回退", "追溯"))
    ]
    top_modules = uniq(fallback_titles)[:5]
    return top_modules, {module: [] for module in top_modules}


def extract_object_names(sections: list[Section], limit: int = 8) -> list[str]:
    objects: list[str] = []
    for section in sections:
        objects.extend(BOOK_RE.findall(section.text))
        objects.extend(BACKTICK_RE.findall(section.text))
    filtered = [value for value in uniq(objects) if "md" not in value.lower()]
    return filtered[:limit]


def extract_function_items(
    sections: list[Section],
    function_section: Section | None,
    function_columns: dict[str, tuple[str, ...]],
) -> list[FunctionItem]:
    rows = parse_first_table(function_section)
    items: list[FunctionItem] = []
    for row in rows:
        code = get_row_value(row, function_columns["code"]).strip()
        if not code.startswith("F"):
            continue
        detail_sections = find_feature_sections(sections, code)
        items.append(
            FunctionItem(
                code=code,
                module=get_row_value(row, function_columns["module"]).strip() or "待补充模块",
                name=get_row_value(row, function_columns["name"]).strip() or code,
                description=get_row_value(row, function_columns["description"]).strip() or "待补充功能描述",
                priority=get_row_value(row, function_columns["priority"]).strip() or "待定",
                detail_points=extract_feature_detail_points(detail_sections, limit=4),
                acceptance_points=[],
            )
        )
    return items


def resolve_acceptance_points(
    item: FunctionItem,
    acceptance_rows: list[dict[str, str]],
    test_cases: list[str],
    acceptance_columns: dict[str, tuple[str, ...]],
    acceptance_items_map: dict[str, list[str]],
    test_case_keyword_map: dict[str, list[str]],
    acceptance_alias_map: dict[str, list[str]],
    acceptance_fallback_map: dict[str, list[str]],
) -> list[str]:
    matched: list[str] = []
    expected_items = acceptance_items_map.get(item.code, [])
    case_keywords = test_case_keyword_map.get(item.code, []) + acceptance_alias_map.get(item.code, [])

    if expected_items:
        for row in acceptance_rows:
            acceptance_item = get_row_value(row, acceptance_columns["item"]).strip()
            acceptance_standard = get_row_value(row, acceptance_columns["standard"]).strip()
            if acceptance_item in expected_items and acceptance_standard:
                matched.append(f"{acceptance_item} {acceptance_standard}")

    for case in test_cases:
        if any(keyword and keyword in case for keyword in case_keywords):
            matched.append(case)

    if not matched:
        matched.extend(acceptance_fallback_map.get(item.code, []))

    return uniq(matched)[:3]


def find_doc(design_dir: Path, keyword: str) -> Path | None:
    matches = sorted(design_dir.glob(f"*{keyword}*.md"))
    return matches[0] if matches else None


def find_testing_doc(testing_dir: Path, keyword: str) -> Path | None:
    matches = sorted(testing_dir.glob(f"*{keyword}*.md"))
    return matches[0] if matches else None


def md_link(path: Path, workspace_root: Path) -> str:
    rel = path.relative_to(workspace_root).as_posix()
    return f"[{rel}]({rel})"


def write_text(path: Path, content: str, dry_run: bool) -> None:
    print(f"- write: {path}")
    if dry_run:
        return
    path.write_text(content.rstrip() + "\n", encoding="utf-8")


def update_requirement_section(
    req_file: Path,
    req_id: str,
    background_items: list[str],
    goal_items: list[str],
    doc_links: list[str],
    dry_run: bool,
) -> None:
    if not req_file.exists():
        return

    lines = req_file.read_text(encoding="utf-8").splitlines()
    heading = f"## {req_id}"
    start = find_section_heading(lines, heading)
    if start is None:
        return

    end = len(lines)
    for idx in range(start + 1, len(lines)):
        if lines[idx].startswith("## "):
            end = idx
            break

    block = [
        heading,
        "",
        "### 背景",
        "",
        *(render_bullets(background_items, "待补充。").splitlines()),
        "",
        "### 目标",
        "",
        *(render_bullets(goal_items, "待补充。").splitlines()),
        "",
        "### 关联文档",
        "",
    ]
    block.extend(f"{index}. {link}" for index, link in enumerate(doc_links, start=1))

    if dry_run:
        print("- requirement-section: would update summary block")
        return

    lines[start:end] = block + [""]
    req_file.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def build_tech_design(
    doc_date: str,
    theme: str,
    prd_rel: str,
    req_id: str,
    background_sections: list[Section],
    goal_sections: list[Section],
    current_sections: list[Section],
    solution_sections: list[Section],
    dependency_sections: list[Section],
    risk_sections: list[Section],
    principles: list[str],
    use_cases: list[str],
    top_modules: list[str],
) -> str:
    return f"""# {theme} - 技术设计文档

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |
| 需求ID | {req_id} |
| PRD来源 | `{prd_rel}` |

## 2. 背景与问题

{render_sections(background_sections, "待结合 PRD 补充背景与问题说明。")}

## 3. 目标

{render_sections(goal_sections, "待结合 PRD 补充建设目标。")}

### 核心原则

{render_bullets(principles, "待补充核心原则。")}

## 4. 现状分析

{render_sections(current_sections, "待结合 PRD 补充现状分析。")}

## 5. 方案设计

{render_sections(solution_sections, "待结合 PRD 补充方案设计。")}

### 模块拆分

{render_bullets(top_modules, "待补充模块拆分。")}

## 6. 系统改造点

{render_sections(dependency_sections, "待结合 PRD 补充系统改造点。")}

## 7. 验收口径

### 核心验收场景

{render_bullets(use_cases, "待补充验收场景。")}

## 8. 风险与依赖

{render_sections(risk_sections, "待结合 PRD 补充风险与依赖。")}
"""


def build_detailed_design(
    doc_date: str,
    theme: str,
    prd_rel: str,
    top_modules: list[str],
    child_map: dict[str, list[str]],
    function_items: list[FunctionItem],
    objects: list[str],
    dependency_sections: list[Section],
    risk_sections: list[Section],
    use_cases: list[str],
) -> str:
    module_rows = ["| 模块 | 职责 | 关键子项 |", "| --- | --- | --- |"]
    if function_items:
        module_map: dict[str, list[str]] = {}
        for item in function_items:
            module_map.setdefault(item.module, []).append(f"{item.code} {item.name}")
        for module, features in list(module_map.items())[:8]:
            children = "；".join(features[:4])
            module_rows.append(f"| {module} | 承接 `{prd_rel}` 中对应模块的实现与联调 | {children} |")
    else:
        for module in top_modules[:6]:
            children = "；".join(child_map.get(module, [])[:4]) or "按 PRD 细化"
            module_rows.append(f"| {module} | 承接 `{prd_rel}` 中对应模块的实现与联调 | {children} |")

    object_bullets = render_bullets(objects, "待补充关键业务对象。")
    feature_bullets = [f"{item.code} {item.name}：{item.description}" for item in function_items[:8]]
    return f"""# {doc_date} 详细开发设计 - {theme}

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |
| PRD来源 | `{prd_rel}` |

## 2. 模块拆分

{chr(10).join(module_rows)}

## 3. 数据流与时序

1. 根据 PRD 准备前置主数据与依赖数据。
2. 执行订单/商品维度的汇总或重算逻辑。
3. 触发本期成本结转、红冲/回退等处理。
4. 生成追溯结果、异常提示与统计输出。

## 4. 核心对象设计

{object_bullets}

## 5. 接口与方法设计

### 预期入口

{render_bullets(feature_bullets or top_modules, "待补充接口与方法。")}

## 6. SQL 与数据落库设计

{render_sections(dependency_sections, "待结合 PRD 补充数据落库设计。", max_sections=2)}

## 7. 异常处理与回退策略

{render_sections(risk_sections, "待结合 PRD 补充异常与回退策略。", max_sections=2)}

## 8. 测试与验证设计

{render_bullets(use_cases, "待补充测试与验证设计。")}
"""


def build_breakdown(
    doc_date: str,
    theme: str,
    design_rel: str,
    detailed_design_rel: str,
    prd_rel: str,
    function_items: list[FunctionItem],
    top_modules: list[str],
    child_map: dict[str, list[str]],
    use_cases: list[str],
) -> str:
    tasks: list[str] = []
    source_items = function_items or [
        FunctionItem(code=f"T{index:03d}", module=module, name=module, description=module, priority="P0", detail_points=child_map.get(module, []), acceptance_points=[])
        for index, module in enumerate((top_modules[:4] or ["基础数据改造", "核心业务实现", "月结与回退", "测试与验收"]), start=1)
    ]
    for index, item in enumerate(source_items, start=1):
        child_text = "；".join(item.detail_points[:4]) or item.description
        acceptance_points = item.acceptance_points or ([use_cases[index - 1]] if index - 1 < len(use_cases) else [f"完成 {item.name} 对应能力并可验证"])
        tasks.extend(
            [
                f"### 3.{index} {item.code} {item.name}",
                "",
                "目标：",
                f"- 依据 `{prd_rel}` 落实 `{item.code} {item.name}` 对应能力。",
                "",
                "改动范围：",
                f"- 设计依据：`{design_rel}`",
                f"- 详细设计：`{detailed_design_rel}`",
                f"- 模块：{item.module}",
                f"- 关键子项：{child_text}",
                "",
                "验收重点：",
                *[f"- {point}" for point in acceptance_points],
                f"- 对齐 PRD 描述：{item.description}",
                "",
            ]
        )

    tasks.extend(
        [
            f"### 3.{len(source_items) + 1} 联调与测试验证",
            "",
            "目标：",
            "- 按 UAT/联调计划完成关键场景验证，补齐自动化和手工证据。",
            "",
            "改动范围：",
            "- 联调验收记录",
            "- 测试结果",
            "- UAT 测试用例",
            "",
            "验收重点：",
            f"- {use_cases[-1] if use_cases else '关键业务场景已覆盖'}",
            "",
        ]
    )

    return f"""# {doc_date} 开发任务拆解 - {theme}

## 1. 对应需求

- PRD：`{prd_rel}`
- 技术设计：`{design_rel}`
- 详细开发设计：`{detailed_design_rel}`

## 2. 总体原则

1. 业务语义以 PRD 为准，不用历史实现反推需求。
2. 每条任务只对应一个明确交付物或 PRD 缺口。
3. 任务完成后必须同步更新追溯、测试或联调文档。

## 3. 任务拆解

{chr(10).join(tasks)}

## 4. 推荐执行顺序

1. 先完成前置数据改造和依赖校验。
2. 再实现核心汇总/结转能力。
3. 最后补齐月结集成、异常回退、报表与测试验证。
"""


def build_physical_design(doc_date: str, theme: str, objects: list[str], table_names: list[str]) -> str:
    existing_rows = "\n".join(f"| {table} | 待补充 | 待补充 | 待补充 | 需结合现有模型确认 |" for table in table_names[:6]) or "| 待补充 | 待补充 | 待补充 | 待补充 | 待补充 |"
    object_notes = render_bullets(objects, "待补充核心表或对象。")
    return f"""# {doc_date} 物理表设计 - {theme}

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |

## 2. 现有表扩展

| 表名 | 字段 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
{existing_rows}

## 3. 新增表设计

{object_notes}

## 4. 唯一键与索引

- 需根据订单 + 商品维度的唯一约束补充索引设计。
- 需根据月结批处理与回退场景评估期间、来源类型等辅助索引。

## 5. 约束与备注

- 正文为基于 PRD 的首轮设计草稿，落库前需结合现有数据库模型复核。
"""


def build_table_mapping(doc_date: str, theme: str, objects: list[str], table_names: list[str]) -> str:
    rows = ["| PRD对象/术语 | 现有表/目标表 | 备注 |", "| --- | --- | --- |"]
    candidates = objects[:6] or ["订单", "商品编码", "销售成本结转", "订单销售成本汇总"]
    for index, name in enumerate(candidates):
        table = table_names[index] if index < len(table_names) else "待补充"
        rows.append(f"| {name} | {table} | 需结合现有库表进一步确认映射关系 |")
    return f"""# {doc_date} 表名对照表 - {theme}

## 1. 设计原则

- 优先把 PRD 术语映射到现有表或目标表。
- 不明确的映射先记录为待确认项，避免直接拍脑袋定库表。

## 2. 对照关系

{chr(10).join(rows)}
"""


def build_prd_trace(doc_date: str, theme: str, prd_rel: str, design_rel: str, detailed_design_rel: str, breakdown_rel: str, uat_rel: str, top_modules: list[str], child_map: dict[str, list[str]]) -> str:
    rows = ["| PRD章节 | PRD需求描述 | 设计/代码/测试对应位置 | 完成状态 | 备注 |", "|----------|-------------|------------------------|----------|------|"]
    modules = top_modules[:6] or ["基础数据改造", "核心业务实现", "月结集成"]
    for module in modules:
        desc = "；".join(child_map.get(module, [])[:4]) or module
        mapping = f"`{design_rel}`；`{detailed_design_rel}`；`{breakdown_rel}`；`{uat_rel}`"
        rows.append(f"| {module} | {desc} | {mapping} | 待实现 | 代码位置待实现后回填 |")
    return f"""# {doc_date} PRD追溯 - {theme}

## 文档信息

| 项目 | 内容 |
|------|------|
| 需求主题 | {theme} |
| 对应PRD | `{prd_rel}` |
| 创建日期 | {doc_date} |

---

## 需求追溯矩阵

{chr(10).join(rows)}

---

## 未实现需求说明

| PRD章节 | PRD需求描述 | 未实现原因 | 计划处理时间 |
|----------|-------------|------------|--------------|
| 待实现 | 待根据开发推进补充 | 当前为开发前准备稿 | 待排期 |

---

## 确认结论

- [x] 已建立 `PRD` 到设计/任务/测试文档的追溯关系
- [ ] 代码与自动化测试位置待实现后继续回填
- [ ] 仍需要按追溯清单继续补代码与测试证据
"""


def build_product_confirmation(doc_date: str, theme: str, prd_rel: str, confirmation_items: list[str]) -> str:
    items = confirmation_items[:5] or ["需确认特殊来源场景的处理边界。", "需确认阻塞场景下的业务处理口径。"]
    detail_lines: list[str] = []
    result_rows = ["| 编号 | 待确认项 | 产品结论 | 确认人 | 确认日期 |", "|------|----------|----------|--------|----------|"]
    for index, item in enumerate(items, start=1):
        detail_lines.extend(
            [
                f"### {index}. 待确认项{index}",
                "",
                f"说明：{item}",
                "",
                "确认：",
                "",
            ]
        )
        result_rows.append(f"| {index} | {item} |  |  |  |")
    return f"""# {doc_date} 产品确认清单 - {theme}

## 文档信息

| 项目 | 内容 |
|------|------|
| 需求主题 | {theme} |
| 对应PRD | `{prd_rel}` |
| 创建日期 | {doc_date} |
| 用途 | 产品补充确认，消除研发落地歧义 |

---

## 待确认项

{chr(10).join(detail_lines)}
---

## 产品确认结果

{chr(10).join(result_rows)}

---

## 结论

- [ ] 所有关键歧义已补充确认，可进入开发实现
- [ ] 如仍有未确认项，开发前需继续冻结
"""


def build_impl_alignment(doc_date: str, theme: str, prd_rel: str, top_modules: list[str]) -> str:
    rows = ["| PRD编号 | 需求内容 | 代码入口/方法 | 当前状态 | 说明 |", "|---|---|---|---|---|"]
    for module in top_modules[:6] or ["核心模块"]:
        rows.append(f"| {module} | {module} | 待实现 | 待实现 | 当前为开发前准备稿，待编码后回填 |")
    return f"""# {doc_date} 流程图与实现对齐 - {theme}

## 1. 结论

- 业务规则与口径：统一以 `{prd_rel}` 为准
- 核心开发：待实现
- 报表/异常视图/导出：待实现
- 最终业务验收：待执行

## 2. 月结主流程图

```mermaid
flowchart TD
    A[读取PRD与配置] --> B[准备订单与商品基础数据]
    B --> C[生成订单销售成本汇总]
    C --> D[执行销售成本结转]
    D --> E[写入追溯/报表/异常结果]
    E --> F[月结完成或回退]
```

## 3. 方法与需求对齐表

{chr(10).join(rows)}
"""


def build_acceptance_doc(doc_date: str, theme: str, use_cases: list[str]) -> str:
    rows = ["| 场景 | 预期结果 | 实际结果 | 状态 |", "| --- | --- | --- | --- |"]
    for case in use_cases[:6] or ["待补充联调场景"]:
        rows.append(f"| {case} | 系统结果符合 PRD 口径，且可追溯 | 待执行 | 待执行 |")
    return f"""# {doc_date} 联调验收记录 - {theme}

## 1. 目标

- 按 PRD 验证关键业务场景、月结链路和异常回退逻辑。

## 2. 环境信息

- 环境：待补充
- 数据准备：参考 UAT 测试用例

## 3. 执行步骤

1. 按测试用例准备订单、发票、回款等业务数据。
2. 执行对应业务流程或月结步骤。
3. 核对汇总表、结转表、异常提示和统计结果。

## 4. 结果记录

{chr(10).join(rows)}

## 5. 结论

- 当前为联调执行计划稿，待实际联调后继续回写。
"""


def build_test_result_doc(doc_date: str, theme: str, use_cases: list[str]) -> str:
    rows = ["| 检查项 | 预期 | 实际 | 状态 |", "| --- | --- | --- | --- |"]
    for case in use_cases[:6] or ["待补充测试范围"]:
        rows.append(f"| {case} | 自动化/手工验证结果符合 PRD | 待执行 | 待执行 |")
    return f"""# {doc_date} 测试结果 - {theme}

## 1. 目标

- 为后续自动化测试和联调回写预置测试范围。

## 2. 环境信息

- 环境：待补充
- 数据准备：参考 UAT 测试用例和联调验收记录

## 3. 执行步骤

1. 执行构建或编译校验。
2. 执行关键自动化测试或数据校验脚本。
3. 对阻塞场景记录替代验证和剩余待验证点。

## 4. 结果记录

{chr(10).join(rows)}

## 5. 结论

- 当前为测试执行计划稿，实际执行结果由自动回写记录继续补充。
"""


def build_uat_cases(doc_date: str, theme: str, use_cases: list[str]) -> str:
    rows = ["| 用例ID | 场景 | 前置条件 | 执行步骤 | 期望结果 |", "| --- | --- | --- | --- | --- |"]
    cases = use_cases[:8] or ["待补充UAT场景"]
    for index, case in enumerate(cases, start=1):
        rows.append(
            f"| UAT-{index:02d} | {case} | 准备与场景匹配的订单/发票/回款/库存数据 | 执行业务流程并核对结果表、异常与报表 | 系统结果符合 PRD，且结转记录可追溯 |"
        )
    return f"""# {doc_date} UAT测试用例 - {theme}

## 1. 目标

- 用于产品、测试和业务在开发完成后执行场景验收。

## 2. 环境信息

- 环境：待补充
- 版本：待补充

## 3. 执行步骤

1. 按用例准备基础数据和财务期间。
2. 执行对应业务动作或月结流程。
3. 核对汇总表、结转表、异常与报表输出。

## 4. 结果记录

{chr(10).join(rows)}

## 5. 结论

- 用例已基于 PRD 预填，待实际执行后补充结果。
"""


def generated_tasks(
    initial_task_id: str,
    task_file: Path,
    sync_date: str,
    breakdown_link: str,
    function_items: list[FunctionItem],
    top_modules: list[str],
    child_map: dict[str, list[str]],
    use_cases: list[str],
) -> list[tuple[str, GeneratedTask]]:
    feature_items = function_items or [
        FunctionItem(code=f"T{index:03d}", module=module, name=f"{module}实现", description=module, priority="P0", detail_points=child_map.get(module, []), acceptance_points=[])
        for index, module in enumerate((top_modules[:4] or ["基础数据改造", "核心业务实现", "月结与回退"]), start=1)
    ]
    tasks: list[GeneratedTask] = []
    for index, item in enumerate(feature_items, start=1):
        child_text = "；".join(item.detail_points[:3]) or item.description
        acceptance = "；".join(item.acceptance_points[:2]) if item.acceptance_points else (use_cases[index - 1] if index - 1 < len(use_cases) else f"{item.name} 相关能力可按 PRD 验证")
        tasks.append(
            GeneratedTask(
                title=f"{item.code} {item.name}",
                acceptance=f"{child_text}；优先级：{item.priority}；验收时需满足：{acceptance}",
                doc_link=breakdown_link,
            )
        )

    tasks.append(
        GeneratedTask(
            title="联调验证与发布闸门准备",
            acceptance="关键业务场景、异常阻塞和替代验证均已记录，可进入发布闸门检查",
            doc_link=breakdown_link,
        )
    )

    results: list[tuple[str, GeneratedTask]] = []
    first_id = initial_task_id
    results.append((first_id, tasks[0]))

    id_match = re.match(r"^(.*?)(\d+)$", initial_task_id)
    if id_match:
        prefix = id_match.group(1)
        width = len(id_match.group(2))
        start = int(id_match.group(2))
        for offset, task in enumerate(tasks[1:], start=1):
            results.append((f"{prefix}{start + offset:0{width}d}", task))
        return results

    generated_seed = next_task_id(task_file, sync_date)
    seed_match = re.match(r"^(.*?)(\d+)$", generated_seed)
    if not seed_match:
        return results
    prefix = seed_match.group(1)
    width = len(seed_match.group(2))
    start = int(seed_match.group(2))
    for offset, task in enumerate(tasks[1:]):
        results.append((f"{prefix}{start + offset:0{width}d}", task))
    return results


def next_task_id(task_file: Path, doc_date: str) -> str:
    prefix = f"TASK-{doc_date}-"
    if not task_file.exists():
        return f"{prefix}01"
    content = task_file.read_text(encoding="utf-8")
    nums = [int(match.group(1)) for match in re.finditer(rf"{re.escape(prefix)}(\d+)", content)]
    return f"{prefix}{max(nums, default=0) + 1:02d}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Populate requirement bundle docs with PRD-driven body content")
    add_profile_arg(parser)
    parser.add_argument("--req-file", help="Requirement pool file")
    parser.add_argument("--task-file", help="Task board file")
    parser.add_argument("--req-id", required=True)
    parser.add_argument("--initial-task-id", required=True)
    parser.add_argument("--theme", required=True)
    parser.add_argument("--date", required=True)
    parser.add_argument("--bundle-dir", required=True, help="Requirement bundle directory")
    parser.add_argument("--prd-file", required=True, help="PRD markdown file")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    section_title_map = get_section_title_map(profile)
    table_column_aliases = get_table_column_aliases(profile)
    acceptance_alias_map, acceptance_items_map, test_case_keyword_map, acceptance_fallback_map = get_feature_rule_maps(profile)
    req_file = Path(args.req_file).resolve() if args.req_file else project_paths.requirements_pool
    task_file = Path(args.task_file).resolve() if args.task_file else project_paths.task_board
    bundle_dir = Path(args.bundle_dir).resolve()
    prd_file = Path(args.prd_file).resolve()

    if not prd_file.exists():
        print(f"Error: PRD file not found: {prd_file}")
        return 1

    design_dir = bundle_dir / "design"
    testing_dir = bundle_dir / "testing"
    if not design_dir.exists():
        print(f"Error: design directory not found: {design_dir}")
        return 1

    tech_design = find_doc(design_dir, "技术设计")
    detailed_design = find_doc(design_dir, "详细开发设计")
    breakdown = find_doc(design_dir, "开发任务拆解")
    physical_design = find_doc(design_dir, "物理表设计")
    table_mapping = find_doc(design_dir, "表名对照表")
    prd_trace = find_doc(design_dir, "PRD追溯")
    product_confirmation = find_doc(design_dir, "产品确认清单")
    impl_alignment = find_doc(design_dir, "流程图与实现对齐")
    acceptance_doc = find_testing_doc(testing_dir, "联调验收记录")
    test_result_doc = find_testing_doc(testing_dir, "测试结果")
    uat_doc = find_testing_doc(testing_dir, "UAT测试用例")

    if not tech_design or not detailed_design or not breakdown or not prd_trace:
        print("Error: required bundle docs are missing")
        return 1

    prd_text = prd_file.read_text(encoding="utf-8")
    sections = parse_sections(prd_text)
    background_sections = find_section_by_titles(sections, section_title_map["background"])
    goal_sections = find_section_by_titles(sections, section_title_map["goal"])
    current_sections = find_section_by_titles(sections, section_title_map["current_state"])
    solution_sections = find_section_by_titles(sections, section_title_map["solution"])
    dependency_sections = find_section_by_titles(sections, section_title_map["dependencies"])
    risk_sections = find_section_by_titles(sections, section_title_map["risks"])
    scenario_sections = find_section_by_titles(sections, section_title_map["scenarios"])
    principle_sections = find_section_by_titles(sections, section_title_map["principles"])
    table_sections = find_section_by_titles(sections, section_title_map["tables"])
    function_section = next(iter(find_section_by_titles(sections, section_title_map["function_list"])), None)
    if function_section is None:
        function_section = find_section_by_table_columns(sections, table_column_aliases["function_list"])
    top_modules, child_map = extract_architecture(sections)
    function_items = extract_function_items(sections, function_section, table_column_aliases["function_list"])
    acceptance_section = next(iter(find_section_by_titles(sections, section_title_map["acceptance"])), None)
    if acceptance_section is None:
        acceptance_section = find_section_by_table_columns(sections, table_column_aliases["acceptance"])
    acceptance_rows = parse_first_table(acceptance_section)
    focus_test_cases = list_items_from_sections(find_section_by_titles(sections, section_title_map["test_cases"]), limit=10)
    for item in function_items:
        item.acceptance_points = resolve_acceptance_points(
            item,
            acceptance_rows,
            focus_test_cases,
            table_column_aliases["acceptance"],
            acceptance_items_map,
            test_case_keyword_map,
            acceptance_alias_map,
            acceptance_fallback_map,
        )
    if not top_modules and function_items:
        top_modules = uniq([item.module for item in function_items])
    use_cases = uniq(list_items_from_sections(scenario_sections, limit=8) + focus_test_cases)[:10]
    principles = list_items_from_sections(principle_sections, limit=8)
    objects = extract_object_names(find_section_by_titles(sections, section_title_map["objects"]), limit=10)
    table_names = [name for name in objects if "_" in name or "表" in name][:8]
    confirmation_items = summary_points(risk_sections + find_sections(sections, ("特殊", "不包含", "已确认事项")), limit=5)

    bundle_rel = bundle_dir.relative_to(project_paths.workspace_root).as_posix()
    design_rel = tech_design.relative_to(project_paths.workspace_root).as_posix()
    detailed_design_rel = detailed_design.relative_to(project_paths.workspace_root).as_posix()
    breakdown_rel = breakdown.relative_to(project_paths.workspace_root).as_posix()
    prd_rel = prd_file.relative_to(project_paths.workspace_root).as_posix()
    uat_rel = uat_doc.relative_to(project_paths.workspace_root).as_posix() if uat_doc else "待补充"

    all_doc_paths = [
        path
        for path in [
            tech_design,
            detailed_design,
            breakdown,
            physical_design,
            table_mapping,
            prd_trace,
            product_confirmation,
            impl_alignment,
            acceptance_doc,
            test_result_doc,
            uat_doc,
        ]
        if path is not None
    ]
    doc_links = [md_link(path, project_paths.workspace_root) for path in all_doc_paths]
    design_doc_links = [md_link(path, project_paths.workspace_root) for path in all_doc_paths if path.parent.name == "design"]

    print_header(
        "Populate Requirement Content",
        {
            "req_id": args.req_id,
            "theme": args.theme,
            "bundle_dir": str(bundle_dir),
            "prd_file": str(prd_file),
            "mode": "dry-run" if args.dry_run else "live",
        },
    )

    write_text(
        tech_design,
        build_tech_design(
            args.date,
            args.theme,
            prd_rel,
            args.req_id,
            background_sections,
            goal_sections,
            current_sections,
            solution_sections,
            dependency_sections,
            risk_sections,
            principles,
            use_cases,
            top_modules,
        ),
        args.dry_run,
    )
    write_text(
        detailed_design,
        build_detailed_design(
            args.date,
            args.theme,
            prd_rel,
            top_modules,
            child_map,
            function_items,
            objects,
            table_sections or dependency_sections,
            risk_sections,
            use_cases,
        ),
        args.dry_run,
    )
    write_text(
        breakdown,
        build_breakdown(
            args.date,
            args.theme,
            design_rel,
            detailed_design_rel,
            prd_rel,
            function_items,
            top_modules,
            child_map,
            use_cases,
        ),
        args.dry_run,
    )
    if physical_design:
        write_text(physical_design, build_physical_design(args.date, args.theme, objects, table_names), args.dry_run)
    if table_mapping:
        write_text(table_mapping, build_table_mapping(args.date, args.theme, objects, table_names), args.dry_run)
    write_text(
        prd_trace,
        build_prd_trace(
            args.date,
            args.theme,
            prd_rel,
            design_rel,
            detailed_design_rel,
            breakdown_rel,
            uat_rel,
            top_modules,
            child_map,
        ),
        args.dry_run,
    )
    if product_confirmation:
        write_text(product_confirmation, build_product_confirmation(args.date, args.theme, prd_rel, confirmation_items), args.dry_run)
    if impl_alignment:
        write_text(impl_alignment, build_impl_alignment(args.date, args.theme, prd_rel, top_modules), args.dry_run)
    if acceptance_doc:
        write_text(acceptance_doc, build_acceptance_doc(args.date, args.theme, use_cases), args.dry_run)
    if test_result_doc:
        write_text(test_result_doc, build_test_result_doc(args.date, args.theme, use_cases), args.dry_run)
    if uat_doc:
        write_text(uat_doc, build_uat_cases(args.date, args.theme, use_cases), args.dry_run)

    background_items = summary_points(background_sections, limit=5)
    goal_items = summary_points(goal_sections, limit=5)
    update_requirement_section(req_file, args.req_id, background_items, goal_items, doc_links, args.dry_run)

    req_row = find_requirement_row(req_file, args.req_id)
    if req_row is None:
        print("Error: requirement row not found after bundle creation")
        return 1

    current_title = get_cell(req_row.cells, req_row.header_map, ("标题", "需求标题")) or args.theme
    current_status = get_cell(req_row.cells, req_row.header_map, ("状态",)) or "planned"
    current_source = get_cell(req_row.cells, req_row.header_map, ("来源",)) or md_link(prd_file, project_paths.workspace_root)
    if "待补PRD文档.md" in current_source or "待补PRD文档" in current_source:
        current_source = md_link(prd_file, project_paths.workspace_root)
    current_task_board = get_cell(req_row.cells, req_row.header_map, ("任务拆解",)) or md_link(task_file, project_paths.workspace_root)

    sync_requirement_pool_entry(
        req_path=req_file,
        req_id=args.req_id,
        title=current_title,
        status=current_status,
        source=current_source,
        design_docs=design_doc_links,
        task_board=current_task_board,
        sync_date=args.date,
        dry_run=args.dry_run,
    )

    breakdown_link = md_link(breakdown, project_paths.workspace_root)
    task_pairs = generated_tasks(
        initial_task_id=args.initial_task_id,
        task_file=task_file,
        sync_date=args.date,
        breakdown_link=breakdown_link,
        function_items=function_items,
        top_modules=top_modules,
        child_map=child_map,
        use_cases=use_cases,
    )

    for task_id, task in task_pairs:
        sync_task_board_entry(
            task_path=task_file,
            req_id=args.req_id,
            req_title=current_title,
            task_id=task_id,
            task_title=task.title,
            status="todo",
            acceptance=task.acceptance,
            doc_link=task.doc_link,
            sync_date=args.date,
            dry_run=args.dry_run,
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
