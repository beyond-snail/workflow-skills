#!/usr/bin/env python3
from __future__ import annotations

import json
from collections import Counter
from datetime import datetime
from pathlib import Path
from typing import Any

from md_board_utils import get_cell, parse_table_rows


STATE_RELATIVE_PATH = Path(".ai/runtime/project-state.json")

REQ_STATUS_ALIASES = {
    "draft": {"draft", "草稿", "待梳理", "待收集"},
    "planned": {"planned", "计划中", "已冻结", "待审核"},
    "doing": {"doing", "进行中", "开发中"},
    "done": {"done", "已完成", "已收口"},
    "closed": {"closed", "已关闭", "已终止"},
}

TASK_STATUS_ALIASES = {
    "todo": {"todo", "待办", "待处理", "待开始"},
    "doing": {"doing", "进行中", "开发中"},
    "blocked": {"blocked", "阻塞中", "已阻塞"},
    "review": {"review", "待审核", "审核中", "待人工审核"},
    "done": {"done", "已完成", "已收口"},
}


def resolve_state_path(workspace_root: Path) -> Path:
    return workspace_root / STATE_RELATIVE_PATH


def load_state(workspace_root: Path, profile: dict[str, Any]) -> dict[str, Any]:
    path = resolve_state_path(workspace_root)
    base = default_state(workspace_root, profile)
    if not path.exists():
        return base
    try:
        loaded = json.loads(path.read_text(encoding="utf-8")) or {}
    except json.JSONDecodeError:
        return base
    return deep_merge(base, loaded)


def default_state(workspace_root: Path, profile: dict[str, Any]) -> dict[str, Any]:
    project = profile.get("project", {})
    paths = profile.get("paths", {})
    return {
        "schemaVersion": "1.0",
        "project": {
            "name": project.get("name", workspace_root.name),
            "path": str(workspace_root),
            "language": project.get("language", ""),
            "buildTool": project.get("build_tool", ""),
            "docsRoot": paths.get("docs_root", "docs/workflow"),
            "prdDirectory": paths.get("prd_directory", "docs/workflow/PRD"),
        },
        "workflow": {
            "stage": "bootstrap",
            "gateStatus": "待初始化",
            "health": "待扫描",
            "risk": "未知",
            "currentReqId": "",
            "currentReqTitle": "",
            "currentTaskId": "",
            "currentTaskTitle": "",
            "currentTaskStatus": "",
            "currentMode": "",
        },
        "metrics": {
            "totalRequirements": 0,
            "draftRequirements": 0,
            "plannedRequirements": 0,
            "doingRequirements": 0,
            "doneRequirements": 0,
            "closedRequirements": 0,
            "totalTasks": 0,
            "todo": 0,
            "doing": 0,
            "blocked": 0,
            "review": 0,
            "done": 0,
            "evidenceCoverage": 0,
        },
        "requirements": [],
        "tasks": [],
        "evidence": [],
        "risks": [],
        "timeline": [],
        "sync": {
            "source": "bootstrap",
            "status": "fresh",
            "lastSyncAt": "",
        },
    }


def build_requirement_entries(requirements_pool: Path) -> list[dict[str, Any]]:
    if not requirements_pool.exists():
        return []
    entries: list[dict[str, Any]] = []
    for row in parse_table_rows(requirements_pool):
        req_id = get_cell(row.cells, row.header_map, ("需求ID", "REQ-ID", "reqId", "req_id")) or row.section_req_id or ""
        if not req_id:
            continue
        entries.append(
            {
                "reqId": req_id,
                "title": get_cell(row.cells, row.header_map, ("标题", "需求标题")) or "",
                "status": get_cell(row.cells, row.header_map, ("状态",)) or "",
                "source": get_cell(row.cells, row.header_map, ("来源",)) or "",
                "designDocs": get_cell(row.cells, row.header_map, ("设计文档",)) or "",
                "taskBoard": get_cell(row.cells, row.header_map, ("任务拆解",)) or "",
            }
        )
    return entries


def build_task_entries(task_board: Path) -> list[dict[str, Any]]:
    if not task_board.exists():
        return []
    entries: list[dict[str, Any]] = []
    for row in parse_table_rows(task_board):
        task_id = get_cell(row.cells, row.header_map, ("任务ID", "TASK-ID", "taskId", "task_id")) or ""
        title = get_cell(row.cells, row.header_map, ("任务标题", "标题", "任务名称")) or ""
        status = get_cell(row.cells, row.header_map, ("状态",)) or ""
        acceptance = get_cell(row.cells, row.header_map, ("验收标准",)) or ""
        docs = get_cell(row.cells, row.header_map, ("关联文档", "关联需求", "任务拆解")) or ""
        if not task_id and not title:
            continue
        entries.append(
            {
                "taskId": task_id,
                "reqId": row.section_req_id or get_cell(row.cells, row.header_map, ("需求ID", "REQ-ID", "关联需求")) or "",
                "title": title,
                "status": status,
                "acceptance": acceptance,
                "docs": docs,
            }
        )
    return entries


def canonical_status(value: str) -> str:
    normalized = value.strip().lower()
    if not normalized:
        return "unknown"
    for canonical, aliases in TASK_STATUS_ALIASES.items():
        if normalized in aliases:
            return canonical
    for canonical, aliases in REQ_STATUS_ALIASES.items():
        if normalized in aliases:
            return canonical
    return normalized


