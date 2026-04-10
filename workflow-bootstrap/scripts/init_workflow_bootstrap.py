#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
from dataclasses import dataclass
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

- `doc/开发协作约定.md`
- `PROJECT_CONTEXT.md`
- `.ai/governance/`
- `.ai/memory/`

## Workflow Chat Aliases

- `wf-init` -> `workflow-bootstrap`
- `wf-doctor` -> `workflow-bootstrap` health check
- `wf-req` -> `workflow-requirement`
- `wf-exec` -> `workflow-execution`
- `wf-arc` -> `workflow-execution` archive
"""

HOST_TEMPLATES = {
    "codex": """# Codex Host

## 1. 作用
- 本文件仅提供 Codex 宿主专属补充。
- 共享项目事实统一读取根目录 `PROJECT_CONTEXT.md`。
- 仓库协作规则统一读取根目录 `AGENTS.md`。

## 2. Codex 宿主补充
- 默认先查 `.ai/memory/tasks/index.md` 与 `.ai/memory/knowledge/`。
- 正式治理主源固定为 `doc/requirements/`。
""",
    "claude": """# Claude Host

## 1. 作用
- 本文件仅提供 Claude 宿主专属补充。
- 共享项目事实统一读取根目录 `PROJECT_CONTEXT.md`。
- 仓库协作规则统一读取根目录 `AGENTS.md`。

## 2. Claude 宿主补充
- 默认先查 `.ai/memory/tasks/index.md` 与 `.ai/memory/knowledge/`。
- 正式治理主源固定为 `doc/requirements/`。
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
    return "doc/PRD"


def detect_docs_root(root: Path) -> str:
    if (root / "doc").exists():
        return "doc"
    if (root / "docs").exists():
        return "docs"
    return "doc"


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


def render_project_context(d: Detection) -> str:
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
    return f"""# PROJECT_CONTEXT

## 1. 作用
- 本文件是项目共享事实主文件，面向所有 AI 宿主共享。
- 只放项目事实，不放宿主专属提示。

## 2. 当前仓库事实
- 项目名称：`{d.project_name}`
- 主要技术栈：`{stack_line}`
- 正式需求治理目录：`{d.docs_root}/requirements/`
- PRD 目录：`{d.prd_directory}`
- 当前任务记忆目录：`.ai/memory/tasks/`
- 当前项目知识目录：`.ai/memory/knowledge/`
- workflow 状态骨架：`.ai/runtime/project-state.json`

## 3. 自动扫描结果
- 代码主目录：{source_line}
- 模块目录：{module_line}
- 常见分层提示：{hint_line}

## 4. 当前协作事实
- 协作规则入口：`AGENTS.md`
- 详细约定：`doc/开发协作约定.md`
- 宿主补充目录：`.ai/governance/`
- workflow runtime profile：`.ai/runtime/profile/project-profile.yml`
- workflow 状态骨架：`.ai/runtime/project-state.json`

## 5. 默认约束
- 正式治理材料长期保留在 `doc/requirements/`
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
        },
        "metrics": {
            "totalTasks": 0,
            "doing": 0,
            "blocked": 0,
            "review": 0,
            "done": 0,
            "evidenceCoverage": 0,
        },
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
- `PROJECT_CONTEXT.md`
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
- 共享项目事实：`PROJECT_CONTEXT.md`
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
        root / "PROJECT_CONTEXT.md",
        root / "doc/开发协作约定.md",
        root / ".ai/memory/tasks/index.md",
        root / ".ai/memory/knowledge/README.md",
        root / ".ai/runtime/profile/project-profile.yml",
        root / ".ai/runtime/project-state.json",
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

    actions: list[tuple[str, str]] = []
    actions.append(("AGENTS.md", write_file(root / "AGENTS.md", ROOT_AGENTS, args.dry_run)))
    actions.append(
        (
            "PROJECT_CONTEXT.md",
            write_file(root / "PROJECT_CONTEXT.md", render_project_context(detection), args.dry_run, force=args.force_context),
        )
    )
    actions.append(
        (
            "doc/开发协作约定.md",
            write_file(root / "doc/开发协作约定.md", render_governance(detection), args.dry_run, force=args.force_governance),
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
        ".ai/bin/wf-req": render_short_bin("req"),
        ".ai/bin/wf-exec": render_short_bin("exec"),
        ".ai/bin/wf-arc": render_short_bin("arc"),
        ".ai/memory/tasks/index.md": TASK_INDEX,
        ".ai/memory/tasks/_template/README.md": TASK_README,
        ".ai/memory/tasks/_template/inbox.md": TASK_INBOX,
        ".ai/memory/tasks/_template/issues.md": TASK_ISSUES,
        ".ai/memory/tasks/_template/decisions.md": TASK_DECISIONS,
        ".ai/memory/tasks/_template/verify.md": TASK_VERIFY,
        ".ai/memory/knowledge/README.md": KNOWLEDGE_README,
        ".ai/runtime/profile/project-profile.yml": render_profile(detection),
        ".ai/runtime/project-state.json": render_project_state(root, detection),
        f"{detection.docs_root}/requirements/需求池.md": REQUIREMENTS_POOL,
        f"{detection.docs_root}/requirements/任务看板.md": TASK_BOARD,
    }
    for rel, content in files.items():
        force = args.force_profile if rel.endswith("project-profile.yml") else False
        writer = write_executable_file if rel.startswith(".ai/bin/") else write_file
        actions.append((rel, writer(root / rel, content, args.dry_run, force=force)))

    actions.append((".ai/runtime/cache/.gitkeep", ensure_gitkeep(root / ".ai/runtime/cache/.gitkeep", args.dry_run)))
    actions.append((".ai/runtime/state/.gitkeep", ensure_gitkeep(root / ".ai/runtime/state/.gitkeep", args.dry_run)))
    actions.append((".ai/memory/tasks/archived/.gitkeep", ensure_gitkeep(root / ".ai/memory/tasks/archived/.gitkeep", args.dry_run)))

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
