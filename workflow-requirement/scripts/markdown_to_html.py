#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path
import re


TEMPLATE = '''<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{project_name} - 项目需求看板</title>
<script src="https://cdn.tailwindcss.com"></script>
<link href="https://fonts.googleapis.com/css2?family=Noto+Sans+SC:wght@300;400;500;600;700&display=swap" rel="stylesheet">
<script>
tailwind.config = {{
  theme: {{
    extend: {{
      fontFamily: {{
        sans: ['Noto Sans SC', 'sans-serif'],
      }},
      colors: {{
        primary: {{
          50: '#eff6ff',
          100: '#dbeafe',
          500: '#3b82f6',
          600: '#2563eb',
          700: '#1d4ed8',
        }},
        success: {{
          100: '#dcfce7',
          500: '#22c55e',
          700: '#15803d',
        }},
        warning: {{
          100: '#fef3c7',
          500: '#f59e0b',
          700: '#b45309',
        }},
        danger: {{
          100: '#fee2e2',
          500: '#ef4444',
          700: '#b91c1c',
        }},
        purple: {{
          100: '#f3e8ff',
          500: '#a855f7',
          700: '#7e22ce',
        }},
      }},
    }},
  }}
}}
</script>
<style>
body {{
  font-family: 'Noto Sans SC', sans-serif;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  min-height: 100vh;
}}
.card {{
  background: rgba(255, 255, 255, 0.95);
  backdrop-filter: blur(10px);
  border-radius: 16px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.1);
}}
a {{
  color: #2563eb;
  text-decoration: underline;
  transition: color 0.2s;
}}
a:hover {{
  color: #1d4ed8;
}}
.badge-done {{
  background: #dcfce7;
  color: #15803d;
  font-weight: 600;
}}
.badge-todo {{
  background: #fef3c7;
  color: #b45309;
  font-weight: 600;
}}
.badge-doing {{
  background: #fee2e2;
  color: #b91c1c;
  font-weight: 600;
}}
.badge-planned {{
  background: #dbeafe;
  color: #1d4ed8;
  font-weight: 600;
}}
.badge-blocked {{
  background: #f3e8ff;
  color: #7e22ce;
  font-weight: 600;
}}
.table-row-hover:hover {{
  background: #f8fafc;
  transition: background 0.2s;
}}
.req-card {{
  border-left: 4px solid #3b82f6;
  transition: all 0.2s;
}}
.req-card:hover {{
  transform: translateX(4px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
}}
.tab-btn {{
  transition: all 0.2s;
}}
.tab-btn.active {{
  background: #3b82f6;
  color: white;
}}
.filter-btn {{
  transition: all 0.2s;
}}
.filter-btn.active {{
  background: #3b82f6;
  color: white;
}}
@keyframes fadeIn {{
  from {{ opacity: 0; transform: translateY(10px); }}
  to {{ opacity: 1; transform: translateY(0); }}
}}
.tab-content {{
  animation: fadeIn 0.3s ease-out;
}}
#searchInput:focus {{
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.2);
}}
</style>
</head>
<body class="py-6 px-4">
<div class="max-w-7xl mx-auto">
  <div class="card p-6 mb-4">
    <div class="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-4">
      <div>
        <h1 class="text-2xl md:text-3xl font-bold text-gray-800 mb-1">{project_name}</h1>
        <p class="text-gray-500">需求看板 · 生成时间: {generate_time}</p>
      </div>
    </div>

    <!-- 统计信息 -->
    <div class="flex flex-wrap items-center gap-3 mb-4 p-3 bg-gray-50 rounded-lg">
      <span class="text-sm text-gray-600 font-medium">进度统计：</span>
      <span id="stats-done" class="px-3 py-1 bg-green-100 text-green-700 rounded-full"></span>
      <span id="stats-doing" class="px-3 py-1 bg-red-100 text-red-700 rounded-full"></span>
      <span id="stats-todo" class="px-3 py-1 bg-yellow-100 text-yellow-700 rounded-full"></span>
    </div>

    <div class="flex flex-wrap gap-3 mb-4 border-b border-gray-200 pb-3">
      <button class="tab-btn active px-4 py-2 rounded-lg font-medium" onclick="switchTab('requirements')">需求池</button>
      <button class="tab-btn px-4 py-2 rounded-lg font-medium text-gray-600 hover:bg-gray-100" onclick="switchTab('tasks')">任务看板</button>
    </div>

    <!-- 搜索和过滤 -->
    <div class="flex flex-col md:flex-row gap-3 mb-4">
      <input 
        type="text" 
        id="searchInput" 
        placeholder="🔍 搜索关键词..." 
        class="flex-1 px-4 py-2 border border-gray-300 rounded-lg focus:outline-none focus:border-primary-500"
        oninput="filterContent()"
      >
      <div class="flex gap-2 flex-wrap">
        <button class="filter-btn active px-3 py-2 border rounded-lg text-sm font-medium text-gray-600 hover:bg-gray-100" data-filter="all" onclick="setFilter('all')">全部</button>
        <button class="filter-btn px-3 py-2 border rounded-lg text-sm font-medium text-green-700 hover:bg-green-50" data-filter="done" onclick="setFilter('done')">完成</button>
        <button class="filter-btn px-3 py-2 border rounded-lg text-sm font-medium text-red-700 hover:bg-red-50" data-filter="doing" onclick="setFilter('doing')">进行中</button>
        <button class="filter-btn px-3 py-2 border rounded-lg text-sm font-medium text-yellow-700 hover:bg-yellow-50" data-filter="todo" onclick="setFilter('todo')">待开始</button>
      </div>
    </div>

    <div id="requirements-tab" class="tab-content">
      <div class="mb-4">
        <div class="flex items-center justify-between mb-3">
          <h2 class="text-xl font-semibold text-gray-700">需求列表</h2>
          <span class="text-sm text-gray-500" id="req-count"></span>
        </div>
        <div id="requirements-content" class="space-y-4">{requirements_html}</div>
        <div id="req-no-match" class="hidden py-8 text-center text-gray-500">
          <p class="text-lg">没有找到匹配的需求</p>
        </div>
      </div>
    </div>

    <div id="tasks-tab" class="tab-content hidden">
      <div class="mb-4">
        <div class="flex items-center justify-between mb-3">
          <h2 class="text-xl font-semibold text-gray-700">任务分解</h2>
          <span class="text-sm text-gray-500" id="task-count"></span>
        </div>
        <div class="overflow-x-auto">
          <table class="w-full">
            <thead>
              <tr class="bg-gray-50 text-left">
                <th class="px-4 py-3 text-sm font-semibold text-gray-600">需求ID</th>
                <th class="px-4 py-3 text-sm font-semibold text-gray-600">任务ID</th>
                <th class="px-4 py-3 text-sm font-semibold text-gray-600">任务标题</th>
                <th class="px-4 py-3 text-sm font-semibold text-gray-600">状态</th>
                <th class="px-4 py-3 text-sm font-semibold text-gray-600">验收标准</th>
              </tr>
            </thead>
            <tbody id="tasks-content">{tasks_html}</tbody>
            <tfoot>
              <tr id="task-no-match" class="hidden">
                <td colspan="5" class="px-4 py-8 text-center text-gray-500">
                  <p class="text-lg">没有找到匹配的任务</p>
                </td>
              </tr>
            </tfoot>
          </table>
        </div>
      </div>
    </div>
  </div>

  <div class="card p-4">
    <div class="text-sm text-gray-500 text-center">
      由 workflow-requirement 自动生成
    </div>
  </div>
</div>

<script>
let currentFilter = 'all';
let currentSearch = '';

function getStatusBadge(status) {{
  const cleanStatus = status.replace(/`/g, '').toLowerCase().trim();
  const badgeClass = {{
    done: 'badge-done',
    todo: 'badge-todo',
    doing: 'badge-doing',
    planned: 'badge-planned',
    blocked: 'badge-blocked',
  }}[cleanStatus] || 'bg-gray-100 text-gray-600';
  return '<span class="inline-block px-3 py-1 rounded-full text-sm status-badge ' + badgeClass + '" data-status="' + cleanStatus + '">' + cleanStatus + '</span>';
}}

function switchTab(tab) {{
  document.querySelectorAll('.tab-content').forEach(el => el.classList.add('hidden'));
  document.querySelectorAll('.tab-btn').forEach(el => el.classList.remove('active'));
  
  if (tab === 'requirements') {{
    document.getElementById('requirements-tab').classList.remove('hidden');
    document.querySelectorAll('.tab-btn')[0].classList.add('active');
  }} else {{
    document.getElementById('tasks-tab').classList.remove('hidden');
    document.querySelectorAll('.tab-btn')[1].classList.add('active');
  }}
  filterContent();
}}

function setFilter(filter) {{
  currentFilter = filter;
  document.querySelectorAll('.filter-btn').forEach(btn => {{
    if (btn.dataset.filter === filter) {{
      btn.classList.add('active');
    }} else {{
      btn.classList.remove('active');
    }}
  }});
  filterContent();
}}

function filterContent() {{
  currentSearch = document.getElementById('searchInput').value.toLowerCase();
  
  // Filter requirements
  const reqItems = document.querySelectorAll('#requirements-content .req-card');
  let reqVisible = 0;
  reqItems.forEach(item => {{
    const text = item.textContent.toLowerCase();
    const statusEl = item.querySelector('.status-badge');
    const status = statusEl ? statusEl.dataset.status : '';
    
    let matchSearch = !currentSearch || text.includes(currentSearch);
    let matchFilter = currentFilter === 'all' || status === currentFilter;
    
    if (matchSearch && matchFilter) {{
      item.style.display = '';
      reqVisible++;
    }} else {{
      item.style.display = 'none';
    }}
  }});
  
  document.getElementById('req-count').textContent = '显示 ' + reqVisible + ' 个需求';
  document.getElementById('req-no-match').classList.toggle('hidden', reqVisible > 0);
  
  // Filter tasks
  const taskRows = document.querySelectorAll('#tasks-content tr');
  let taskVisible = 0;
  taskRows.forEach(row => {{
    const text = row.textContent.toLowerCase();
    const statusEl = row.querySelector('.status-badge');
    const status = statusEl ? statusEl.dataset.status : '';
    
    let matchSearch = !currentSearch || text.includes(currentSearch);
    let matchFilter = currentFilter === 'all' || status === currentFilter;
    
    if (matchSearch && matchFilter) {{
      row.style.display = '';
      taskVisible++;
    }} else {{
      row.style.display = 'none';
    }}
  }});
  
  document.getElementById('task-count').textContent = '显示 ' + taskVisible + ' 个任务';
  document.getElementById('task-no-match').classList.toggle('hidden', taskVisible > 0);
  
  updateStats();
}}

function updateStats() {{
  updateStatsOverall();
}}

function updateStatsOverall() {{
  const stats = {{ done: 0, todo: 0, doing: 0, planned: 0, blocked: 0 }};
  const allBadges = document.querySelectorAll('.status-badge');
  allBadges.forEach(el => {{
    const status = el.dataset.status;
    if (stats[status] !== undefined) stats[status]++;
  }});
  document.getElementById('stats-done').textContent = '完成: ' + stats.done;
  document.getElementById('stats-doing').textContent = '进行中: ' + stats.doing;
  document.getElementById('stats-todo').textContent = '待开始: ' + (stats.todo + stats.planned + stats.blocked);
}}

window.onload = function() {{
  filterContent();
}};
</script>
</body>
</html>
'''