def build_metrics(requirements: list[dict[str, Any]], tasks: list[dict[str, Any]]) -> dict[str, int]:
    req_counts = Counter(canonical_status(item.get("status", "")) for item in requirements)
    task_counts = Counter(canonical_status(item.get("status", "")) for item in tasks)
    total_tasks = len(tasks)
    done = task_counts.get("done", 0)
    evidence_coverage = int(round((done / total_tasks) * 100)) if total_tasks else 0
    return {
        "totalRequirements": len(requirements),
        "draftRequirements": req_counts.get("draft", 0),
        "plannedRequirements": req_counts.get("planned", 0),
        "doingRequirements": req_counts.get("doing", 0),
        "doneRequirements": req_counts.get("done", 0),
        "closedRequirements": req_counts.get("closed", 0),
        "totalTasks": total_tasks,
        "todo": task_counts.get("todo", 0),
        "doing": task_counts.get("doing", 0),
        "blocked": task_counts.get("blocked", 0),
        "review": task_counts.get("review", 0),
        "done": done,
        "evidenceCoverage": evidence_coverage,
    }


def update_timeline(
    timeline: list[dict[str, Any]],
    *,
    stage: str,
    source: str,
    title: str,
    detail: str,
    req_id: str = "",
    task_id: str = "",
) -> list[dict[str, Any]]:
    event_id = ":".join(filter(None, [source, stage, req_id, task_id, title]))
    if any(item.get("id") == event_id for item in timeline):
        return timeline
    timeline.append(
        {
            "id": event_id,
            "time": datetime.now().strftime("%Y-%m-%d %H:%M"),
            "stage": stage,
            "source": source,
            "title": title,
            "detail": detail,
            "reqId": req_id,
            "taskId": task_id,
        }
    )
    return timeline


def merge_unique_entries(
    existing: list[dict[str, Any]],
    updates: list[dict[str, Any]],
    key_fields: tuple[str, ...],
) -> list[dict[str, Any]]:
    merged: list[dict[str, Any]] = []
    seen: set[tuple[str, ...]] = set()

    for item in [*existing, *updates]:
        key = tuple(str(item.get(field, "")) for field in key_fields)
        if key in seen:
            continue
        seen.add(key)
        merged.append(item)
    return merged


def deep_merge(base: dict[str, Any], override: dict[str, Any]) -> dict[str, Any]:
    result = dict(base)
    for key, value in override.items():
        if key in result and isinstance(result[key], dict) and isinstance(value, dict):
            result[key] = deep_merge(result[key], value)
        else:
            result[key] = value
    return result


def build_project_state(
    workspace_root: Path,
    profile: dict[str, Any],
    *,
    requirements_pool: Path,
    task_board: Path,
    stage: str,
    gate_status: str,
    health: str,
    risk: str,
    sync_source: str,
    sync_status: str,
    current_req_id: str = "",
    current_req_title: str = "",
    current_task_id: str = "",
    current_task_title: str = "",
    current_task_status: str = "",
    current_mode: str = "",
    summary: str = "",
    blockers: list[str] | None = None,
    evidence_refs: list[str] | None = None,
    legacy_context: dict[str, Any] | None = None,
) -> dict[str, Any]:
    state = load_state(workspace_root, profile)
    requirements = build_requirement_entries(requirements_pool)
    tasks = build_task_entries(task_board)
    metrics = build_metrics(requirements, tasks)

    state["project"] = {
        "name": profile.get("project", {}).get("name", workspace_root.name),
        "path": str(workspace_root),
        "language": profile.get("project", {}).get("language", ""),
        "buildTool": profile.get("project", {}).get("build_tool", ""),
        "docsRoot": profile.get("paths", {}).get("docs_root", "docs/workflow"),
        "prdDirectory": profile.get("paths", {}).get("prd_directory", "docs/workflow/PRD"),
    }
    state["workflow"] = {
        "stage": stage,
        "gateStatus": gate_status,
        "health": health,
        "risk": risk,
        "currentReqId": current_req_id,
        "currentReqTitle": current_req_title,
        "currentTaskId": current_task_id,
        "currentTaskTitle": current_task_title,
        "currentTaskStatus": current_task_status,
        "currentMode": current_mode,
    }
    if legacy_context is not None:
        state["workflow"]["legacyContext"] = {
            "summary": legacy_context.get("summary", ""),
            "matchedDomains": [item.get("name", "") for item in legacy_context.get("matched_domains", [])],
            "matchedChains": [item.get("endpoint", "") for item in legacy_context.get("matched_chains", [])[:5]],
            "matchedDocs": [item.get("path", "") for item in legacy_context.get("matched_docs", [])[:5]],
        }
    state["metrics"] = metrics
    state["requirements"] = requirements
    state["tasks"] = tasks
    if blockers is not None:
        state["risks"] = [{"source": sync_source, "text": blocker} for blocker in blockers if blocker]
    if evidence_refs:
        evidence = state.get("evidence", [])
        additions = [{"kind": "file", "ref": ref} for ref in evidence_refs if ref]
        state["evidence"] = merge_unique_entries(evidence, additions, ("kind", "ref"))
    state["timeline"] = update_timeline(
        list(state.get("timeline", [])),
        stage=stage,
        source=sync_source,
        title=summary or gate_status,
        detail=summary or gate_status,
        req_id=current_req_id,
        task_id=current_task_id,
    )
    state["sync"] = {
        "source": sync_source,
        "status": sync_status,
        "lastSyncAt": datetime.now().strftime("%Y-%m-%d %H:%M"),
    }
    return state


def write_project_state(workspace_root: Path, state: dict[str, Any], dry_run: bool) -> Path:
    path = resolve_state_path(workspace_root)
    if dry_run:
        return path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return path
