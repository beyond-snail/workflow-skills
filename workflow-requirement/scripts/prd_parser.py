#!/usr/bin/env python3
from __future__ import annotations

import re
from pathlib import Path
from typing import Any


SECTION_RE = re.compile(r"^##\s+(\d+(?:\.\d+)*)\.?\s+(.+?)\s*$", re.MULTILINE)
SUBSECTION_RE = re.compile(r"^###\s+(\d+(?:\.\d+)*)\.?\s+(.+?)\s*$", re.MULTILINE)
TABLE_SEPARATOR_RE = re.compile(r"^\|(?:\s*:?-+:?\s*\|)+\s*$")
LIST_ITEM_RE = re.compile(r"^\s*(?:[-*]|\d+\.)\s+(.+?)\s*$")


class PrdParser:
    @classmethod
    def parse(cls, path: str | Path | None) -> dict[str, Any] | None:
        if not path:
            return None
        file_path = Path(path)
        if not file_path.exists() or not file_path.is_file():
            return None
        try:
            text = file_path.read_text(encoding="utf-8")
        except OSError:
            return None

        sections = cls._split_sections(text)
        if not sections:
            return {
                "path": str(file_path),
                "raw_text": text,
            }

        result: dict[str, Any] = {
            "path": str(file_path),
            "raw_text": text,
            "doc_info": cls._get_section(sections, [("1", "文档信息")]),
            "background": cls._get_section(sections, [("2", "背景与目标"), ("2", "背景"), ("2", "项目背景")]),
            "roles": cls._get_section(sections, [("3", "角色与使用场景"), ("3", "用户与场景"), ("3", "角色"), ("3", "用户场景")]),
            "terms": cls._get_section(sections, [("4", "术语与口径定义"), ("4", "术语"), ("4", "口径定义")]),
            "problems": cls._get_section(sections, [("5", "现状问题分析"), ("5", "现状分析"), ("5", "当前代码与数据基线"), ("5", "当前事实基线")]),
            "dependencies": cls._get_section(sections, [("6", "业务范围与依赖"), ("6", "项目范围"), ("6", "范围定义"), ("6", "依赖与前置")]),
            "function_overview": cls._get_section(sections, [("7", "功能概述"), ("7", "功能清单"), ("7", "功能详情"), ("7", "详细需求"), ("7", "方案设计")]),
            "data_structures": cls._get_section(sections, [("8", "数据结构设计"), ("8", "数据模型建议"), ("8", "表结构设计"), ("8", "数据建议")]),
            "process_design": cls._get_section(sections, [("9", "核心流程设计"), ("9", "流程设计"), ("9", "核心流程"), ("9", "业务流程")]),
            "business_rules": cls._get_section(
                sections,
                [
                    ("10", "详细业务规则"),
                    ("10", "核心业务规则"),
                    ("10", "业务规则"),
                    ("10", "功能规则"),
                    ("10", "处理规则"),
                ],
            ),
            "integration": cls._get_section(
                sections,
                [
                    ("11", "系统集成与异常处理"),
                    ("11", "系统集成"),
                    ("11", "异常场景与处理策略"),
                    ("11", "异常处理"),
                    ("11", "集成与依赖"),
                ],
            ),
            "reports": cls._get_section(sections, [("12", "报表与查询"), ("12", "查询与统计"), ("12", "统计与报表")]),
            "exceptions": cls._get_section(
                sections,
                [
                    ("13", "异常场景与处理策略"),
                    ("13", "异常处理策略"),
                    ("13", "异常场景"),
                    ("14", "异常场景与处理策略"),
                    ("14", "异常处理策略"),
                ],
            ),
            "non_functional": cls._get_section(sections, [("14", "非功能需求"), ("15", "非功能需求"), ("14", "性能与安全"), ("15", "性能与安全")]),
            "acceptance": cls._get_section(sections, [("15", "验收标准"), ("16", "验收标准"), ("15", "验收口径"), ("16", "验收口径")]),
            "implementation_suggestions": cls._get_section(
                sections,
                [("16", "研发实现建议"), ("17", "研发实现建议"), ("16", "实现建议"), ("17", "实现建议")],
            ),
            "risks": cls._get_section(sections, [("17", "风险与依赖评估"), ("18", "风险与依赖评估"), ("17", "风险与待确认"), ("18", "风险与待确认")]),
            "review_conclusion": cls._get_section(sections, [("18", "评审结论"), ("19", "评审结论"), ("18", "结论"), ("19", "结论")]),
            "formulas": cls._get_section(sections, [("19", "附录：关键公式汇总"), ("19", "附录:关键公式汇总"), ("20", "附录：关键公式汇总"), ("20", "附录:关键公式汇总")]),
        }

        result["doc_info_rows"] = cls.extract_tables(result["doc_info"])
        result["goals_rows"] = cls.extract_tables(result["background"])
        result["roles_rows"] = cls.extract_tables(result["roles"])
        result["terms_rows"] = cls.extract_tables(result["terms"])
        result["function_list_rows"] = cls._extract_named_table(result["function_overview"], ["编号", "模块", "功能名称", "描述", "优先级"])
        result["process_order_items"] = cls.extract_list_items(result["function_overview"])
        result["data_structure_tables"] = cls.extract_tables(result["data_structures"])
        result["exception_rows"] = cls.extract_tables(result["exceptions"])
        result["acceptance_rows"] = cls.extract_tables(result["acceptance"])
        result["risk_rows"] = cls.extract_tables(result["risks"])
        result["scope_in_items"] = cls._extract_scope_items(result["background"], "本期包含")
        result["scope_out_items"] = cls._extract_scope_items(result["background"], "本期不包含")
        result["core_principles"] = cls._extract_numbered_block(result["background"], "### 2.3 核心原则")
        result["subsections"] = cls._split_subsections(sections)
        return result

    @classmethod
    def _normalize_section_title(cls, title: str) -> str:
        return re.sub(r"\s+", "", title).replace("：", ":").strip()

    @classmethod
    def _get_section(cls, sections: dict[str, str], candidates: list[tuple[str, str]]) -> str:
        normalized_map = {
            cls._normalize_section_title(key): value
            for key, value in sections.items()
        }
        for number, name in candidates:
            exact_key = cls._normalize_section_title(f"{number}. {name}")
            if exact_key in normalized_map:
                return normalized_map[exact_key]
        for key, value in normalized_map.items():
            for number, name in candidates:
                normalized_name = cls._normalize_section_title(name)
                if key.startswith(f"{number}.") and normalized_name in key:
                    return value
        return ""

    @classmethod
    def _split_sections(cls, text: str) -> dict[str, str]:
        matches = list(SECTION_RE.finditer(text))
        sections: dict[str, str] = {}
        for index, match in enumerate(matches):
            key = f"{match.group(1)}. {match.group(2).strip()}"
            start = match.end()
            end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
            sections[key] = text[start:end].strip()
        return sections

    @classmethod
    def _split_subsections(cls, sections: dict[str, str]) -> dict[str, dict[str, str]]:
        result: dict[str, dict[str, str]] = {}
        for section_name, content in sections.items():
            matches = list(SUBSECTION_RE.finditer(content))
            sub_map: dict[str, str] = {}
            for index, match in enumerate(matches):
                key = f"{match.group(1)} {match.group(2).strip()}"
                start = match.end()
                end = matches[index + 1].start() if index + 1 < len(matches) else len(content)
                sub_map[key] = content[start:end].strip()
            if sub_map:
                result[section_name] = sub_map
        return result

    @classmethod
    def extract_tables(cls, text: str) -> list[list[dict[str, str]]]:
        lines = text.splitlines()
        tables: list[list[dict[str, str]]] = []
        i = 0
        while i < len(lines):
            line = lines[i].strip()
            if line.startswith("|") and i + 1 < len(lines) and TABLE_SEPARATOR_RE.match(lines[i + 1].strip()):
                headers = [cell.strip() for cell in line.strip("|").split("|")]
                i += 2
                rows: list[dict[str, str]] = []
                while i < len(lines) and lines[i].strip().startswith("|"):
                    cells = [cell.strip() for cell in lines[i].strip().strip("|").split("|")]
                    if len(cells) < len(headers):
                        cells.extend([""] * (len(headers) - len(cells)))
                    row = {headers[idx]: cells[idx] if idx < len(cells) else "" for idx in range(len(headers))}
                    rows.append(row)
                    i += 1
                tables.append(rows)
                continue
            i += 1
        return tables

    @classmethod
    def extract_list_items(cls, text: str) -> list[str]:
        items: list[str] = []
        for line in text.splitlines():
            match = LIST_ITEM_RE.match(line)
            if match:
                items.append(match.group(1).strip())
        return items

    @classmethod
    def _extract_named_table(cls, text: str, expected_headers: list[str]) -> list[dict[str, str]]:
        for table in cls.extract_tables(text):
            if not table:
                continue
            headers = set(table[0].keys())
            if all(header in headers for header in expected_headers):
                return table
        return []

    @classmethod
    def _extract_scope_items(cls, text: str, marker: str) -> list[str]:
        block = cls._extract_block_between_markers(text, f"**{marker}：**", "**本期不包含：**" if marker == "本期包含" else "---")
        return cls.extract_list_items(block)

    @classmethod
    def _extract_numbered_block(cls, text: str, heading: str) -> list[str]:
        marker = text.find(heading)
        if marker < 0:
            return []
        remainder = text[marker + len(heading):]
        next_heading = remainder.find("### ")
        block = remainder[:next_heading].strip() if next_heading >= 0 else remainder.strip()
        return cls.extract_list_items(block)

    @classmethod
    def _extract_block_between_markers(cls, text: str, start_marker: str, end_marker: str) -> str:
        start = text.find(start_marker)
        if start < 0:
            return ""
        start += len(start_marker)
        end = text.find(end_marker, start)
        if end < 0:
            end = len(text)
        return text[start:end].strip()
