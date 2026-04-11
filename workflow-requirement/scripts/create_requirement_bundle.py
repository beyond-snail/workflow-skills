#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from datetime import date
from pathlib import Path

from cli_common import add_profile_arg, print_header
from sync_requirement_pool import sync_requirement_pool_entry
from sync_task_board import sync_task_board_entry


def build_names(doc_date: str, theme: str, design_prefix: str, breakdown_prefix: str) -> tuple[str, str]:
    return (
        f"{doc_date}-{design_prefix}-{theme}.md",
        f"{doc_date}-{breakdown_prefix}-{theme}.md",
    )


def write_if_absent(path: Path, content: str) -> bool:
    if path.exists():
        return False
    path.write_text(content, encoding="utf-8")
    return True


def design_body(doc_date: str, theme: str) -> str:
    return f"""# {theme} - 技术设计文档

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |

## 2. 背景与问题

## 3. 目标

## 4. 现状分析

## 5. 方案设计

## 6. 系统改造点

## 7. 验收口径

## 8. 风险与依赖
"""


def breakdown_body(doc_date: str, theme: str, design_rel: str) -> str:
    return f"""# {doc_date} 开发任务拆解 - {theme}

## 1. 对应需求

- 设计文档：`{design_rel}`

## 2. 总体原则

## 3. 任务拆解

### 3.1 子任务一

目标：

改动范围：

验收重点：

### 3.2 子任务二

目标：

改动范围：

验收重点：

## 4. 推荐执行顺序
"""


def detailed_design_body(doc_date: str, theme: str) -> str:
    return f"""# {doc_date} 详细开发设计 - {theme}

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |

## 2. 模块拆分

## 3. 数据流与时序

## 4. 核心对象设计

## 5. 接口与方法设计

## 6. SQL 与数据落库设计

## 7. 异常处理与回退策略

## 8. 测试与验证设计
"""


def physical_design_body(doc_date: str, theme: str) -> str:
    return f"""# {doc_date} 物理表设计 - {theme}

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |

## 2. 现有表扩展

| 表名 | 字段 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |

## 3. 新增表设计

| 表名 | 字段 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |

## 4. 唯一键与索引

## 5. 约束与备注
"""


def table_mapping_body(doc_date: str, theme: str) -> str:
    return f"""# {doc_date} 表名对照表 - {theme}

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |

## 2. PRD-物理表对照

| PRD名称 | 物理表名 | 说明 | 章节 |
| --- | --- | --- | --- |

## 3. 字段对照

| PRD字段 | 物理字段 | 说明 |
| --- | --- | --- |
"""


def prd_trace_body(doc_date: str, theme: str, prd_rel: str, design_rel: str) -> str:
    return f"""# {doc_date} PRD需求-设计追溯清单 - {theme}

## 文档信息

| 项目 | 内容 |
|------|------|
| 需求ID | |
| PRD文档 | `{prd_rel}` |
| 设计文档 | `{design_rel}` |
| 创建日期 | {doc_date} |
| 最后更新 | {doc_date} |

---

## 追溯清单

| PRD章节 | PRD需求描述 | 设计/代码/测试对应位置 | 完成状态 | 备注 |
|----------|-------------|------------------------|----------|------|

---

## 未实现需求说明

| PRD章节 | PRD需求描述 | 未实现原因 | 计划处理时间 |
|----------|-------------|------------|--------------|

---

## 确认结论

- [ ] 已建立 `PRD` 到设计/代码/测试的追溯关系
- [ ] 已明确列出当前仍未收口的 `PRD` 条款
- [ ] 仍需要按追溯清单继续补代码与测试证据
"""


def product_confirmation_body(doc_date: str, theme: str, prd_rel: str) -> str:
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

### 1. 待确认项一

说明：

确认：

### 2. 待确认项二

说明：

确认：

---

## 产品确认结果

| 编号 | 待确认项 | 产品结论 | 确认人 | 确认日期 |
|------|----------|----------|--------|----------|
| 1 | 待确认项一 |  |  |  |
| 2 | 待确认项二 |  |  |  |

---

## 结论

- [ ] 所有关键歧义已补充确认，可进入开发实现
- [ ] 仍有未确认项，开发前需继续冻结
"""


def impl_alignment_body(doc_date: str, theme: str, prd_rel: str) -> str:
    return f"""# {doc_date} 流程图与实现对齐 - {theme}

## 1. 结论

