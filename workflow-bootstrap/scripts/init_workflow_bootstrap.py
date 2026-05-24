#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import shutil
from dataclasses import asdict, dataclass
from pathlib import Path


ROOT_AGENTS = """# AGENTS

## Default Collaboration Contract

默认执行顺序：

`检索历史 -> 分析 -> 改动 -> 验证 -> 沉淀 -> 提交`

Mandatory rules:

1. 先分析后改  
首次输出必须包含：涉及文件/方法、SQL或接口链路、调用链与影响范围、根因结论。
2. 先检索后执行  
开始任务前优先读取历史任务档案、项目知识和现有 requirement/task board；若命中历史问题，先继承已有结论再继续分析。
3. 边界锁定  
仅修改任务明确范围；若必须跨边界，先暂停并说明影响，待确认后再改。
4. 验证闭环  
改动后至少编译通过，并输出验证命令、结果、未覆盖风险；未验证不得提交推送。
5. 沉淀闭环  
对有复用价值的分析、根因、决策、验证和遗留问题，统一更新 `.ai/memory/`；不得只改代码不留痕。

## Source of Truth

- `docs/workflow/开发协作约定.md`
- `docs/workflow/PROJECT_CONTEXT.md`
- `.ai/governance/`
- `.ai/memory/`

## Workflow Chat Aliases

- `wf-init` -> `workflow-bootstrap`
- `wf-doctor` -> `workflow-bootstrap` health check
- `wf-cons` -> `workflow-bootstrap` consistency check
- `wf-req` -> `workflow-requirement`
- `wf-exec` -> `workflow-execution`
- `wf-arc` -> `workflow-execution` archive

## Codex Token 控制
- Codex 压缩恢复优先读取 `.ai/memory/context-brief.md` 和 checkpoint 的 `Transcript Digest`，不得默认读取完整 transcript。
- 多窗口并行时，项目共享稳定状态使用 `.ai/memory/context-brief.md`；窗口级当前焦点优先读取 `.ai/memory/session-briefs/<session_id>.md`。
- 压缩恢复默认只读取 `AGENTS.md` 与 `.ai/memory/context-brief.md`；除非需要事实结论、代码改动、SQL/接口判断或验证提交，否则不得扩展读取历史文件、完整 transcript、完整 checkpoint 或大文档。
- `.ai/runtime/conversations/`、`.ai/memory/compact-checkpoints/` 和 `~/.codex/memories/compact-checkpoints/` 视为冷归档；只有追溯证据时才按关键词局部读取。
- skill 使用只读取触发的 `SKILL.md` 和必要 references；PRD、大日志、大 diff、历史任务文件必须先检索再局部读取。
- 工具输出需要主动限量，优先使用 `rg`、`sed -n`、`git diff --stat`、tail 摘要，避免把大文件整段送入上下文。
"""

HOST_TEMPLATES = {
    "codex": """# Codex Host

## 1. 作用
- 本文件仅提供 Codex 宿主专属补充。
- 共享项目事实统一读取 `docs/workflow/PROJECT_CONTEXT.md`。
- 仓库协作规则统一读取根目录 `AGENTS.md`。

## 2. Codex 宿主补充
- 默认先查 `.ai/memory/tasks/index.md` 与 `.ai/memory/knowledge/`。
- 正式治理主源固定为 `docs/workflow/requirements/`。
""",
    "claude": """# Claude Host

## 1. 作用
- 本文件仅提供 Claude 宿主专属补充。
- 共享项目事实统一读取 `docs/workflow/PROJECT_CONTEXT.md`。
- 仓库协作规则统一读取根目录 `AGENTS.md`。

## 2. Claude 宿主补充
- 默认先查 `.ai/memory/tasks/index.md` 与 `.ai/memory/knowledge/`。
- 正式治理主源固定为 `docs/workflow/requirements/`。
""",
}

WORKFLOW_BIN = """#!/usr/bin/env bash
set -euo pipefail

if [[ -f "$HOME/.codex/skills/workflow-bootstrap/scripts/workflow_cli.py" ]]; then
  exec python3 "$HOME/.codex/skills/workflow-bootstrap/scripts/workflow_cli.py" "$@"
fi

if [[ -f "$HOME/.claude/skills/workflow-bootstrap/scripts/workflow_cli.py" ]]; then
  exec python3 "$HOME/.claude/skills/workflow-bootstrap/scripts/workflow_cli.py" "$@"
fi

echo "[BLOCK] workflow_cli.py not found in ~/.codex or ~/.claude skills" >&2
exit 1
"""


def render_short_bin(subcommand: str) -> str:
    return f"""#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${{BASH_SOURCE[0]}}")" && pwd)"
exec "$SCRIPT_DIR/workflow" {subcommand} "$@"
"""

TASK_INDEX = """# 任务索引

## 1. 目的
统一登记当前仓库的活跃任务、历史任务和遗留问题，作为新窗口或新一轮排障的首个检索入口。

## 2. 使用规则
- 开始任务前先查本索引，再决定是否进入对应任务目录。
- 新任务建立后，必须在本索引新增一条记录。
- 活跃任务状态变化、遗留问题变化、任务关闭时，必须同步更新。

## 3. 状态约定
- `todo`：已识别但尚未开始
- `doing`：正在推进
- `blocked`：存在阻塞
- `done`：已收口
- `archived`：已归档，仅供检索

## 4. 索引模板
| 任务ID | 任务名称 | 类型 | 关联需求/关键词 | 状态 | 最新结论 | 目录 | 最后更新 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TASK-YYYYMMDD-001 | 示例任务 | feature/bugfix/ops | REQ-xxxx / 关键词 | doing | 一句话说明当前结论 | `.ai/memory/tasks/xxxx/` | YYYY-MM-DD |
"""

TASK_README = """# 任务主档案

## 1. 基本信息
- 任务ID：
- 任务名称：
- 类型：`feature / bugfix / continuation / ops`
- 关联需求：
- 当前状态：

## 2. 目标与边界
- 目标：
- 范围内：
- 非范围：

## 3. 入口与链路
- 代码入口：
- SQL / 接口链路：
- 调用链：

## 4. 当前结论
- 结论一：
- 结论二：

## 5. 下一步
- 下一步动作：
- 风险与待确认：
"""

TASK_INBOX = """# Inbox

## 作用
- 暂存临时分析、报错样本、排查路径。
- 仅作为工作记忆收件箱，不承担正式主状态。
"""

TASK_ISSUES = """# Issues

## 记录模板
### ISSUE-001
- 现象：
- 触发条件：
- 根因：
- 处理方案：
- 验证结果：
- 状态：
"""

TASK_DECISIONS = """# Decisions

## 记录模板
### DEC-001
- 决策：
- 背景：
- 选择原因：
- 放弃方案：
- 风险：
"""

TASK_VERIFY = """# Verify

## 记录模板
### YYYY-MM-DD HH:MM
- 验证动作：
- 验证命令：
- 验证结果：
- 未覆盖风险：
"""

