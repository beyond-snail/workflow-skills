import { useState } from "react";
import type { AlertProviderMode, AlertSettings } from "../lib/types";

type AlertSettingsPanelProps = {
  settings: AlertSettings;
  saving: boolean;
  onSave: (settings: AlertSettings) => Promise<void>;
  onSendTest: () => Promise<void>;
  onBack: () => void;
};

const modeOptions: Array<{ value: AlertProviderMode; label: string; detail: string }> = [
  { value: "disabled", label: "关闭远程提醒", detail: "只保留本机提醒和状态弹窗。" },
  { value: "bridge", label: "桥接服务", detail: "发给你自己的 HTTP 服务，再由服务转发到飞书。" },
  { value: "feishu", label: "直连飞书", detail: "直接在桌面端使用飞书应用凭证发消息。" },
];

const notificationOptions: Array<{ key: keyof AlertSettings; label: string; detail: string }> = [
  { key: "local_notifications_enabled", label: "本机通知", detail: "macOS 通知中心提醒。" },
  { key: "remote_notifications_enabled", label: "远程通知", detail: "飞书或桥接服务提醒。" },
  { key: "notify_task_completed", label: "任务完成", detail: "任务状态切到 done 时提醒。" },
  { key: "notify_project_completed", label: "项目完成", detail: "项目进入完成阶段时提醒。" },
  { key: "notify_project_blocked", label: "项目阻塞", detail: "项目进入 blocked 时提醒。" },
  { key: "notify_task_interrupted", label: "项目中断/自动续跑", detail: "执行掉线或触发自动续跑时提醒。" },
  { key: "notify_auto_resume_failed", label: "自动续跑失败", detail: "自动续跑失败时提醒。" },
];

export function AlertSettingsPanel({
  settings,
  saving,
  onSave,
  onSendTest,
  onBack,
}: AlertSettingsPanelProps) {
  const [form, setForm] = useState<AlertSettings>(settings);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [sendingTest, setSendingTest] = useState(false);

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

  async function handleSendTest() {
    setError("");
    setMessage("");
    setSendingTest(true);
    try {
      await onSave(form);
      await onSendTest();
      setMessage("测试消息已触发，请去飞书看是否收到。");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSendingTest(false);
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

      <div className="settings-form">
        <div className="settings-section-head">
          <strong>通知范围</strong>
          <span>你可以决定哪些通知保留，哪些静默。</span>
        </div>
        <div className="settings-toggle-list">
          {notificationOptions.map((option) => (
            <label className="settings-toggle" key={option.key}>
              <div className="settings-toggle__copy">
                <strong>{option.label}</strong>
                <span>{option.detail}</span>
              </div>
              <input
                type="checkbox"
                checked={Boolean(form[option.key])}
                onChange={(event) => patchForm({ [option.key]: event.target.checked } as Partial<AlertSettings>)}
              />
            </label>
          ))}
        </div>
      </div>

      <div className="settings-actions">
        <button className="ghost-button" type="button" onClick={onBack}>
          稍后再配
        </button>
        <button className="ghost-button" type="button" onClick={handleSendTest} disabled={saving || sendingTest}>
          {sendingTest ? "发送中..." : "发送测试消息"}
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
