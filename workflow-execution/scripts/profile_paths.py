#!/usr/bin/env python3
"""Resolve common project paths from profile configuration for workflow-execution.

Localized copy to keep the skill self-contained without relying on _shared/.
"""
from __future__ import annotations

from dataclasses import dataclass
import os
from pathlib import Path
from typing import Any


@dataclass
class ProjectPaths:
    workspace_root: Path
    docs_root: Path
    requirements_pool: Path
    task_board: Path
    prd_directory: Path
    readme_index: Path
    tasks_index: Path
    tasks_template_dir: Path
    knowledge_dir: Path

    @classmethod
    def from_profile(cls, profile: dict[str, Any], workspace_root: Path | str) -> ProjectPaths:
        root = Path(workspace_root).resolve()
        paths_cfg = profile.get("paths", {})
        docs_root = _resolve_existing_dir(root, paths_cfg.get("docs_root"), ("docs/workflow", "docs", "doc"))
        requirements_pool = _resolve_existing_file(
            root,
            paths_cfg.get("requirements_pool"),
            (
                "docs/workflow/requirements/需求池.md",
                "doc/requirements/需求池.md",
                "docs/requirements/需求池.md",
                "requirements/需求池.md",
            ),
            fallback_name="需求池.md",
        )
        task_board = _resolve_existing_file(
            root,
            paths_cfg.get("task_board"),
            (
                "docs/workflow/requirements/任务看板.md",
                "doc/requirements/任务看板.md",
                "docs/requirements/任务看板.md",
                "requirements/任务看板.md",
            ),
            fallback_name="任务看板.md",
        )
        prd_directory = _resolve_existing_dir(
            root,
            paths_cfg.get("prd_directory"),
            ("docs/workflow/PRD", "doc/PRD", "docs/PRD", "doc/prd", "docs/prd", "PRD", "prd", "specs", "docs/specs"),
            fallback_dir_name="PRD",
        )
        readme_index = _resolve_existing_file(
            root,
            paths_cfg.get("readme_index"),
            ("doc/README.md", "docs/README.md", "README.md"),
            fallback_name="README.md",
        )
        tasks_index = _resolve_existing_file(
            root,
            paths_cfg.get("tasks_index"),
            (".ai/memory/tasks/index.md", "doc/tasks/index.md", "docs/tasks/index.md", "tasks/index.md"),
            fallback_name="index.md",
        )
        tasks_template_dir = _resolve_existing_dir(
            root,
            paths_cfg.get("tasks_template_dir"),
            (".ai/memory/tasks/_template", "doc/tasks/_template", "docs/tasks/_template", "tasks/_template"),
            fallback_dir_name="_template",
        )
        knowledge_dir = _resolve_existing_dir(
            root,
            paths_cfg.get("knowledge_dir"),
            (".ai/memory/knowledge", "doc/knowledge", "docs/knowledge", "knowledge"),
            fallback_dir_name="knowledge",
        )
        if docs_root in {root / "doc", root / "docs", root / "docs/workflow"} and not docs_root.exists():
            if requirements_pool.exists():
                docs_root = requirements_pool.parent.parent
            elif prd_directory.exists():
                docs_root = prd_directory.parent
            elif readme_index.exists() and readme_index.parent != root:
                docs_root = readme_index.parent
        return cls(
            workspace_root=root,
            docs_root=docs_root,
            requirements_pool=requirements_pool,
            task_board=task_board,
            prd_directory=prd_directory,
            readme_index=readme_index,
            tasks_index=tasks_index,
            tasks_template_dir=tasks_template_dir,
            knowledge_dir=knowledge_dir,
        )

    @property
    def requirements_dir(self) -> Path:
        return self.requirements_pool.parent


def _search_by_name(root: Path, target: str, max_depth: int = 4) -> Path | None:
    root_depth = len(root.parts)
    for current_root, dirnames, filenames in os.walk(root):
        current_path = Path(current_root)
        depth = len(current_path.parts) - root_depth
        if depth > max_depth:
            dirnames[:] = []
            continue
        if target in filenames:
            return current_path / target
    return None


def _search_dir_by_name(root: Path, target: str, max_depth: int = 4) -> Path | None:
    root_depth = len(root.parts)
    for current_root, dirnames, _ in os.walk(root):
        current_path = Path(current_root)
        depth = len(current_path.parts) - root_depth
        if current_path.name == target and depth <= max_depth:
            return current_path
        if depth >= max_depth:
            dirnames[:] = []
    return None


def _resolve_existing_file(root: Path, configured: str | None, candidates: tuple[str, ...], fallback_name: str) -> Path:
    if configured:
        path = root / configured
        if path.exists():
            return path
    for candidate in candidates:
        path = root / candidate
        if path.exists():
            return path
    detected = _search_by_name(root, fallback_name)
    if detected:
        return detected
    return root / (configured or candidates[0])


def _resolve_existing_dir(root: Path, configured: str | None, candidates: tuple[str, ...], fallback_dir_name: str | None = None) -> Path:
    if configured:
        path = root / configured
        if path.exists():
            return path
    for candidate in candidates:
        path = root / candidate
        if path.exists():
            return path
    if fallback_dir_name:
        detected = _search_dir_by_name(root, fallback_dir_name)
        if detected:
            return detected
    return root / (configured or candidates[0])