KNOWLEDGE_README = """# Knowledge

## 作用
- 存放跨任务、跨窗口可复用的稳定知识。
- 仅收录长期有效、复用价值明确、已形成稳定结论的内容。
"""

CONTEXT_BRIEF = """# Context Brief

## 作用
- Codex 压缩恢复后的优先上下文摘要。
- 只保留项目共享稳定状态、关键决策、关键文件、验证、阻塞和下一步。
- 覆盖更新，不长期追加；建议控制在 120 行以内。

## 当前上下文
- 当前主线：
- 项目共享状态：
- 当前状态：
- 关键决策：
- 关键文件：
- 验证结论：
- 阻塞风险：
- 下一步：

## 恢复规则
- 新窗口或压缩恢复后，先读本文件、`AGENTS.md` 和 `.ai/memory/tasks/index.md`。
- 如果存在 `.ai/memory/session-briefs/<session_id>.md`，它优先承载当前窗口任务焦点。
- 默认只读取 `AGENTS.md` 与本文件；除非需要事实结论、代码改动、SQL/接口判断或验证提交，否则不得扩展读取历史文件、完整 transcript、完整 checkpoint 或大文档。
- 完整 transcript、compact checkpoint、conversation 冷归档仅在追溯证据时按关键词局部读取。
"""

SESSION_BRIEF = """# Session Brief

## 作用
- 仅记录当前窗口/当前 session 的任务焦点。
- 解决多窗口并行时共享 `context-brief.md` 被互相覆盖的问题。
- 覆盖更新，不长期追加。

## 当前会话
- session_id：
- 当前任务：
- 当前结论：
- 关键文件：
- 验证结论：
- 阻塞风险：
- 下一步：

## 恢复规则
- 当前窗口压缩恢复后，优先读本文件，再读 `.ai/memory/context-brief.md`。
- 本文件只描述当前窗口；跨窗口共享状态以 `context-brief.md` 为准。
"""

REQUIREMENTS_POOL = """# 需求池

| REQ-ID | 标题 | 来源 | 状态 | 设计文档 | 备注 |
| --- | --- | --- | --- | --- | --- |
"""

TASK_BOARD = """# 任务看板

| TASK-ID | REQ-ID | 标题 | 状态 | 优先级 | 负责人 | 备注 |
| --- | --- | --- | --- | --- | --- | --- |
"""


@dataclass
class Detection:
    project_name: str
    language: str
    build_tool: str
    docs_root: str
    prd_directory: str
    compile_cmd: str
    test_cmd: str
    source_dirs: list[str]
    module_dirs: list[str]
    code_hints: list[str]


@dataclass
class WorkflowState:
    state: str
    current_score: int
    legacy_score: int
    current_signals: list[str]
    legacy_signals: list[str]
    recommended_actions: list[str]


@dataclass
class BusinessDomain:
    name: str
    summary: str
    confidence: str
    evidence: list[str]
    doc_evidence: list[str]
    code_locations: list[str]
    keywords: list[str]
    aliases: list[str]


@dataclass
class ApiChain:
    domain: str
    endpoint: str
    controller: str
    service: str
    mapper_or_table: str
    downstream: str
    request_object: str
    response_object: str
    confidence: str
    tags: list[str]
    evidence: list[str]


def detect_language_and_build(root: Path) -> tuple[str, str]:
    if (root / "pom.xml").exists():
        return "java", "maven"
    if (root / "build.gradle").exists() or (root / "build.gradle.kts").exists():
        return "java", "gradle"
    if (root / "package.json").exists():
        return "javascript", "npm"
    if (root / "pyproject.toml").exists():
        return "python", "pyproject"
    if (root / "requirements.txt").exists():
        return "python", "pip"
    return "", ""


def detect_compile_test(build_tool: str, root: Path) -> tuple[str, str]:
    if build_tool == "maven":
        return "mvn -q -DskipTests compile", "mvn -q test"
    if build_tool == "gradle":
        wrapper = "./gradlew" if (root / "gradlew").exists() else "gradle"
        return f"{wrapper} compileJava", f"{wrapper} test"
    if build_tool == "npm":
        return "npm run build", "npm test"
    if build_tool in {"pyproject", "pip"}:
        return "python -m py_compile $(rg --files -g '*.py')", "pytest"
    return "", ""


def detect_prd_directory(root: Path) -> str:
    candidates = [
        "docs/workflow/PRD",
        "doc/PRD",
        "docs/PRD",
        "doc/prd",
        "docs/prd",
        "PRD",
        "prd",
        "specs",
        "docs/specs",
    ]
    for candidate in candidates:
        if (root / candidate).exists():
            return candidate
    return "docs/workflow/PRD"


def detect_docs_root(root: Path) -> str:
    return "docs/workflow"


def detect_source_dirs(root: Path) -> list[str]:
    candidates = [
        "src/main/java",
        "src/test/java",
        "src/main/resources",
        "src/main/kotlin",
        "src",
        "app",
        "lib",
        "server",
        "services",
        "packages",
    ]
    found: list[str] = []
    for candidate in candidates:
        if (root / candidate).exists():
            found.append(candidate)
    if found:
        return found[:8]
    nested_candidates = [
        "src/main/java",
        "src/test/java",
        "src/main/resources",
        "src/main/kotlin",
    ]
    for child in sorted(root.iterdir()):
        if not child.is_dir() or child.name.startswith("."):
            continue
        for candidate in nested_candidates:
            nested = child / candidate
            if nested.exists():
                found.append(str(nested.relative_to(root)))
    return found[:8]


def detect_module_dirs(root: Path) -> list[str]:
    modules: list[str] = []
    for child in sorted(root.iterdir()):
        if not child.is_dir():
            continue
        if child.name.startswith(".") or child.name in {"doc", "docs", "target", "build", "node_modules"}:
            continue
        if any((child / marker).exists() for marker in ("pom.xml", "build.gradle", "build.gradle.kts", "package.json", "pyproject.toml")):
            modules.append(child.name)
    return modules[:12]


def detect_code_hints(root: Path) -> list[str]:
    hints: list[str] = []
    hint_targets = {
        "controller": ("controller", "controllers"),
        "service": ("service", "services"),
        "mapper": ("mapper", "mappers"),
        "repository": ("repository", "repositories", "dao"),
        "web": ("web",),
    }
    for label, names in hint_targets.items():
        if any((root / "src/main/java").rglob(name) for name in names) or any((root / name).exists() for name in names):
            hints.append(label)
    return hints


def collect_detection(root: Path) -> Detection:
    language, build_tool = detect_language_and_build(root)
    compile_cmd, test_cmd = detect_compile_test(build_tool, root)
    return Detection(
        project_name=root.name,
        language=language,
        build_tool=build_tool,
        docs_root=detect_docs_root(root),
        prd_directory=detect_prd_directory(root),
        compile_cmd=compile_cmd,
        test_cmd=test_cmd,
        source_dirs=detect_source_dirs(root),
        module_dirs=detect_module_dirs(root),
        code_hints=detect_code_hints(root),
    )