- 业务规则与口径：统一以 `{prd_rel}` 为准
- 核心开发：待补充
- 报表/异常视图/导出：待补充
- 最终业务验收：待补充

## 2. 月结主流程图

```mermaid
flowchart TD
    A[入口] --> B[待补充]
```

## 3. 方法与需求对齐表

| PRD编号 | 需求内容 | 代码入口/方法 | 当前状态 | 说明 |
|---|---|---|---|---|
"""


def sql_template_body(doc_date: str, theme: str, title: str) -> str:
    return f"""-- {doc_date} {title} - {theme}
-- 按实际需求补充 SQL
"""


def testing_doc_body(doc_date: str, theme: str, title: str) -> str:
    return f"""# {doc_date} {title} - {theme}

## 1. 目标

## 2. 环境信息

## 3. 执行步骤

## 4. 结果记录

## 5. 结论
"""


def scripts_readme_body(theme: str) -> str:
    return f"""# scripts

本目录用于放置 `{theme}` 相关的装载、校验、抽样与回填辅助脚本。
"""


def directory_note_body() -> str:
    return """# 目录说明

本目录按“设计文档 / SQL 脚本 / 测试材料 / 执行脚本”分层管理。

## design

放需求实现说明类文档：

1. 技术设计
2. 详细开发设计
3. 开发任务拆解
4. 物理表设计
5. 表名对照表
6. PRD追溯清单
7. 产品确认清单
8. 流程图与实现对齐说明

## sql

统一放 SQL 脚本，避免和说明文档混堆。

### sql/ddl

放结构类 SQL：

1. 初始 DDL
2. 字段修正
3. 索引修正
4. 主键序列补充

### sql/fix

放历史数据修复和补全脚本：

1. 自动回填脚本
2. 人工映射模板

### sql/testdata

放测试样本装载 SQL：

1. 测试样本初始化

## testing

放联调与测试结果：

1. 联调验收记录
2. 测试数据方案
3. 自动化测试结果
4. UAT测试用例

## scripts

放辅助执行脚本：

