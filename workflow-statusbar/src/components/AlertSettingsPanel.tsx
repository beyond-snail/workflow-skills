import { useState } from "react";
import type { AlertProviderMode, AlertSettings } from "../lib/types";

type AlertSettingsPanelProps = {
  settings: AlertSettings;
  saving: boolean;
  onSave: (settings: AlertSettings) => Promise<void>;
  onBack: () => void;
};

const modeOptions: Array<{ value: AlertProviderMode; label: string; detail: string }> = [
  { value: "disabled", label: "关闭远程提醒", detail: "只保留本机提醒和状态弹窗。" },
  { value: "bridge", label: "桥接服务", detail: "发给你自己的 HTTP 服务，再由服务转发到飞书。" },
  { value: "feishu", label: "直连飞书", detail: "直接在桌面端使用飞书应用凭证发消息。" },
];

export function AlertSettingsPanel({
  settings,
  saving,
  onSave,
  onBack,
}: AlertSettingsPanelProps) {
  const [form, setForm] = useState<AlertSettings>(settings);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  function patchForm(patch: Partial<AlertSettings>) {
    setForm((current) => ({ ...current, ...patch }));
  }

  async function handleSave() {
    setError("");
    setMessage("");
    try {
      await onSave(form);
      setMessage("提醒配置已保存，后续新告警会按这份配置发送。");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <section className="card card--settings">
      <div className="group-toolbar">
        <button className="inline-link-button inline-link-button--strong" type="button" onClick={onBack}>
          返回监控
        </button>
        <span className="eyebrow">提醒配置</span>
      </div>

      <div className="settings-hero">
        <div className="card-copy">
          <p className="card-copy__eyebrow">Remote Alerts</p>
          <h2 className="card-copy__title">把任务状态推到飞书</h2>
          <p className="card-copy__detail">
            右键托盘图标即可重新打开这里。支持桥接服务和直连飞书两种模式。
          </p>
        </div>
      </div>

      <div className="settings-mode-grid">
        {modeOptions.map((option) => (
          <button
            key={option.value}
            type="button"
            className={form.mode === option.value ? "settings-mode settings-mode--active" : "settings-mode"}
            onClick={() => patchForm({ mode: option.value })}
          >
            <strong>{option.label}</strong>
            <span>{option.detail}</span>
          </button>
        ))}
      </div>

      {form.mode === "bridge" ? (
        <div className="settings-form">
          <label className="settings-field">
            <span>桥接服务地址</span>
            <input
              value={form.bridge_endpoint}
              onChange={(event) => patchForm({ bridge_endpoint: event.target.value })}
              placeholder="https://your-alert-bridge.example.com/alert"
            />
          </label>
          <label className="settings-field">
            <span>鉴权 Token</span>
            <input
              value={form.bridge_token}
              onChange={(event) => patchForm({ bridge_token: event.target.value })}
              placeholder="可留空，但建议启用"
            />
          </label>
        </div>
      ) : null}

      {form.mode === "feishu" ? (
        <div className="settings-form">
          <label className="settings-field">
            <span>App ID</span>
            <input
              value={form.feishu_app_id}
              onChange={(event) => patchForm({ feishu_app_id: event.target.value })}
              placeholder="cli_xxx"
            />
          </label>
          <label className="settings-field">
            <span>App Secret</span>
            <input
              type="password"
              value={form.feishu_app_secret}
              onChange={(event) => patchForm({ feishu_app_secret: event.target.value })}
              placeholder="重新生成后的 app secret"
            />
          </label>
          <label className="settings-field">
            <span>Open ID</span>
            <input
              value={form.feishu_open_id}
              onChange={(event) => patchForm({ feishu_open_id: event.target.value })}
              placeholder="发给个人时填写"
            />
          </label>
          <label className="settings-field">
            <span>Chat ID</span>
            <input
              value={form.feishu_chat_id}
              onChange={(event) => patchForm({ feishu_chat_id: event.target.value })}
              placeholder="发给群时填写，填了它会优先使用"
            />
          </label>
          <p className="settings-hint">
            `app_secret` 会保存在本机应用配置目录中，不会提交到仓库。建议你重置刚才发出来的旧密钥后再填这里。
          </p>
        </div>
      ) : null}

      <div className="settings-actions">
        <button className="ghost-button" type="button" onClick={onBack}>
          稍后再配
        </button>
        <button className="settings-save" type="button" onClick={handleSave} disabled={saving}>
          {saving ? "保存中..." : "保存并启用"}
        </button>
      </div>

      {message ? <p className="settings-feedback settings-feedback--success">{message}</p> : null}
      {error ? <p className="settings-feedback settings-feedback--error">{error}</p> : null}
    </section>
  );
}