def detect_workflow_state(root: Path) -> WorkflowState:
    current_markers = {
        "docs/workflow/PROJECT_CONTEXT.md": 3,
        "docs/workflow/开发协作约定.md": 2,
        "docs/workflow/requirements/需求池.md": 2,
        "docs/workflow/requirements/任务看板.md": 2,
        ".ai/runtime/project-state.json": 3,
        ".ai/runtime/profile/project-profile.yml": 2,
        ".ai/memory/tasks/index.md": 2,
    }
    legacy_markers = {
        "PROJECT_CONTEXT.md": 2,
        "BUSINESS_LOGIC.md": 1,
        "API_REFERENCE.md": 1,
        "ARCHITECTURE.md": 1,
        "doc/开发协作约定.md": 2,
        "doc/requirements/需求池.md": 3,
        "doc/requirements/任务看板.md": 3,
        "doc/PRD": 2,
    }
    current_signals = [path for path in current_markers if (root / path).exists()]
    legacy_signals = [path for path in legacy_markers if (root / path).exists()]
    current_score = sum(current_markers[path] for path in current_signals)
    legacy_score = sum(legacy_markers[path] for path in legacy_signals)

    if current_score == 0 and legacy_score == 0:
        state = "fresh"
    elif current_score >= 9 and legacy_score >= 5:
        state = "mixed_legacy_current"
    elif current_score >= 9:
        state = "current"
    elif legacy_score >= 5:
        state = "legacy_bootstrap"
    else:
        state = "partial_current"

    recommendations = {
        "fresh": ["初始化标准 workflow 底座", "生成最小项目上下文"],
        "legacy_bootstrap": ["建立 docs/workflow 新主源", "迁移或索引旧治理资料", "生成老项目画像报告"],
        "current": ["补齐缺失文件", "刷新老项目画像与缓存"],
        "partial_current": ["补齐当前底座缺口", "刷新项目上下文与状态缓存"],
        "mixed_legacy_current": ["保留旧目录兼容", "以 docs/workflow 为主源", "输出迁移收口建议与冲突清单"],
    }
    return WorkflowState(
        state=state,
        current_score=current_score,
        legacy_score=legacy_score,
        current_signals=current_signals,
        legacy_signals=legacy_signals,
        recommended_actions=recommendations[state],
    )


def iter_java_files(root: Path, suffix: str) -> list[Path]:
    base = root / "src/main/java"
    if base.exists():
        return sorted(base.rglob(f"*{suffix}"))
    files: list[Path] = []
    for module in root.iterdir():
        if not module.is_dir() or module.name.startswith("."):
            continue
        nested = module / "src/main/java"
        if nested.exists():
            files.extend(sorted(nested.rglob(f"*{suffix}")))
    return files


def iter_markdown_files(root: Path) -> list[Path]:
    ignored = {".git", ".idea", ".vscode", "node_modules", "target", "build", ".ai"}
    files: list[Path] = []
    for path in root.rglob("*.md"):
        if any(part in ignored or part.startswith(".") for part in path.parts):
            continue
        files.append(path)
    return sorted(files)


def collect_doc_titles(root: Path) -> list[tuple[str, str]]:
    titles: list[tuple[str, str]] = []
    for path in iter_markdown_files(root):
        text = path.read_text(encoding="utf-8", errors="ignore")
        first_heading = ""
        for line in text.splitlines():
            stripped = line.strip()
            if stripped.startswith("#"):
                first_heading = stripped.lstrip("#").strip()
                break
        title = first_heading or path.stem
        titles.append((str(path.relative_to(root)), title))
    return titles


def tokenize_name(value: str) -> set[str]:
    cleaned = []
    current = []
    for ch in value:
        if ch.isalnum():
            current.append(ch.lower())
        else:
            if current:
                cleaned.append("".join(current))
                current = []
    if current:
        cleaned.append("".join(current))
    tokens = set(cleaned)
    merged = "".join(cleaned)
    if merged:
        tokens.add(merged)
    return {token for token in tokens if len(token) >= 3}


def split_identifier(value: str) -> list[str]:
    text = value.replace("-", " ").replace("_", " ")
    expanded: list[str] = []
    current = []
    for ch in text:
        if ch.isupper() and current and current[-1].islower():
            expanded.append("".join(current))
            current = [ch]
        elif ch.isspace():
            if current:
                expanded.append("".join(current))
                current = []
        else:
            current.append(ch)
    if current:
        expanded.append("".join(current))
    tokens: list[str] = []
    for item in expanded:
        compact = "".join(ch for ch in item if ch.isalnum())
        if compact:
            tokens.append(compact.lower())
    return tokens


def normalize_domain_name(name: str) -> str:
    stop_words = {
        "controller",
        "service",
        "impl",
        "mapper",
        "entity",
        "model",
        "dto",
        "vo",
        "feign",
        "client",
        "convert",
    }
    tokens = [token for token in split_identifier(name) if token not in stop_words]
    if not tokens:
        lowered = "".join(ch.lower() for ch in name if ch.isalnum())
        return lowered or "project-overview"
    return "-".join(tokens[:6])


def humanize_domain_name(raw_name: str) -> str:
    tokens = [token for token in raw_name.split("-") if token]
    if not tokens:
        return "project overview"
    return " ".join(tokens[:6])


def summarize_domain(raw_name: str) -> str:
    return f"围绕 `{humanize_domain_name(raw_name)}` 相关代码与文档自动聚合出的候选业务域。"


def build_domain_terms(key: str, evidence: dict[str, list[str]], doc_titles: list[tuple[str, str]]) -> tuple[list[str], list[str]]:
    keyword_candidates: list[str] = []
    alias_candidates: list[str] = [key, key.replace("-", ""), humanize_domain_name(key)]

    for source in (*evidence["controller"], *evidence["service"], *evidence["model"]):
        stem = Path(source).stem
        alias_candidates.append(stem)
        alias_candidates.extend(split_identifier(stem))
    for rel in evidence["docs"]:
        alias_candidates.append(rel)
        title = next((title for path, title in doc_titles if path == rel), "")
        if title:
            keyword_candidates.append(title)
            alias_candidates.extend(split_identifier(title))
        alias_candidates.extend(split_identifier(Path(rel).stem))

    for token in split_identifier(key):
        keyword_candidates.append(token)
    keyword_candidates.extend(evidence["docs"])

    def unique(items: list[str]) -> list[str]:
        seen: set[str] = set()
        result: list[str] = []
        for item in items:
            cleaned = item.strip()
            compact = "".join(ch for ch in cleaned.lower() if ch.isalnum())
            if not cleaned or len(compact) < 2 or compact in seen:
                continue
            seen.add(compact)
            result.append(cleaned)
        return result

    return unique(keyword_candidates)[:12], unique(alias_candidates)[:12]