def parse_requirements(md_text: str) -> list[dict]:
    requirements = []
    in_table = False
    headers = []
    
    lines = md_text.splitlines()
    for line in lines:
        line = line.strip()
        if not line:
            continue
        
        if '需求列表' in line:
            in_table = True
            continue
        
        if in_table and line.startswith('|') and '---' not in line:
            cells = [c.strip().strip('`') for c in line.split('|') if c.strip()]
            if not headers:
                headers = cells
                continue
            if len(cells) == len(headers):
                req = {}
                for h, c in zip(headers, cells):
                    req[h] = c
                requirements.append(req)
    
    return requirements


def parse_tasks(md_text: str) -> dict[str, list[dict]]:
    tasks_by_req = {}
    current_req_id = ''
    in_table = False
    headers = []
    
    lines = md_text.splitlines()
    for line in lines:
        line = line.strip()
        if not line:
            continue
        
        if line.startswith('## ') and 'REQ-' in line:
            match = re.search(r'(REQ-[\d-]+)', line)
            if match:
                current_req_id = match.group(1)
                tasks_by_req[current_req_id] = []
            in_table = False
            continue
        
        if current_req_id and line.startswith('|') and '任务ID' in line:
            in_table = True
            headers = [c.strip() for c in line.split('|') if c.strip()]
            continue
        
        if in_table and line.startswith('|') and '---' not in line:
            cells = [c.strip().strip('`') for c in line.split('|') if c.strip()]
            if len(cells) == len(headers):
                task = {}
                for h, c in zip(headers, cells):
                    task[h] = c
                tasks_by_req[current_req_id].append(task)
    
    return tasks_by_req


