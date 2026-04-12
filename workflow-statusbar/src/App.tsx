import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertSettingsPanel } from "./components/AlertSettingsPanel";
import { AppShell } from "./components/AppShell";
import { EmptyState } from "./components/EmptyState";
import { FocusCard } from "./components/FocusCard";
import { ProjectGroups } from "./components/ProjectGroups";
import { StatusCard } from "./components/StatusCard";
import type { AlertSettings, RuntimeState } from "./lib/types";

const windowLabel = getCurrentWindow().label;

function App() {
  const [state, setState] = useState<RuntimeState | null>(null);
  const [alertSettings, setAlertSettings] = useState<AlertSettings | null>(null);
  const [savingAlertSettings, setSavingAlertSettings] = useState(false);
  const [panelMode, setPanelMode] = useState<"dashboard" | "alert-settings">("dashboard");
  const [error, setError] = useState("");

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    async function load() {
      try {
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

  const groupedSummary = useMemo(() => {
    if (!state) {
      return [];
    }

    return [
      { label: "执行中", value: state.summary.execution },
      { label: "需求中", value: state.summary.requirement },
      { label: "待初始化", value: state.summary.bootstrap },
      { label: "已阻塞", value: state.summary.blocked },
      { label: "已完成", value: state.summary.done },
    ];
  }, [state]);

  const displayProject = state?.spotlight_project ?? state?.projects[0] ?? null;

  if (!state) {
    return (
      <AppShell compact={windowLabel === "floating"}>
        <EmptyState
          title="读取状态中"
          detail={error || "正在聚合 Codex 会话、最近项目与 workflow 状态。"}
        />
      </AppShell>
    );
  }

  if (windowLabel === "floating") {
    return null;
  }

  async function handleSaveAlertSettings(nextSettings: AlertSettings) {
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
    await invoke("send_test_alert_command");
  }

  if (panelMode === "alert-settings" && alertSettings) {
    return (
      <AppShell>
        <AlertSettingsPanel
          settings={alertSettings}
          saving={savingAlertSettings}
          onSave={handleSaveAlertSettings}
          onSendTest={handleSendTestAlert}
          onBack={() => setPanelMode("dashboard")}
        />
      </AppShell>
    );
  }

  return (
    <AppShell>
      <StatusCard
        key={`status-${displayProject?.path ?? "empty"}`}
        state={state}
        summary={groupedSummary}
        project={displayProject}
      />
      {displayProject ? (
        <FocusCard
          key={`focus-${displayProject.path}`}
          project={displayProject}
        />
      ) : null}
      <ProjectGroups groups={state.groups} spotlightPath={displayProject?.path ?? null} />
    </AppShell>
  );
}

export default App;
