import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { KbSearchItem, KbSearchResponse, KbStats, KbTraceResponse } from "../lib/types";

type KnowledgebasePanelProps = {
  onBack: () => void;
};

export function KnowledgebasePanel({ onBack }: KnowledgebasePanelProps) {
  const [stats, setStats] = useState<KbStats>({ projects: 0, items: 0, events: 0, links: 0 });
  const [query, setQuery] = useState("知识库");
  const [items, setItems] = useState<KbSearchItem[]>([]);
  const [trace, setTrace] = useState<KbTraceResponse | null>(null);
  const [projectPath, setProjectPath] = useState("");
  const [registerPath, setRegisterPath] = useState("");
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

  async function runAction(action: () => Promise<void>) {
    try {
      setBusy(true);
      setError("");
      await action();
      await loadStats();
      await runSearch();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    runAction(async () => {
      await loadStats();
      await runSearch();
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
            <input value={projectPath} onChange={(e) => setProjectPath(e.target.value)} placeholder="项目路径（同步 inbox）" />
            <button
              type="button"
              onClick={() => runAction(async () => {
                if (!projectPath.trim()) return;
                await invoke("kb_ingest_inbox", { path: projectPath.trim() });
              })}
              disabled={busy}
            >
              同步Inbox
            </button>
          </div>

          <div className="kb-panel__row">
            <input value={registerPath} onChange={(e) => setRegisterPath(e.target.value)} placeholder="项目路径（注册）" />
            <button
              type="button"
              onClick={() => runAction(async () => {
                if (!registerPath.trim()) return;
                await invoke("kb_register_project", { path: registerPath.trim() });
              })}
              disabled={busy}
            >
              注册项目
            </button>
          </div>

          <div className="kb-panel__results">
            {items.length === 0 ? (
              <div className="muted">无结果</div>
            ) : items.map((item) => (
              <button key={item.item_id} type="button" className="kb-panel__item" onClick={() => loadTrace(item.item_id)}>
                <b>{item.title}</b>
                <span>{item.source_path}</span>
              </button>
            ))}
          </div>

          {error ? <div className="kb-panel__error">{error}</div> : null}
        </div>

        <div className="card kb-panel__trace-card">
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
