# Context Brief

- updated_at: 2026-05-24T17:54:52+08:00
- workspace: /Users/wucongpeng/Documents/ai/skill/workflow-skills-copy
- requirement: `N/A` 未指定
- task: `SKILL-PROMPT-COMPACT` workflow skills 简明提示词
- status: done / compact
- summary: 在不改执行逻辑的前提下，精简 AGENTS 与三类 skill 输出要求：默认 3-5 行短分析、最终 5 行以内，风险/审计/用户要求时再展开。
- files: AGENTS.md；workflow-bootstrap/scripts/init_workflow_bootstrap.py；workflow-bootstrap/SKILL.md；workflow-requirement/SKILL.md；workflow-execution/SKILL.md
- evidence: AGENTS.md
- verified: python3 -m py_compile workflow-bootstrap/scripts/*.py workflow-requirement/scripts/*.py workflow-execution/scripts/*.py -> PASS；git diff --check -> PASS
- risk: 无
- next: commit + push