1. 样本装载脚本
2. 数据检查脚本
"""


def sync_readme(readme_path: Path, section: str, rel_paths: list[str]) -> None:
    if not readme_path.exists():
        return

    lines = readme_path.read_text(encoding="utf-8").splitlines()
    existing = set()
    for line in lines:
        text = line.strip()
        if text.startswith("- `") and text.endswith("`"):
            existing.add(text[3:-1])

    missing = [path for path in rel_paths if path not in existing]
    if not missing:
        return

    insert_idx = None
    for idx, line in enumerate(lines):
        if line.strip() == section:
            insert_idx = idx + 1
            break
    if insert_idx is None:
        return

    while insert_idx < len(lines) and lines[insert_idx].strip() == "":
        insert_idx += 1

    lines[insert_idx:insert_idx] = [f"- `{path}`" for path in missing]
    readme_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def create_layered_bundle(
    exec_dir: Path,
    rel_root: str,
    doc_date: str,
    theme: str,
    design_name: str,
    breakdown_name: str,
    prd_rel: str,
) -> list[tuple[str, str, bool]]:
    design_dir = exec_dir / "design"
    sql_ddl_dir = exec_dir / "sql" / "ddl"
    sql_fix_dir = exec_dir / "sql" / "fix"
    sql_testdata_dir = exec_dir / "sql" / "testdata"
    testing_dir = exec_dir / "testing"
    scripts_dir = exec_dir / "scripts"

    for path in (design_dir, sql_ddl_dir, sql_fix_dir, sql_testdata_dir, testing_dir, scripts_dir):
        path.mkdir(parents=True, exist_ok=True)

    directory_note_rel = f"{rel_root}/00-目录说明.md"
    design_rel = f"{rel_root}/design/{design_name}"
    detailed_design_name = f"{doc_date}-详细开发设计-{theme}.md"
    detailed_design_rel = f"{rel_root}/design/{detailed_design_name}"
    breakdown_rel = f"{rel_root}/design/{breakdown_name}"
    physical_design_name = f"{doc_date}-物理表设计-{theme}.md"
    physical_design_rel = f"{rel_root}/design/{physical_design_name}"
    table_mapping_name = f"{doc_date}-表名对照表-{theme}.md"
    table_mapping_rel = f"{rel_root}/design/{table_mapping_name}"
    prd_trace_name = f"{doc_date}-PRD追溯-{theme}.md"
    prd_trace_rel = f"{rel_root}/design/{prd_trace_name}"
    product_confirm_name = f"{doc_date}-产品确认清单-{theme}.md"
    product_confirm_rel = f"{rel_root}/design/{product_confirm_name}"
    impl_alignment_name = f"{doc_date}-流程图与实现对齐-{theme}.md"
    impl_alignment_rel = f"{rel_root}/design/{impl_alignment_name}"
    ddl_name = f"{doc_date}-DDL-{theme}.sql"
    ddl_rel = f"{rel_root}/sql/ddl/{ddl_name}"
    ddl_field_fix_name = f"{doc_date}-DDL-字段修正-{theme}.sql"
    ddl_field_fix_rel = f"{rel_root}/sql/ddl/{ddl_field_fix_name}"
    ddl_index_fix_name = f"{doc_date}-DDL-索引修正-{theme}.sql"
    ddl_index_fix_rel = f"{rel_root}/sql/ddl/{ddl_index_fix_name}"
    ddl_slim_name = f"{doc_date}-DDL-精简字段-{theme}.sql"
    ddl_slim_rel = f"{rel_root}/sql/ddl/{ddl_slim_name}"
    ddl_sequence_name = f"{doc_date}-DDL-主键序列-{theme}.sql"
    ddl_sequence_rel = f"{rel_root}/sql/ddl/{ddl_sequence_name}"
    fix_auto_name = f"{doc_date}-SQL-历史补全-{theme}.sql"
    fix_auto_rel = f"{rel_root}/sql/fix/{fix_auto_name}"
    fix_manual_name = f"{doc_date}-SQL-人工映射模板-{theme}.sql"
    fix_manual_rel = f"{rel_root}/sql/fix/{fix_manual_name}"
    testdata_name = f"{doc_date}-SQL-测试样本-{theme}.sql"
    testdata_rel = f"{rel_root}/sql/testdata/{testdata_name}"
    acceptance_name = f"{doc_date}-联调验收记录-{theme}.md"
    acceptance_rel = f"{rel_root}/testing/{acceptance_name}"
    test_result_name = f"{doc_date}-测试结果-{theme}.md"
    test_result_rel = f"{rel_root}/testing/{test_result_name}"
    uat_case_name = f"{doc_date}-UAT测试用例-{theme}.md"
    uat_case_rel = f"{rel_root}/testing/{uat_case_name}"
    scripts_readme_rel = f"{rel_root}/scripts/README.md"

    created_items = [
        ("directory-note", directory_note_rel, write_if_absent(exec_dir / "00-目录说明.md", directory_note_body())),
        ("design", design_rel, write_if_absent(design_dir / design_name, design_body(doc_date, theme))),
        ("detailed-design", detailed_design_rel, write_if_absent(design_dir / detailed_design_name, detailed_design_body(doc_date, theme))),
        ("breakdown", breakdown_rel, write_if_absent(design_dir / breakdown_name, breakdown_body(doc_date, theme, design_rel))),
        ("physical-design", physical_design_rel, write_if_absent(design_dir / physical_design_name, physical_design_body(doc_date, theme))),
        ("table-mapping", table_mapping_rel, write_if_absent(design_dir / table_mapping_name, table_mapping_body(doc_date, theme))),
        ("prd-trace", prd_trace_rel, write_if_absent(design_dir / prd_trace_name, prd_trace_body(doc_date, theme, prd_rel, design_rel))),
        ("product-confirmation", product_confirm_rel, write_if_absent(design_dir / product_confirm_name, product_confirmation_body(doc_date, theme, prd_rel))),
        ("impl-alignment", impl_alignment_rel, write_if_absent(design_dir / impl_alignment_name, impl_alignment_body(doc_date, theme, prd_rel))),
        ("ddl", ddl_rel, write_if_absent(sql_ddl_dir / ddl_name, sql_template_body(doc_date, theme, "DDL"))),
        ("ddl-field-fix", ddl_field_fix_rel, write_if_absent(sql_ddl_dir / ddl_field_fix_name, sql_template_body(doc_date, theme, "DDL-字段修正"))),
        ("ddl-index-fix", ddl_index_fix_rel, write_if_absent(sql_ddl_dir / ddl_index_fix_name, sql_template_body(doc_date, theme, "DDL-索引修正"))),
        ("ddl-slim-fields", ddl_slim_rel, write_if_absent(sql_ddl_dir / ddl_slim_name, sql_template_body(doc_date, theme, "DDL-精简字段"))),
        ("ddl-sequence", ddl_sequence_rel, write_if_absent(sql_ddl_dir / ddl_sequence_name, sql_template_body(doc_date, theme, "DDL-主键序列"))),
        ("fix-auto", fix_auto_rel, write_if_absent(sql_fix_dir / fix_auto_name, sql_template_body(doc_date, theme, "SQL-历史补全"))),
        ("fix-manual", fix_manual_rel, write_if_absent(sql_fix_dir / fix_manual_name, sql_template_body(doc_date, theme, "SQL-人工映射模板"))),
        ("testdata", testdata_rel, write_if_absent(sql_testdata_dir / testdata_name, sql_template_body(doc_date, theme, "SQL-测试样本"))),
        ("acceptance", acceptance_rel, write_if_absent(testing_dir / acceptance_name, testing_doc_body(doc_date, theme, "联调验收记录"))),
        ("test-result", test_result_rel, write_if_absent(testing_dir / test_result_name, testing_doc_body(doc_date, theme, "测试结果"))),
        ("uat-testcase", uat_case_rel, write_if_absent(testing_dir / uat_case_name, testing_doc_body(doc_date, theme, "UAT测试用例"))),
        ("scripts-readme", scripts_readme_rel, write_if_absent(scripts_dir / "README.md", scripts_readme_body(theme))),
    ]

    return created_items


def create_flat_bundle(
    exec_dir: Path,
    rel_root: str,
    doc_date: str,
    theme: str,
    design_name: str,
    breakdown_name: str,
) -> list[tuple[str, str, bool]]:
    design_rel = f"{rel_root}/{design_name}"
    breakdown_rel = f"{rel_root}/{breakdown_name}"

    created_design = write_if_absent(exec_dir / design_name, design_body(doc_date, theme))
    created_breakdown = write_if_absent(
        exec_dir / breakdown_name,
        breakdown_body(doc_date, theme, design_rel),
    )

    return [
        ("design", design_rel, created_design),
        ("breakdown", breakdown_rel, created_breakdown),
    ]


def next_req_id(req_file: Path, doc_date: str) -> str:
    prefix = f"REQ-{doc_date}-"
    if not req_file.exists():
        return f"{prefix}01"
    content = req_file.read_text(encoding="utf-8")
    nums = []
    for match in re.finditer(rf"{re.escape(prefix)}(\d+)\b", content):
        nums.append(int(match.group(1)))
    return f"{prefix}{max(nums, default=0) + 1:02d}"


def next_task_id(task_file: Path, doc_date: str) -> str:
    prefix = f"TASK-{doc_date}-"
    if not task_file.exists():
        return f"{prefix}01"
    content = task_file.read_text(encoding="utf-8")
    nums = []
    for match in re.finditer(rf"{re.escape(prefix)}(\d+)", content):
        nums.append(int(match.group(1)))
    return f"{prefix}{max(nums, default=0) + 1:02d}"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Create requirement governance docs and folders, then sync README if possible"
    )
    add_profile_arg(parser)
    parser.add_argument("--docs-root", required=True, help="Docs root, e.g. /path/to/repo/doc")
    parser.add_argument("--date", default=date.today().isoformat())
    parser.add_argument("--theme", required=True)
    parser.add_argument(
        "--req-root",
        default="requirements",
        help="Root folder for requirement governance under docs-root, e.g. requirements",
    )
    parser.add_argument(
        "--layout",
        choices=("layered", "flat"),
        default="layered",
        help="Requirement directory layout. layered matches the current repo convention.",
    )
    parser.add_argument(
        "--per-demand-folder",
        dest="per_demand_folder",
        action="store_true",
        help="Create docs under req-root/YYYY-MM-DD-theme/",
    )
    parser.add_argument(
        "--no-per-demand-folder",
        dest="per_demand_folder",
        action="store_false",
        help="Create docs under req-root/01-开发执行/ instead of a dated demand folder",
    )
    parser.set_defaults(per_demand_folder=True)
    parser.add_argument("--readme", default="README.md")
    parser.add_argument("--readme-section", default="### 执行文档")
    parser.add_argument("--design-prefix", default="技术设计")
    parser.add_argument("--breakdown-prefix", default="开发任务拆解")
    parser.add_argument("--prd-rel", default="docs/workflow/PRD/待补PRD文档.md")
    parser.add_argument("--req-id", help="Optional requirement id; auto-generated if omitted")
    parser.add_argument("--initial-task-id", help="Optional initial bootstrap task id; auto-generated if omitted")
    parser.add_argument("--req-status", default="planned")
    parser.add_argument("--initial-task-status", default="todo")
    parser.add_argument("--initial-task-title", help="Optional initial task title")
    parser.add_argument("--initial-task-acceptance", default="技术设计文档与开发任务拆解初稿已建立，可进入细化评审")
    parser.add_argument("--skip-governance-sync", action="store_true", help="Only create bundle files, skip 需求池/任务看板 auto sync")
    args = parser.parse_args()

    docs_root = Path(args.docs_root)
    req_root = docs_root / args.req_root
    req_root.mkdir(parents=True, exist_ok=True)

    design_name, breakdown_name = build_names(
        args.date,
        args.theme,
        args.design_prefix,
        args.breakdown_prefix,
    )

    if args.per_demand_folder:
        rel_root = f"{args.req_root}/{args.date}-{args.theme}"
        exec_dir = req_root / f"{args.date}-{args.theme}"
    else:
        rel_root = f"{args.req_root}/01-开发执行"
        exec_dir = req_root / "01-开发执行"
    exec_dir.mkdir(parents=True, exist_ok=True)

    if args.layout == "layered":
        created_items = create_layered_bundle(
            exec_dir,
            rel_root,
            args.date,
            args.theme,
            design_name,
            breakdown_name,
            args.prd_rel,
        )
    else:
        created_items = create_flat_bundle(
            exec_dir,
            rel_root,
            args.date,
            args.theme,
            design_name,
            breakdown_name,
        )

    sync_readme(
        docs_root / args.readme,
        args.readme_section,
        [f"{rel_root}/"] + [item[1] for item in created_items],
    )

    req_file = req_root / "需求池.md"
    task_file = req_root / "任务看板.md"
    req_id = args.req_id or next_req_id(req_file, args.date)
    initial_task_id = args.initial_task_id or next_task_id(task_file, args.date)
    initial_task_title = args.initial_task_title or f"完善{args.theme}技术设计与任务拆解初稿"

    governance_synced = False
    if not args.skip_governance_sync and args.layout == "layered" and args.per_demand_folder:
        docs_prefix = docs_root.name
        bundle_link_root = f"{docs_prefix}/{rel_root}"
        design_docs = [
            f"[{bundle_link_root}/design/{design_name}]({bundle_link_root}/design/{design_name})",
            f"[{bundle_link_root}/design/{breakdown_name}]({bundle_link_root}/design/{breakdown_name})",
            f"[{bundle_link_root}/design/{args.date}-PRD追溯-{args.theme}.md]({bundle_link_root}/design/{args.date}-PRD追溯-{args.theme}.md)",
        ]
        source_link = f"[{args.prd_rel}]({args.prd_rel})"
        task_board_link = f"[{docs_prefix}/{args.req_root}/任务看板.md]({docs_prefix}/{args.req_root}/任务看板.md)"
        doc_link = f"[{bundle_link_root}/design/{design_name}]({bundle_link_root}/design/{design_name})"

        sync_requirement_pool_entry(
            req_path=req_file,
            req_id=req_id,
            title=args.theme,
            status=args.req_status,
            source=source_link,
            design_docs=design_docs,
            task_board=task_board_link,
            sync_date=args.date,
            dry_run=False,
        )
        sync_task_board_entry(
            task_path=task_file,
            req_id=req_id,
            req_title=args.theme,
            task_id=initial_task_id,
            task_title=initial_task_title,
            status=args.initial_task_status,
            acceptance=args.initial_task_acceptance,
            doc_link=doc_link,
            sync_date=args.date,
            dry_run=False,
        )
        governance_synced = True

    print_header(
        "Requirement Bundle",
        {
            "layout": args.layout,
            "folder": f"{rel_root}/",
            "req_id": req_id,
            "initial_task_id": initial_task_id,
        },
    )
    for label, rel_path, created in created_items:
        print(f"- {label}: {rel_path} ({'created' if created else 'exists'})")
    if args.layout == "layered":
        print(f"- created-dir: {rel_root}/design/")
        print(f"- created-dir: {rel_root}/sql/ddl/")
        print(f"- created-dir: {rel_root}/sql/fix/")
        print(f"- created-dir: {rel_root}/sql/testdata/")
        print(f"- created-dir: {rel_root}/testing/")
        print(f"- created-dir: {rel_root}/scripts/")
    print("- readme: synced if section exists")
    print(f"- governance-sync: {'done' if governance_synced else 'skipped'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
