# Knowledgebase V6 Regression

Date: 2026-04-29

## Scope

- Project health snapshot model and migration.
- Multi-project overview/detail/actions APIs.
- Multi-project dashboard UI.
- Structured project action generation.
- Project links to health, evidence search, task starter, retrospective, prompt engineering, and collector.
- Existing tray panel startup risk.

## Commands

```bash
cargo check
npm run build
node - <<'NODE'
const fs = require('fs');
const vm = require('vm');
const html = fs.readFileSync('workflow-statusbar/src-tauri/resources/knowledgebase/index.html', 'utf8');
const scripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
for (const [index, script] of scripts.entries()) new vm.Script(script, { filename: `knowledgebase-inline-${index}.js` });
console.log(`checked ${scripts.length} inline script(s)`);
NODE
curl -fsS http://127.0.0.1:8788/api/projects/snapshots
curl -fsS http://127.0.0.1:8788/api/projects/overview
curl -fsS http://127.0.0.1:8788/api/projects/{project_id}/health
curl -fsS http://127.0.0.1:8788/api/projects/{project_id}/actions
```

## Results

- `cargo check`: passed.
- `npm run build`: passed.
- Inline script syntax check: passed.
- `/api/projects/snapshots`: returned 21 snapshots.
- `/api/projects/overview`: returned 21 projects.
- `/api/projects/:id/health`: first project was `erp-base`.
- `/api/projects/:id/actions`: returned 6 actions with `collect`, `verify`, `retro`, `template`, `cleanup`, and `starter`.
- Playwright browser check: passed.
  - Multi-project dashboard rendered project portfolio, ranking, project detail, direct project links, and action list.
  - Project `开工包` link opened task starter, filled `基于项目 erp-base 生成下一轮开工包。风险：缺少文档采集`, and generated a preview with 6 evidence items.
  - Project `健康度` link opened the health view and kept the project focus note for `erp-base`.

## Notes

- `erp-base` is a real local knowledgebase project: `/Users/wucongpeng/Documents/jty-work/erp-base`.
- Development startup for panel validation must use `npm run tauri -- dev`; plain `cargo run` starts the Rust side without the Vite panel frontend.
- macOS tray click itself still requires manual confirmation because terminal automation cannot fully reproduce the system menu extra click path.