def infer_business_domains(root: Path) -> list[BusinessDomain]:
    controller_files = iter_java_files(root, "Controller.java")
    doc_titles = collect_doc_titles(root)
    buckets: dict[str, dict[str, list[str]]] = {}
    def ensure_bucket(name: str) -> dict[str, list[str]]:
        return buckets.setdefault(
            name,
            {
                "controller": [],
                "service": [],
                "model": [],
                "docs": [],
            },
        )
    for path in controller_files:
        stem = path.stem.removesuffix("Controller")
        key = normalize_domain_name(stem[:1].lower() + stem[1:] if stem else path.stem)
        ensure_bucket(key)["controller"].append(str(path.relative_to(root)))
    for path in iter_java_files(root, "Service.java"):
        key = normalize_domain_name(path.stem.removesuffix("Service"))
        ensure_bucket(key)["service"].append(str(path.relative_to(root)))
    for path in iter_java_files(root, ".java"):
        stem = path.stem
        if stem.endswith(("Controller", "Service", "ServiceImpl", "Mapper", "FeignClient", "Convert")):
            continue
        key = normalize_domain_name(stem)
        if key != "project-overview":
            ensure_bucket(key)["model"].append(str(path.relative_to(root)))
    for rel, title in doc_titles:
        doc_tokens = tokenize_name(title) | tokenize_name(rel)
        for key in list(buckets.keys()) or ["project-overview"]:
            if key == "project-overview":
                continue
            key_tokens = tokenize_name(key)
            if key_tokens & doc_tokens:
                ensure_bucket(key)["docs"].append(rel)
    domains: list[BusinessDomain] = []
    ranked = sorted(
        buckets.items(),
        key=lambda item: (
            -(len(item[1]["controller"]) * 4 + len(item[1]["service"]) * 3 + len(item[1]["model"]) + len(item[1]["docs"]) * 2),
            item[0],
        ),
    )
    for key, evidence in ranked[:8]:
        score = len(evidence["controller"]) * 4 + len(evidence["service"]) * 3 + len(evidence["model"]) + len(evidence["docs"]) * 2
        confidence = "high" if score >= 8 else "medium" if score >= 4 else "low"
        merged_evidence = evidence["controller"][:2] + evidence["service"][:2] + evidence["model"][:2]
        keywords, aliases = build_domain_terms(key, evidence, doc_titles)
        domains.append(
            BusinessDomain(
                name=key,
                summary=summarize_domain(key),
                confidence=confidence,
                evidence=merged_evidence[:6],
                doc_evidence=evidence["docs"][:4],
                code_locations=(evidence["controller"][:2] + evidence["service"][:2] + evidence["model"][:2])[:6],
                keywords=keywords,
                aliases=aliases,
            )
        )
    if not domains:
        domains.append(
            BusinessDomain(
                name="project-overview",
                summary="未识别到明显业务域，建议结合现有文档补充。",
                confidence="low",
                evidence=[],
                doc_evidence=[item[0] for item in doc_titles[:4]],
                code_locations=[],
                keywords=["项目概览", "系统概览"],
                aliases=["projectoverview", "project-overview"],
            )
        )
    return domains


def extract_request_mapping(text: str) -> str:
    for marker in ('@RequestMapping("', '@RequestMapping("/', "@RequestMapping(value = \""):
        if marker in text:
            fragment = text.split(marker, 1)[1]
            return "/" + fragment.split('"', 1)[0].lstrip("/")
    return ""


def extract_endpoint_lines(text: str) -> list[tuple[str, str]]:
    endpoints: list[tuple[str, str]] = []
    for line in text.splitlines():
        stripped = line.strip()
        for marker in ("@GetMapping(", "@PostMapping(", "@PutMapping(", "@DeleteMapping("):
            if marker in stripped:
                value = stripped.split(marker, 1)[1].split(")", 1)[0].strip().strip('"')
                endpoints.append((marker[1:-1], value if value else "/"))
    return endpoints


def extract_request_response_objects(text: str) -> tuple[str, str]:
    request_object = ""
    response_object = ""
    for line in text.splitlines():
        stripped = line.strip()
        if "public " in stripped and "(" in stripped and ")" in stripped:
            signature = stripped
            response_object = signature.split("public ", 1)[1].split(" ", 1)[0].strip()
            params = signature.split("(", 1)[1].split(")", 1)[0]
            for piece in params.split(","):
                piece = piece.strip()
                if not piece:
                    continue
                token = piece.split()[-2] if len(piece.split()) >= 2 else piece.split()[-1]
                if token[0].isupper():
                    request_object = token
                    break
            break
    return request_object or "(unknown)", response_object or "(unknown)"


def infer_api_chains(root: Path, domains: list[BusinessDomain]) -> list[ApiChain]:
    domain_names = {domain.name.lower(): domain.name for domain in domains}
    chains: list[ApiChain] = []
    for path in iter_java_files(root, "Controller.java"):
        text = path.read_text(encoding="utf-8", errors="ignore")
        request_root = extract_request_mapping(text)
        service_name = ""
        mapper_name = ""
        downstream = ""
        request_object, response_object = extract_request_response_objects(text)
        for line in text.splitlines():
            stripped = line.strip()
            if stripped.startswith("private final ") and stripped.endswith("Service;"):
                service_name = stripped.split()[-1].rstrip(";")
                break
        service_hint = path.stem.removesuffix("Controller")
        if not service_name:
            service_name = service_hint[:1].lower() + service_hint[1:] + "Service" if service_hint else ""
        service_impl = next((item for item in iter_java_files(root, "ServiceImpl.java") if service_hint in item.stem), None)
        if service_impl:
            service_text = service_impl.read_text(encoding="utf-8", errors="ignore")
            for line in service_text.splitlines():
                stripped = line.strip()
                if stripped.startswith("private final ") and stripped.endswith("Mapper;"):
                    mapper_name = stripped.split()[-1].rstrip(";")
                    break
                if "@FeignClient" in stripped or "FeignClient" in stripped:
                    downstream = "feign-client"
            if not downstream and "FeignClient" in service_text:
                downstream = "feign-client"
        matched_domain = next((value for key, value in domain_names.items() if key in service_hint.lower()), service_hint or "unknown")
        for method, endpoint in extract_endpoint_lines(text)[:4]:
            tags = []
            lowered = endpoint.lower()
            if any(item in lowered for item in ("export", "download")):
                tags.append("export")
            if any(item in lowered for item in ("add", "create", "save", "edit", "finish", "verify")):
                tags.append("write")
            if any(item in lowered for item in ("page", "list", "get")):
                tags.append("query")
            if any(item in lowered for item in ("batch", "sync")):
                tags.append("batch")
            if any(item in lowered for item in ("verify", "check", "risk")):
                tags.append("risk")
            if any(item in lowered for item in ("finish", "settlement", "transfer")):
                tags.append("flow")
            chains.append(
                ApiChain(
                    domain=matched_domain,
                    endpoint=f"{method} {request_root}{endpoint}",
                    controller=str(path.relative_to(root)),
                    service=service_name or "(unknown)",
                    mapper_or_table=mapper_name or "(unknown)",
                    downstream=downstream or "(none)",
                    request_object=request_object,
                    response_object=response_object,
                    confidence="medium" if mapper_name or service_name else "low",
                    tags=tags or ["general"],
                    evidence=[str(path.relative_to(root))] + ([str(service_impl.relative_to(root))] if service_impl else []),
                )
            )
    return chains[:20]


