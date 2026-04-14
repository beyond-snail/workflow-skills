#!/usr/bin/env python3
from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any


LEGACY_SCAN_PATH = Path(".ai/runtime/cache/legacy-scan.json")


def load_legacy_scan(workspace_root: Path) -> dict[str, Any]:
    path = workspace_root / LEGACY_SCAN_PATH
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}


def _tokenize(text: str) -> set[str]:
    parts = re.split(r"[\s_\-/]+", text.lower())
    tokens = {part for part in parts if len(part) >= 2}
    compact = "".join(ch for ch in text.lower() if ch.isalnum())
    if len(compact) >= 3:
        tokens.add(compact)
    return tokens


def match_legacy_context(scan: dict[str, Any], *queries: str) -> dict[str, Any]:
    if not scan:
        return {
            "matched_domains": [],
            "matched_chains": [],
            "matched_docs": [],
            "summary": "未找到 legacy-scan 缓存，跳过老项目画像命中分析。",
            "evidence_refs": [],
        }

    query_tokens: set[str] = set()
    for item in queries:
        if item:
            query_tokens.update(_tokenize(item))

    compact_queries = {"".join(ch for ch in item.lower() if ch.isalnum()) for item in queries if item}

    def score_text(value: str) -> int:
        target_tokens = _tokenize(value)
        compact_value = "".join(ch for ch in value.lower() if ch.isalnum())
        score = len(query_tokens & target_tokens)
        if compact_value:
            for query in compact_queries:
                if query and (query in compact_value or compact_value in query):
                    score += 2
        return score

    domains = []
    for domain in scan.get("business_domains", []):
        score = score_text(domain.get("name", "")) + score_text(domain.get("summary", ""))
        for evidence in domain.get("evidence", []):
            score += score_text(evidence)
        for evidence in domain.get("doc_evidence", []):
            score += score_text(evidence)
        if score > 0:
            domains.append((score, domain))
    domains.sort(key=lambda item: (-item[0], item[1].get("name", "")))
    matched_domains = [item[1] for item in domains[:3]]

    allowed_domain_names = {item.get("name", "") for item in matched_domains}
    chains = []
    for chain in scan.get("key_api_chains", []):
        score = score_text(chain.get("endpoint", "")) + score_text(chain.get("domain", ""))
        score += score_text(chain.get("service", "")) + score_text(chain.get("mapper_or_table", ""))
        for tag in chain.get("tags", []):
            score += score_text(tag)
        if chain.get("domain", "") in allowed_domain_names:
            score += 2
        if score > 0:
            chains.append((score, chain))
    chains.sort(key=lambda item: (-item[0], item[1].get("endpoint", "")))
    matched_chains = [item[1] for item in chains[:5]]

    docs = []
    for item in scan.get("doc_inventory", []):
        score = score_text(item.get("path", "")) + score_text(item.get("title", ""))
        if score > 0:
            docs.append((score, item))
    docs.sort(key=lambda item: (-item[0], item[1].get("path", "")))
    matched_docs = [item[1] for item in docs[:5]]

    evidence_refs = []
    legacy_analysis = "docs/workflow/legacy-analysis.md"
    evidence_refs.append(legacy_analysis)
    evidence_refs.extend(item.get("path", "") for item in matched_docs if item.get("path"))

    if matched_domains:
        summary = "命中老项目画像：" + "、".join(item.get("name", "") for item in matched_domains)
    else:
        summary = "已加载 legacy-scan 缓存，但未命中明显业务域。"

    return {
        "matched_domains": matched_domains,
        "matched_chains": matched_chains,
        "matched_docs": matched_docs,
        "summary": summary,
        "evidence_refs": list(dict.fromkeys(ref for ref in evidence_refs if ref)),
    }


def render_legacy_context_lines(context: dict[str, Any]) -> list[str]:
    lines = [f"- legacy_context: {context.get('summary', '(none)')}"]
    domains = context.get("matched_domains", [])
    if domains:
        lines.append("- legacy_domains: " + ", ".join(item.get("name", "") for item in domains))
    chains = context.get("matched_chains", [])
    if chains:
        lines.append("- legacy_api_chains: " + ", ".join(item.get("endpoint", "") for item in chains[:3]))
    docs = context.get("matched_docs", [])
    if docs:
        lines.append("- legacy_docs: " + ", ".join(item.get("path", "") for item in docs[:3]))
    return lines
