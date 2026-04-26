import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import type { RuntimeState } from "../lib/types";

type KnowledgebaseCardProps = {
  state: RuntimeState;
};

const isTauriRuntime = "__TAURI_INTERNALS__" in window;

export function KnowledgebaseCard({ state }: KnowledgebaseCardProps) {
  const kb = state.knowledgebase_push;
  const endpoint = normalizeEndpoint(kb.endpoint);
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState("");
  const statusText = !kb.enabled
    ? "推送已关闭"
    : kb.connected
      ? "已连接"
      : "未连接";

  const detailText = openError
    || (kb.enabled
      ? (kb.connected
        ? `最近 ${kb.last_push_at} · 失败 ${kb.failure_count}`
        : (kb.last_error || "知识库未启动，点击“启动并打开”自动拉起"))
      : "启用后将自动回写知识事件");

  async function handleOpen() {
    if (!endpoint) {
      return;
    }
    setOpening(true);
    setOpenError("");
    try {
      if (isTauriRuntime) {
        await invoke("open_knowledgebase");
        return;
      }
      window.open(endpoint, "_blank", "noopener,noreferrer");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setOpenError(message || "知识库启动失败");
      if (!isTauriRuntime) {
        window.open(endpoint, "_blank", "noopener,noreferrer");
      }
    } finally {
      setOpening(false);
    }
  }

  const canOpen = Boolean(endpoint) && !opening;
  const actionLabel = kb.connected ? "打开知识库" : "启动并打开";

  return (
    <section className="card kb-card" role="region" aria-label="知识库连接状态">
      <div className="kb-card__top">
        <h3 className="kb-card__title">知识库连接</h3>
        <div className="kb-card__actions">
          <div className="kb-card__status" data-state={kb.connected ? "ok" : "error"}>
            <span className="status-dot" />
            <strong>{statusText}</strong>
          </div>
          <button className="kb-card__open" type="button" onClick={handleOpen} disabled={!canOpen}>
            {opening ? "启动中..." : actionLabel}
          </button>
        </div>
      </div>
      <p className="kb-card__summary">
        <span className="kb-card__meta">{detailText}</span>
        <span className="kb-card__endpoint" title={kb.connected ? endpoint : (kb.last_error || endpoint)}>
          {endpoint || "未配置 endpoint"}
        </span>
      </p>
    </section>
  );
}

function normalizeEndpoint(raw: string): string {
  const value = raw.trim();
  if (!value) {
    return "";
  }
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(value)) {
    return value;
  }
  return `http://${value}`;
}