def convert_md_links_to_html(text: str) -> str:
    import re
    # Convert markdown links [text](url) to HTML <a href="url">text</a>
    # Keep .md links as-is so browser markdown plugins can render them directly.
    # The main output file is in doc/requirements/, so we only adjust relative path.
    def replace_link(match):
        link_text = match.group(1)
        link_url = match.group(2)
        # If path starts with doc/, need to strip it because:
        # Original path is from project root: doc/PRD/xxx.md
        # index.html is at doc/requirements/需求看板.html
        # So doc/PRD/xxx.md -> ../PRD/xxx.md
        # doc/requirements/xxx -> ../requirements/xxx (this is correct from index.html location)
        if link_url.startswith('doc/'):
            link_url = '../' + link_url[4:]  # Remove 'doc/' and add ../
        return f'<a href="{link_url}" target="_blank" class="text-primary-600 hover:text-primary-700 underline">{link_text}</a>'
    
    # Find all markdown links and replace them
    return re.sub(r'\[([^\]]+)\]\(([^\)]+)\)', replace_link, text)


def render_requirements(requirements: list[dict]) -> str:
    if not requirements:
        return '<p class="text-gray-500 italic">暂无需求</p>'
    
    html = ''
    for req in requirements:
        title = req.get('标题', '')
        req_id = req.get('需求ID', '')
        status = req.get('状态', '')
        source = req.get('来源', '')
        design = req.get('设计文档', '')
        
        status_clean = status.replace('`', '').lower().strip()
        badge_class = {
            'done': 'badge-done',
            'todo': 'badge-todo',
            'doing': 'badge-doing',
            'planned': 'badge-planned',
            'blocked': 'badge-blocked',
        }.get(status_clean, 'bg-gray-100 text-gray-600')
        
        status_badge = f'<span class="inline-block px-3 py-1 rounded-full text-sm status-badge {badge_class}" data-status="{status_clean}">{status_clean}</span>'
        
        html += f'''
<div class="req-card bg-white p-5 rounded-lg shadow-sm border border-gray-100" data-search-title="{title.lower()}">
  <div class="flex items-start justify-between mb-3">
    <div>
      <h3 class="text-lg font-semibold text-gray-800 mb-1">{title}</h3>
      <p class="text-sm text-gray-500">{req_id}</p>
    </div>
    <div>{status_badge}</div>
  </div>
  <div class="space-y-2 text-sm">
'''
        if source:
            source_html = convert_md_links_to_html(source)
            html += f'    <div><span class="text-gray-500">来源：</span>{source_html}</div>\n'
        if design:
            design_html = convert_md_links_to_html(design)
            html += f'    <div><span class="text-gray-500">设计文档：</span><br>{design_html}</div>\n'
        
        html += '''
  </div>
</div>
'''
    return html


