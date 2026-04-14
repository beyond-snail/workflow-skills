import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { AlertSettingsPanel } from "./components/AlertSettingsPanel";
import { AppShell } from "./components/AppShell";
import { EmptyState } from "./components/EmptyState";
import { StatusCard } from "./components/StatusCard";
import type { AlertSettings, RuntimeState } from "./lib/types";

const isTauriRuntime = "__TAURI_INTERNALS__" in window;
const windowLabel = isTauriRuntime ? getCurrentWindow().label : "main";
const initialPanelMode =
  new URLSearchParams(window.location.search).get("panel") === "alert-settings"
    ? "alert-settings"
    : "dashboard";

type PanelViewport = {
  mode: "dashboard" | "alert-settings";
  height: number;
};

const mockProject = {
  name: "solo",
  path: "/mock/solo",
  workflow_stage: "done",
  gate_status: "已完成",
  health: "可能卡住",
  risk: "",
  current_req_id: "",
  current_req_title: "买卖端页面 UI/UX 优化",
  current_task_id: "TASK-2026-04-07-01",
  current_task_title: "买卖端页面 UI/UX 优化",
  current_task_status: "done",
  current_mode: "执行",
  last_sync_at: "刚刚",
  sync_source: "mock",
  is_blocked: false,
  is_active_by_codex: true,
  is_open_in_ide: true,
  progress_label: "任务 1 / 1",
  stage_label: "执行",
  codex_status: "stalled",
  codex_heartbeat_at: "41 分钟前",
  codex_thread_id: "mock-thread",
  codex_thread_name: "solo",
  last_message_role: "assistant",
  last_message_text: "下一步如果你要继续，我就直接进入支付页联调，并把下单链路里的异常提示一起收掉。",
  token_total: 166000,
  token_input: 163000,
  token_output: 3000,
  token_reasoning: 576,
  auto_resume_enabled: true,
} as const;

const mockState: RuntimeState = {
  codex: {
    status: "running",
    heartbeat_at: "7 秒前",
    active_thread_id: "mock-thread",
    active_thread_name: "看下 刚刚 改了些什么",
    last_message_role: "assistant",
    last_message_text: "下一步如果你要继续，我就直接进入支付页联调，并把下单链路里的异常提示一起收掉。",
    active_ide_project_name: "solo",
    active_project_path: mockProject.path,
    source: "mock",
    confidence: "test",
    process_running: true,
    auto_resume_enabled: true,
    monitored_project_name: "solo",
  },
  projects: [mockProject],
  groups: [],
  summary: {
    idle: 0,
    bootstrap: 0,
    requirement: 0,
    execution: 1,
    blocked: 0,
    done: 4,
  },
  spotlight_project: mockProject,
  updated_at: "刚刚",
};

const mockAlertSettings: AlertSettings = {
  mode: "feishu",
  local_notifications_enabled: true,
  remote_notifications_enabled: true,
  local_notify_task_completed: true,
  remote_notify_task_completed: true,
  local_notify_project_completed: true,
  remote_notify_project_completed: true,
  local_notify_project_blocked: true,
  remote_notify_project_blocked: true,
  local_notify_task_interrupted: true,
  remote_notify_task_interrupted: true,
  local_notify_auto_resume_failed: true,
  remote_notify_auto_resume_failed: true,
  bridge_endpoint: "",
  bridge_token: "",
  feishu_app_id: "cli_mock",
  feishu_app_secret: "mock",
  feishu_open_id: "ou_mock",
  feishu_chat_id: "",
};