def collect_legacy_assets(root: Path) -> dict[str, list[str]]:
    groups = {
        "root_docs": ["PROJECT_CONTEXT.md", "BUSINESS_LOGIC.md", "API_REFERENCE.md", "ARCHITECTURE.md"],
        "legacy_governance": ["doc/开发协作约定.md", "doc/requirements/需求池.md", "doc/requirements/任务看板.md"],
        "legacy_prd": ["doc/PRD"],
    }
    result: dict[str, list[str]] = {}
    for key, candidates in groups.items():
        found = [candidate for candidate in candidates if (root / candidate).exists()]
        if found:
            result[key] = found
    return result


def build_migration_recommendations(workflow_state: WorkflowState, legacy_assets: dict[str, list[str]]) -> list[str]:
    recommendations: list[str] = []
    if workflow_state.state in {"legacy_bootstrap", "mixed_legacy_current"}:
        recommendations.append("优先以 `docs/workflow/` 作为后续治理主源，旧 `doc/` 目录先保留兼容。")
    if legacy_assets.get("legacy_governance"):
        recommendations.append("将旧治理文档建立索引到 `docs/workflow/`，暂不直接删除原文件。")
    if legacy_assets.get("legacy_prd"):
        recommendations.append("将旧 PRD 目录映射到 `docs/workflow/PRD/`，新需求统一从新路径进入。")
    if legacy_assets.get("root_docs"):
        recommendations.append("根目录历史分析文档建议保留原位，并在 `legacy-analysis.md` 中作为证据引用。")
    if not recommendations:
        recommendations.append("当前仓库已接近标准结构，主要执行补齐与刷新。")
    return recommendations


def build_legacy_scan_payload(root: Path, detection: Detection, workflow_state: WorkflowState) -> dict[str, object]:
    domains = infer_business_domains(root)
    chains = infer_api_chains(root, domains)
    legacy_assets = collect_legacy_assets(root)
    doc_titles = collect_doc_titles(root)
    risks: list[str] = []
    if workflow_state.state in {"legacy_bootstrap", "mixed_legacy_current"}:
        risks.append("检测到旧版 workflow 资产，需兼容迁移并避免覆盖。")
    if not detection.compile_cmd:
        risks.append("未自动识别到默认编译命令。")
    if not detection.test_cmd:
        risks.append("未自动识别到默认测试命令。")
    if len(chains) == 0:
        risks.append("未识别到关键接口链路，需结合文档和人工补充。")
    return {
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "workflow_state": asdict(workflow_state),
        "detection": asdict(detection),
        "legacy_assets": legacy_assets,
        "doc_inventory": [{"path": path, "title": title} for path, title in doc_titles[:40]],
        "business_domains": [asdict(item) for item in domains],
        "key_api_chains": [asdict(item) for item in chains],
        "migration_recommendations": build_migration_recommendations(workflow_state, legacy_assets),
        "risks": risks,
        "manual_confirmations": [
            "低置信度业务域需后续需求执行时持续修正。",
            "旧文档与新底座并存时，以 docs/workflow 为后续主源。",
            "查询与导出链路是否共用同一口径，建议后续需求执行时重点确认。"
        ],
    }


def render_legacy_analysis(payload: dict[str, object]) -> str:
    state = payload["workflow_state"]
    detection = payload["detection"]
    domains = payload["business_domains"]
    chains = payload["key_api_chains"]
    legacy_assets = payload["legacy_assets"]
    doc_inventory = payload["doc_inventory"]
    migration_recommendations = payload["migration_recommendations"]
    risks = payload["risks"]
    lines = [
        "# Legacy Analysis",
        "",
        "## 1. 接入结论",
        f"- workflow 状态：`{state['state']}`",
        f"- 当前底座分数：`{state['current_score']}`",
        f"- 旧底座分数：`{state['legacy_score']}`",
        f"- 建议动作：{'；'.join(state['recommended_actions'])}",
        "",
        "## 2. 仓库画像",
        f"- 项目名称：`{detection['project_name']}`",
        f"- 语言 / 构建：`{detection['language'] or 'unknown'}` / `{detection['build_tool'] or 'unknown'}`",
        f"- 代码主目录：{', '.join(f'`{item}`' for item in detection['source_dirs']) or '待补充'}",
        f"- 模块目录：{', '.join(f'`{item}`' for item in detection['module_dirs']) or '待补充'}",
        f"- 默认编译命令：`{detection['compile_cmd'] or '待补充'}`",
        f"- 默认测试命令：`{detection['test_cmd'] or '待补充'}`",
        "",
        "## 3. 治理画像",
    ]
    if legacy_assets:
        for key, items in legacy_assets.items():
            lines.append(f"- {key}：{', '.join(f'`{item}`' for item in items)}")
    else:
        lines.append("- 未检测到明显旧版治理资产。")
    lines.extend(["", "## 4. 业务画像"])
    for domain in domains:
        lines.append(f"- `{domain['name']}`：{domain['summary']}（confidence=`{domain['confidence']}`）")
        if domain["evidence"]:
            lines.append(f"  代码证据：{', '.join(f'`{item}`' for item in domain['evidence'])}")
        if domain["doc_evidence"]:
            lines.append(f"  文档证据：{', '.join(f'`{item}`' for item in domain['doc_evidence'])}")
    lines.extend(["", "## 5. 关键接口链路画像"])
    if chains:
        for chain in chains[:12]:
            lines.append(
                f"- `{chain['endpoint']}` -> `{chain['service']}` -> `{chain['mapper_or_table']}` / `{chain['downstream']}` "
                f"[domain=`{chain['domain']}` req=`{chain['request_object']}` resp=`{chain['response_object']}` "
                f"tags=`{','.join(chain['tags'])}` confidence=`{chain['confidence']}`]"
            )
            if chain["evidence"]:
                lines.append(f"  证据：{', '.join(f'`{item}`' for item in chain['evidence'])}")
    else:
        lines.append("- 未自动识别到关键接口链路。")
    lines.extend(["", "## 6. 文档证据概览"])
    if doc_inventory:
        for item in doc_inventory[:12]:
            lines.append(f"- `{item['path']}`：{item['title']}")
    else:
        lines.append("- 未发现可用 Markdown 文档证据。")
    lines.extend(["", "## 7. 迁移建议"])
    for item in migration_recommendations:
        lines.append(f"- {item}")
    lines.extend(["", "## 8. 风险与待确认项"])
    if risks:
        for risk in risks:
            lines.append(f"- {risk}")
    else:
        lines.append("- 当前未发现高风险项。")
    for item in payload["manual_confirmations"]:
        lines.append(f"- 待确认：{item}")
    lines.append("")
    return "\n".join(lines)


