import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  KbCollectProjectResult,
  KbProjectStatus,
  KbSearchItem,
  KbSearchResponse,
  KbStats,
  KbTraceResponse,
} from "../lib/types";

type KnowledgebasePanelProps = {
  onBack: () => void;
};

export function KnowledgebasePanel({ onBack }: KnowledgebasePanelProps) {
  const [stats, setStats] = useState<KbStats>({ projects: 0, items: 0, events: 0, links: 0 });
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<KbSearchItem[]>([]);
  const [trace, setTrace] = useState<KbTraceResponse | null>(null);
  const [projects, setProjects] = useState<KbProjectStatus[]>([]);
  const [collectPath, setCollectPath] = useState("");
  const [lastCollect, setLastCollect] = useState<KbCollectProjectResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function loadStats() {
    const data = await invoke<KbStats>("kb_get_stats");
    setStats(data);
  }

  async function runSearch() {
    if (!query.trim()) {
      setItems([]);
      return;
    }
    const res = await invoke<KbSearchResponse>("kb_search", { query: query.trim() });
    setItems(res.items || []);
  }

  async function loadTrace(itemId: string) {
    const res = await invoke<KbTraceResponse>("kb_trace", { itemId });
    setTrace(res);
  }

  async function loadProjects() {
    const rows = await invoke<KbProjectStatus[]>("kb_list_projects");
    setProjects(rows || []);
  }

  async function runAction(action: () => Promise<void>) {
    try {
      setBusy(true);
      setError("");
      await action();
      await loadStats();
      await loadProjects();
      if (query.trim()) {
        await runSearch();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    runAction(async () => {
      await loadStats();
      await loadProjects();
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="kb-panel">
      <div className="kb-panel__head">
        <h3>个人超级知识库</h3>
        <button type="button" className="inline-link-button" onClick={onBack}>返回面板</button>
      </div>

      <div className="kb-panel__kpi">
        <div className="card"><b>{stats.items}</b><span>知识条目</span></div>
        <div className="card"><b>{stats.projects}</b><span>活跃项目</span></div>
        <div className="card"><b>{stats.events}</b><span>事件条目</span></div>
        <div className="card"><b>{stats.links}</b><span>追溯关系</span></div>
      </div>

      <div className="kb-panel__grid">
        <div className="card kb-panel__search-card">
          <div className="kb-panel__row">
            <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索关键词" />
            <button type="button" onClick={() => runAction(runSearch)} disabled={busy}>检索</button>
          </div>

          <div className="kb-panel__row">
            <input
              value={collectPath}
              onChange={(e) => setCollectPath(e.target.value)}
              placeholder="项目路径（手动采集）"
            />
            <button
              type="button"
              onClick={() => runAction(async () => {
                if (!collectPath.trim()) return;
                const res = await invoke<KbCollectProjectResult>("kb_collect_project", { path: collectPath.trim() });
                setLastCollect(res);
              })}
              disabled={busy}
            >
              采集项目
            </button>
          </div>

          {lastCollect ? (
            <div className="muted">
              最近采集：{lastCollect.project} · 新增事件 {lastCollect.events} · 文档 {lastCollect.documents} · 处理文件 {lastCollect.processed_files}
            </div>
          ) : null}

          <div className="kb-panel__results">
            {items.length === 0 ? (
              <div className="muted">请输入关键词后检索</div>
            ) : items.map((item) => (
              <button key={item.item_id} type="button" className="kb-panel__item" onClick={() => loadTrace(item.item_id)}>
                <b>{item.title}</b>
                <span>{item.item_type} · {item.source_path}</span>
              </button>
            ))}
          </div>

          {error ? <div className="kb-panel__error">{error}</div> : null}
        </div>

        <div className="card kb-panel__trace-card">
          <h4>项目采集状态</h4>
          <div className="kb-panel__projects">
            {projects.length === 0 ? (
              <div className="muted">还没有采集到项目数据，先执行一次“采集项目”。</div>
            ) : projects.map((row) => (
              <button
                key={row.path}
                type="button"
                className="kb-panel__project-item"
                onClick={() => {
                  setCollectPath(row.path);
                }}
              >
                <b>{row.project}</b>
                <span>{row.path}</span>
                <span>
                  条目 {row.item_count} · 事件 {row.event_count} · 文档 {row.document_count} · 对话 {row.conversation_count}
                </span>
              </button>
            ))}
          </div>
          <h4>追溯链</h4>
          {!trace?.item ? (
            <div className="muted">{"REQ -> TASK -> Decision -> Evidence"}</div>
          ) : (
            <div className="kb-panel__trace-list">
              <div className="node">
                <b>{trace.item.title}</b>
                <span>{trace.item.item_type} · {trace.item.source_path}</span>
              </div>
              {trace.related_items.length === 0 ? (
                <div className="muted">暂无关联节点</div>
              ) : trace.related_items.map((item) => (
                <div key={item.item_id} className="node">
                  <b>{item.title}</b>
                  <span>{item.item_type} · {item.source_path}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