function App() {
  const [state, setState] = useState<RuntimeState | null>(null);
  const [alertSettings, setAlertSettings] = useState<AlertSettings | null>(null);
  const [savingAlertSettings, setSavingAlertSettings] = useState(false);
  const [panelMode, setPanelMode] = useState<"dashboard" | "alert-settings">(initialPanelMode);
  const [error, setError] = useState("");
  const contentRef = useRef<HTMLDivElement | null>(null);
  const [viewport, setViewport] = useState<PanelViewport | null>(null);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    async function load() {
      try {
        if (!isTauriRuntime) {
          setState(mockState);
          setAlertSettings(mockAlertSettings);
          setError("");
          return;
        }

        const payload = await invoke<RuntimeState>("get_runtime_state");
        const settings = await invoke<AlertSettings>("get_alert_settings");
        setState(payload);
        setAlertSettings(settings);
        setError("");
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    }

    load();

    if (!isTauriRuntime) {
      return () => {};
    }

    listen<RuntimeState>("runtime-state", (event) => {
      setState(event.payload);
      setError("");
    }).then((fn) => {
      unlisteners.push(fn);
    });

    listen<boolean>("open-alert-settings", () => {
      setPanelMode("alert-settings");
    }).then((fn) => {
      unlisteners.push(fn);
    });

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!isTauriRuntime || windowLabel !== "main") {
      return;
    }

    let disposed = false;

    const measureContentHeight = () => {
      if (panelMode === "alert-settings") {
        const head = contentRef.current?.querySelector<HTMLElement>(".settings-layout__head");
        const body = contentRef.current?.querySelector<HTMLElement>(".settings-layout__body");
        const footer = contentRef.current?.querySelector<HTMLElement>(".settings-layout__footer");

        if (head && body && footer) {
          return Math.ceil(head.offsetHeight + body.scrollHeight + footer.offsetHeight + 24);
        }
      }

      return Math.ceil(contentRef.current?.scrollHeight ?? 0);
    };

    const syncWindowHeight = async () => {
      const contentHeight = measureContentHeight();
      if (!contentHeight) {
        return;
      }

      const monitor = await currentMonitor().catch(() => null);
      const monitorHeight = monitor?.size.height ?? window.screen.availHeight ?? 900;
      const maxHeight = Math.max(320, Math.min(760, monitorHeight - 96));
      const nextHeight = Math.min(maxHeight, contentHeight + 12);

      if (disposed) {
        return;
      }

      setViewport({ mode: panelMode, height: nextHeight - 12 });
      invoke("sync_main_window_size", { contentHeight }).catch(() => {
        // Ignore resize failures outside the Tauri runtime.
      });
    };

    const runAfterPaint = () => {
      requestAnimationFrame(() => {
        syncWindowHeight();
      });
    };

    runAfterPaint();

    const observer = new ResizeObserver(() => {
      runAfterPaint();
    });

    if (contentRef.current) {
      observer.observe(contentRef.current);
    }

    window.addEventListener("resize", runAfterPaint);

    return () => {
      disposed = true;
      observer.disconnect();
      window.removeEventListener("resize", runAfterPaint);
    };
  }, [panelMode, state, alertSettings]);

  const displayProject = state?.spotlight_project ?? null;
  const openProjects = state?.projects.filter((project) => project.is_open_in_ide) ?? [];
  const secondaryOpenProjects = openProjects.filter((project) => project.path !== displayProject?.path);

  if (!state) {
    return (
      <AppShell compact={windowLabel === "floating"}>
        <div className="panel-content" ref={contentRef}>
          <EmptyState
            title="读取状态中"
            detail={error || "正在聚合 Codex 会话、最近项目与 workflow 状态。"}
          />
        </div>
      </AppShell>
    );
  }

  if (windowLabel === "floating") {
    return null;
  }

  async function handleSaveAlertSettings(nextSettings: AlertSettings) {
    if (!isTauriRuntime) {
      setAlertSettings(nextSettings);
      return;
    }

    setSavingAlertSettings(true);
    try {
      const saved = await invoke<AlertSettings>("save_alert_settings_command", {
        settings: nextSettings,
      });
      setAlertSettings(saved);
    } finally {
      setSavingAlertSettings(false);
    }
  }

  async function handleSendTestAlert() {
    if (!isTauriRuntime) {
      return;
    }

    await invoke("send_test_alert_command");
  }

  if (panelMode === "alert-settings" && alertSettings) {
    const panelMaxHeight = viewport?.mode === "alert-settings" ? viewport.height : undefined;

    return (
      <AppShell>
        <div className="panel-content" ref={contentRef} style={{ maxHeight: panelMaxHeight }}>
          <AlertSettingsPanel
            settings={alertSettings}
            saving={savingAlertSettings}
            onSave={handleSaveAlertSettings}
            onSendTest={handleSendTestAlert}
            onBack={() => setPanelMode("dashboard")}
          />
        </div>
      </AppShell>
    );
  }

  return (
    <AppShell>
      <div className="panel-content panel-content--dashboard" ref={contentRef}>
        <StatusCard
          key={`status-${displayProject?.path ?? "empty"}`}
          state={state}
          project={displayProject}
        />
        {secondaryOpenProjects.map((project) => (
          <StatusCard
            key={`status-secondary-${project.path}`}
            compact
            state={state}
            project={project}
          />
        ))}
      </div>
    </AppShell>
  );
}

export default App;