def render_project_context(d: Detection, workflow_state: WorkflowState | None = None, payload: dict[str, object] | None = None) -> str:
    tech = []
    if d.language == "java":
        tech.append("Java")
    elif d.language == "python":
        tech.append("Python")
    elif d.language == "javascript":
        tech.append("JavaScript/TypeScript")
    if d.build_tool == "maven":
        tech.append("Maven")
    elif d.build_tool == "gradle":
        tech.append("Gradle")
    elif d.build_tool == "npm":
        tech.append("npm")
    elif d.build_tool in {"pyproject", "pip"}:
        tech.append("Python packaging")
    stack_line = "、".join(tech) if tech else "待补充"
    source_line = "、".join(f"`{item}`" for item in d.source_dirs) if d.source_dirs else "待补充"
    module_line = "、".join(f"`{item}`" for item in d.module_dirs) if d.module_dirs else "待补充"
    hint_line = "、".join(f"`{item}`" for item in d.code_hints) if d.code_hints else "待补充"
    workflow_line = workflow_state.state if workflow_state else "unknown"
    domain_line = "待补充"
    chain_line = "待补充"
    if payload:
        domains = payload.get("business_domains", [])
        chains = payload.get("key_api_chains", [])
        if domains:
            domain_line = "、".join(f"`{item['name']}`" for item in domains[:6])
        if chains:
            chain_line = "、".join(f"`{item['endpoint']}`" for item in chains[:5])
    return f"""# PROJECT_CONTEXT

## 1. 作用
- 本文件是项目共享事实主文件，面向所有 AI 宿主共享。
- 只放项目事实，不放宿主专属提示。

## 2. 当前仓库事实
- 项目名称：`{d.project_name}`
- 主要技术栈：`{stack_line}`
- workflow 当前状态：`{workflow_line}`
- 正式需求治理目录：`{d.docs_root}/requirements/`
- PRD 目录：`{d.prd_directory}`
- 当前任务记忆目录：`.ai/memory/tasks/`
- 当前项目知识目录：`.ai/memory/knowledge/`
- workflow 状态骨架：`.ai/runtime/project-state.json`

## 3. 自动扫描结果
- 代码主目录：{source_line}
- 模块目录：{module_line}
- 常见分层提示：{hint_line}
- 主要业务域：{domain_line}
- 关键接口链路索引：{chain_line}

## 4. 当前协作事实
- 协作规则入口：`AGENTS.md`
- 详细约定：`docs/workflow/开发协作约定.md`
- 宿主补充目录：`.ai/governance/`
- workflow runtime profile：`.ai/runtime/profile/project-profile.yml`
- workflow 状态骨架：`.ai/runtime/project-state.json`
- 老项目详细画像：`docs/workflow/legacy-analysis.md`

## 5. 默认约束
- 正式治理材料长期保留在 `docs/workflow/requirements/`
- AI 记忆与运行态资产统一维护在 `.ai/`
"""


def render_profile(d: Detection) -> str:
    source_dirs = ", ".join(f'"{item}"' for item in d.source_dirs)
    modules = ", ".join(f'"{item}"' for item in d.module_dirs)
    return f"""version: "1.0"
project:
  name: "{d.project_name}"
  language: "{d.language}"
  build_tool: "{d.build_tool}"
paths:
  docs_root: "{d.docs_root}"
  requirements_pool: "{d.docs_root}/requirements/需求池.md"
  task_board: "{d.docs_root}/requirements/任务看板.md"
  prd_directory: "{d.prd_directory}"
  project_state: ".ai/runtime/project-state.json"
  readme_index: "README.md"
  tasks_index: ".ai/memory/tasks/index.md"
  tasks_template_dir: ".ai/memory/tasks/_template"
  knowledge_dir: ".ai/memory/knowledge"
project_layout:
  source_dirs: [{source_dirs}]
  module_dirs: [{modules}]
build:
  compile: "{d.compile_cmd}"
  test: "{d.test_cmd}"
  settings_file: ""
features:
  database: true
  mcp_database: true
  private_registry: false
  lightweight_mode: false
"""