def render_tasks(tasks_by_req: dict[str, list[dict]]) -> str:
    has_tasks = False
    html = ''
    
    for req_id, tasks in tasks_by_req.items():
        if not tasks:
            continue
        for task in tasks:
            has_tasks = True
            title = task.get('任务标题', '')
            task_id = task.get('任务ID', '')
            status = task.get('状态', '')
            acceptance = task.get('验收标准', '')
            
            status_clean = status.replace('`', '').lower().strip()
            badge_class = {
                'done': 'badge-done',
                'todo': 'badge-todo',
                'doing': 'badge-doing',
                'blocked': 'badge-blocked',
            }.get(status_clean, 'bg-gray-100 text-gray-600')
            
            status_badge = f'<span class="inline-block px-3 py-1 rounded-full text-sm status-badge {badge_class}" data-status="{status_clean}">{status_clean}</span>'
            
            html += f'''
<tr class="table-row-hover border-b border-gray-100">
  <td class="px-4 py-3 text-sm text-gray-600">{req_id}</td>
  <td class="px-4 py-3 text-sm text-gray-600 font-mono">{task_id}</td>
  <td class="px-4 py-3 text-sm font-medium text-gray-800">{title}</td>
  <td class="px-4 py-3">{status_badge}</td>
  <td class="px-4 py-3 text-sm text-gray-600">{acceptance}</td>
</tr>
'''
    
    if not has_tasks:
        return '<tr><td colspan="5" class="px-4 py-8 text-center text-gray-500 italic">暂无任务</td></tr>'
    
    return html