def render_project_state(root: Path, d: Detection) -> str:
    payload = {
        "schemaVersion": "1.0",
        "project": {
            "name": d.project_name,
            "path": str(root),
            "language": d.language,
            "buildTool": d.build_tool,
            "docsRoot": d.docs_root,
            "prdDirectory": d.prd_directory,
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
    return json.dumps(payload, ensure_ascii=False, indent=2) + "\n"


def render_project_state_with_scan(root: Path, d: Detection, workflow_state: WorkflowState, payload: dict[str, object]) -> str:
    base = json.loads(render_project_state(root, d))
    base["workflow"]["health"] = "已扫描"
    base["workflow"]["risk"] = "中" if payload["risks"] else "低"
    base["sync"]["status"] = workflow_state.state
    base["sync"]["lastSyncAt"] = payload["generated_at"]
    base["evidence"] = [
        {"type": "business_domain", "name": item["name"], "confidence": item["confidence"]}
        for item in payload["business_domains"]
    ]
    base["risks"] = [{"level": "warning", "summary": item} for item in payload["risks"]]
    return json.dumps(base, ensure_ascii=False, indent=2) + "\n"


def render_governance(d: Detection) -> str:
    test_cmd_line = d.test_cmd or "待补充"
    compile_cmd_line = d.compile_cmd or "待补充"
    source_line = "、".join(f"`{item}`" for item in d.source_dirs) if d.source_dirs else "待补充"
    module_line = "、".join(f"`{item}`" for item in d.module_dirs) if d.module_dirs else "待补充"
    return f"""# 开发协作约定

## 1. 目的
统一 `{d.project_name}` 仓库的需求执行方式，确保每次任务都遵循同一套高约束流程，避免误改、漏验、带风险提交，并将可复用记忆沉淀为仓库事实。

## 2. 适用范围
- 适用于本仓库所有需求、缺陷、SQL 处理、性能优化和联调任务。
- 默认生效；若任务明确声明“临时豁免”，需先说明风险并得到确认。
- 适用于 workflow skill 执行回合，也适用于普通多轮 bugfix、逻辑调整和新窗口续做场景。

## 3. 自动识别的仓库事实
- 代码主目录：{source_line}
- 模块目录：{module_line}
- 正式治理目录：`{d.docs_root}/requirements/`
- PRD 目录：`{d.prd_directory}`
- 默认编译命令：`{compile_cmd_line}`
- 默认测试命令：`{test_cmd_line}`

## 4. 五条硬性约定（必须执行）

### 4.1 先分析后改
- 禁止直接改代码。
- 首次输出必须包含：
- 涉及文件/类/方法
- SQL 或接口链路
- 调用链与影响范围
- 根因结论与备选方案（如有）

### 4.2 先检索历史
- 开始任务前必须优先检索以下内容：
- `AGENTS.md`
- `docs/workflow/PROJECT_CONTEXT.md`
- 本文档
- `{d.docs_root}/requirements/需求池.md`
- `{d.docs_root}/requirements/任务看板.md`
- `.ai/memory/tasks/index.md`
- 命中的 `.ai/memory/tasks/**`
- 命中的 `.ai/memory/knowledge/**`

### 4.3 边界锁定
- 只允许修改“改动范围”中明确列出的模块/链路。
- 禁止顺手改“正常逻辑”或做无关重构。

### 4.4 验证闭环
- 改动后必须执行最小可行验证，至少编译通过。
- 输出必须包含：
- 验证命令
- 验证结果
- 未覆盖风险

### 4.5 沉淀闭环
- 不允许只解决问题不留痕。
- 统一把任务记忆写入 `.ai/memory/tasks/`
- 统一把项目知识写入 `.ai/memory/knowledge/`

## 5. 标准执行流程
`检索历史 -> 分析 -> 改动 -> 验证 -> 沉淀 -> 提交`

## 6. 目录约定
- 共享项目事实：`docs/workflow/PROJECT_CONTEXT.md`
- 宿主补充：`.ai/governance/`
- 任务索引：`.ai/memory/tasks/index.md`
- 任务模板：`.ai/memory/tasks/_template/`
- 项目知识：`.ai/memory/knowledge/`
- runtime profile：`.ai/runtime/profile/project-profile.yml`
- workflow 状态骨架：`.ai/runtime/project-state.json`
- 正式需求治理：`{d.docs_root}/requirements/`
- PRD 目录：`{d.prd_directory}`

## 7. 与 Workflow Skill 的结合方式
- `workflow-bootstrap`：负责初始化和迁移协作底座
- `workflow-requirement`：负责 `PRD -> 需求池 -> 任务看板 -> 交接材料`
- `workflow-execution`：负责 `开干 -> 实现 -> 验证 -> 提交`
"""


def write_file(path: Path, content: str, dry_run: bool, force: bool = False) -> str:
    existed = path.exists()
    if existed and not force:
        return "skipped"
    if not dry_run:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
    return "created" if (not existed or force) else "skipped"


def write_executable_file(path: Path, content: str, dry_run: bool, force: bool = False) -> str:
    status = write_file(path, content, dry_run, force=force)
    if not dry_run and path.exists():
        os.chmod(path, 0o755)
    return status


def ensure_gitkeep(path: Path, dry_run: bool) -> str:
    if path.exists():
        return "skipped"
    if not dry_run:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("", encoding="utf-8")
    return "created"


def move_entry(src: Path, dst: Path, dry_run: bool) -> str:
    if not src.exists():
        return "missing"
    if dst.exists():
        return "skipped"
    if not dry_run:
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(src), str(dst))
    return "migrated"


def cleanup_empty_dir(path: Path, dry_run: bool) -> str:
    if not path.exists() or not path.is_dir():
        return "missing"
    if any(path.iterdir()):
        return "skipped"
    if not dry_run:
        path.rmdir()
    return "removed"


def replace_in_markdown(root: Path, old: str, new: str, dry_run: bool) -> int:
    replacements = 0
    ignored = {".git", ".idea", ".vscode", "node_modules", "target", "build"}
    for path in root.rglob("*.md"):
        if any(part in ignored for part in path.parts):
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        if old not in text:
            continue
        replacements += text.count(old)
        if not dry_run:
            path.write_text(text.replace(old, new), encoding="utf-8")
    return replacements


def migrate_legacy_memory(root: Path, dry_run: bool) -> list[tuple[str, str]]:
    actions: list[tuple[str, str]] = []
    legacy_tasks = root / "doc/tasks"
    legacy_knowledge = root / "doc/knowledge"
    target_tasks = root / ".ai/memory/tasks"
    target_knowledge = root / ".ai/memory/knowledge"

    if legacy_tasks.exists():
        for child in sorted(legacy_tasks.iterdir()):
            if child.name == "_template":
                for tpl in sorted(child.iterdir()):
                    dst = target_tasks / "_template" / tpl.name
                    actions.append((f"migrate:{child.name}/{tpl.name}", move_entry(tpl, dst, dry_run)))
            elif child.name == "index.md":
                actions.append(("migrate:tasks/index.md", move_entry(child, target_tasks / "index.md", dry_run)))
            else:
                actions.append((f"migrate:tasks/{child.name}", move_entry(child, target_tasks / child.name, dry_run)))
        actions.append(("cleanup:doc/tasks/_template", cleanup_empty_dir(legacy_tasks / "_template", dry_run)))
        actions.append(("cleanup:doc/tasks", cleanup_empty_dir(legacy_tasks, dry_run)))

    if legacy_knowledge.exists():
        for child in sorted(legacy_knowledge.iterdir()):
            actions.append((f"migrate:knowledge/{child.name}", move_entry(child, target_knowledge / child.name, dry_run)))
        actions.append(("cleanup:doc/knowledge", cleanup_empty_dir(legacy_knowledge, dry_run)))

    replacements = 0
    replacements += replace_in_markdown(root, "doc/tasks", ".ai/memory/tasks", dry_run)
    replacements += replace_in_markdown(root, "doc/knowledge", ".ai/memory/knowledge", dry_run)
    if replacements:
        actions.append(("rewrite:markdown-paths", f"updated:{replacements}"))
    return actions


def self_check(root: Path, d: Detection) -> tuple[list[str], list[str]]:
    warnings: list[str] = []
    infos: list[str] = []
    required = [
        root / "AGENTS.md",
        root / "docs/workflow/PROJECT_CONTEXT.md",
        root / "docs/workflow/开发协作约定.md",
        root / ".ai/memory/tasks/index.md",
        root / ".ai/memory/knowledge/README.md",
        root / ".ai/runtime/profile/project-profile.yml",
        root / ".ai/runtime/project-state.json",
        root / ".ai/runtime/inbox/.gitkeep",
        root / ".ai/runtime/conversations/.gitkeep",
        root / f"{d.docs_root}/requirements/需求池.md",
        root / f"{d.docs_root}/requirements/任务看板.md",
    ]
    missing = [str(path.relative_to(root)) for path in required if not path.exists()]
    if missing:
        warnings.append(f"missing_required={', '.join(missing)}")
    else:
        infos.append("required_files=ok")

    if not d.compile_cmd:
        warnings.append("compile_command=empty")
    else:
        infos.append(f"compile_command={d.compile_cmd}")
    if not d.test_cmd:
        warnings.append("test_command=empty")
    else:
        infos.append(f"test_command={d.test_cmd}")
    if not d.source_dirs:
        warnings.append("source_dirs=none_detected")
    else:
        infos.append(f"source_dirs={', '.join(d.source_dirs)}")
    if not d.prd_directory:
        warnings.append("prd_directory=empty")
    return warnings, infos


def main() -> int:
    parser = argparse.ArgumentParser(description="Initialize workflow + memory bootstrap scaffold")
    parser.add_argument("--workspace-root", default=".")
    parser.add_argument("--host", action="append", choices=("codex", "claude"), default=[])
    parser.add_argument("--force-host-files", action="store_true")
    parser.add_argument("--force-context", action="store_true")
    parser.add_argument("--force-governance", action="store_true")
    parser.add_argument("--force-profile", action="store_true")
    parser.add_argument("--skip-legacy-migration", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    root = Path(args.workspace_root).resolve()
    hosts = args.host or ["codex"]
    detection = collect_detection(root)
    workflow_state = detect_workflow_state(root)
    legacy_scan = build_legacy_scan_payload(root, detection, workflow_state)

    actions: list[tuple[str, str]] = []
    actions.append(("AGENTS.md", write_file(root / "AGENTS.md", ROOT_AGENTS, args.dry_run)))
    actions.append(
        (
            "docs/workflow/PROJECT_CONTEXT.md",
            write_file(
                root / "docs/workflow/PROJECT_CONTEXT.md",
                render_project_context(detection, workflow_state, legacy_scan),
                args.dry_run,
                force=args.force_context or workflow_state.state in {"legacy_bootstrap", "mixed_legacy_current", "partial_current"},
            ),
        )
    )
    actions.append(
        (
            "docs/workflow/开发协作约定.md",
            write_file(root / "docs/workflow/开发协作约定.md", render_governance(detection), args.dry_run, force=args.force_governance),
        )
    )

    for host in hosts:
        host_path = root / ".ai" / "governance" / f"{host}-host.md"
        actions.append((str(host_path.relative_to(root)), write_file(host_path, HOST_TEMPLATES[host], args.dry_run, force=args.force_host_files)))

    if not args.skip_legacy_migration:
        actions.extend(migrate_legacy_memory(root, args.dry_run))

    files = {
        ".ai/bin/workflow": WORKFLOW_BIN,
        ".ai/bin/wf-init": render_short_bin("init"),
        ".ai/bin/wf-doctor": render_short_bin("doctor"),
        ".ai/bin/wf-cons": render_short_bin("cons"),
        ".ai/bin/wf-req": render_short_bin("req"),
        ".ai/bin/wf-exec": render_short_bin("exec"),
        ".ai/bin/wf-arc": render_short_bin("arc"),
        ".ai/memory/tasks/index.md": TASK_INDEX,
        ".ai/memory/tasks/_template/README.md": TASK_README,
        ".ai/memory/tasks/_template/inbox.md": TASK_INBOX,
        ".ai/memory/tasks/_template/issues.md": TASK_ISSUES,
        ".ai/memory/tasks/_template/decisions.md": TASK_DECISIONS,
        ".ai/memory/tasks/_template/verify.md": TASK_VERIFY,
        ".ai/memory/context-brief.md": CONTEXT_BRIEF,
        ".ai/memory/session-briefs/README.md": SESSION_BRIEF,
        ".ai/memory/knowledge/README.md": KNOWLEDGE_README,
        ".ai/runtime/profile/project-profile.yml": render_profile(detection),
        ".ai/runtime/project-state.json": render_project_state_with_scan(root, detection, workflow_state, legacy_scan),
        ".ai/runtime/cache/legacy-scan.json": json.dumps(legacy_scan, ensure_ascii=False, indent=2) + "\n",
        "docs/workflow/legacy-analysis.md": render_legacy_analysis(legacy_scan),
        f"{detection.docs_root}/requirements/需求池.md": REQUIREMENTS_POOL,
        f"{detection.docs_root}/requirements/任务看板.md": TASK_BOARD,
    }
    for rel, content in files.items():
        force = args.force_profile if rel.endswith("project-profile.yml") else False
        if rel in {".ai/runtime/project-state.json", ".ai/runtime/cache/legacy-scan.json", "docs/workflow/legacy-analysis.md"}:
            force = True
        writer = write_executable_file if rel.startswith(".ai/bin/") else write_file
        actions.append((rel, writer(root / rel, content, args.dry_run, force=force)))

    actions.append((".ai/runtime/cache/.gitkeep", ensure_gitkeep(root / ".ai/runtime/cache/.gitkeep", args.dry_run)))
    actions.append((".ai/runtime/conversations/.gitkeep", ensure_gitkeep(root / ".ai/runtime/conversations/.gitkeep", args.dry_run)))
    actions.append((".ai/runtime/inbox/.gitkeep", ensure_gitkeep(root / ".ai/runtime/inbox/.gitkeep", args.dry_run)))
    actions.append((".ai/runtime/state/.gitkeep", ensure_gitkeep(root / ".ai/runtime/state/.gitkeep", args.dry_run)))
    actions.append((".ai/memory/tasks/archived/.gitkeep", ensure_gitkeep(root / ".ai/memory/tasks/archived/.gitkeep", args.dry_run)))
    actions.append((".ai/memory/session-briefs/.gitkeep", ensure_gitkeep(root / ".ai/memory/session-briefs/.gitkeep", args.dry_run)))
    actions.append((f"{detection.prd_directory}/.gitkeep", ensure_gitkeep(root / detection.prd_directory / ".gitkeep", args.dry_run)))

    warnings, infos = self_check(root, detection)

    print("# Workflow Bootstrap")
    print(f"- workspace_root: {root}")
    print(f"- mode: {'dry-run' if args.dry_run else 'live'}")
    print(f"- hosts: {', '.join(hosts)}")
    print(f"- detected_language: {detection.language or '(unknown)'}")
    print(f"- detected_build_tool: {detection.build_tool or '(unknown)'}")
    print(f"- detected_docs_root: {detection.docs_root}")
    print(f"- detected_prd_directory: {detection.prd_directory}")
    print(f"- detected_compile: {detection.compile_cmd or '(empty)'}")
    print(f"- detected_test: {detection.test_cmd or '(empty)'}")
    print(f"- detected_source_dirs: {', '.join(detection.source_dirs) or '(none)'}")
    print(f"- detected_module_dirs: {', '.join(detection.module_dirs) or '(none)'}")
    print(f"- detected_code_hints: {', '.join(detection.code_hints) or '(none)'}")
    print(f"- workflow_state: {workflow_state.state}")
    print(f"- workflow_current_score: {workflow_state.current_score}")
    print(f"- workflow_legacy_score: {workflow_state.legacy_score}")
    print(f"- detected_business_domains: {len(legacy_scan['business_domains'])}")
    print(f"- detected_key_api_chains: {len(legacy_scan['key_api_chains'])}")
    print(f"- recommended_actions: {'; '.join(workflow_state.recommended_actions)}")
    for rel, status in actions:
        print(f"- {status}: {rel}")
    for info in infos:
        print(f"- selfcheck-info: {info}")
    for warning in warnings:
        print(f"- selfcheck-warning: {warning}")
    print("- next: 如自动识别结果可接受，可直接继续用 workflow-requirement / workflow-execution")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