def main() -> int:
    from datetime import datetime
    parser = argparse.ArgumentParser(description="Convert requirement pool and task board to beautiful combined HTML page for sharing")
    parser.add_argument("--requirements", required=True, help="Requirements pool markdown path, e.g. doc/requirements/需求池.md")
    parser.add_argument("--tasks", required=True, help="Task board markdown path, e.g. doc/requirements/任务看板.md")
    parser.add_argument("--project", default="项目需求看板", help="Project name, displayed in title")
    parser.add_argument("--output", help="Output HTML file path, default: doc/requirements/需求看板.html")
    args = parser.parse_args()
    
    req_path = Path(args.requirements)
    tasks_path = Path(args.tasks)
    
    if not req_path.exists():
        print(f"Error: Requirements file not found: {req_path}")
        return 1
    if not tasks_path.exists():
        print(f"Error: Tasks file not found: {tasks_path}")
        return 1
    
    req_text = req_path.read_text(encoding="utf-8")
    tasks_text = tasks_path.read_text(encoding="utf-8")
    
    requirements = parse_requirements(req_text)
    tasks_by_req = parse_tasks(tasks_text)
    
    requirements_html = render_requirements(requirements)
    tasks_html = render_tasks(tasks_by_req)
    
    generate_time = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
    
    full_html = TEMPLATE.format(
        project_name=args.project,
        generate_time=generate_time,
        requirements_html=requirements_html,
        tasks_html=tasks_html
    )
    
    if args.output:
        output_path = Path(args.output)
    else:
        output_path = req_path.parent / '需求看板.html'
    
    output_path.write_text(full_html, encoding="utf-8")
    
    print(f"\n✅ Generated combined HTML page: {output_path}")
    print("  Linked documents keep their original .md targets for browser markdown plugins.")
    
    print(f"\n📋 Features:")
    print(f"  - Combined view with tabs: 需求池 | 任务看板")
    print(f"  - 🔍 Search by keyword")
    print(f"  - 🎯 Filter by status (all/done/doing/todo)")
    print(f"  - 📊 Stats always visible at top")
    print(f"  - Beautiful gradient background with glass card design")
    print(f"  - Status colors: done=green, todo=yellow, doing=red")
    print(f"  - Responsive layout, works on desktop and mobile")
    print(f"\n📤 Share:")
    print(f"  Send {output_path} to product manager")
    print(f"  Product opens in browser directly, no login required")
    
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
