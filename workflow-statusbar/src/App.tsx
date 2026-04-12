import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppShell } from "./components/AppShell";
import { EmptyState } from "./components/EmptyState";
import { FocusCard } from "./components/FocusCard";
import { ProjectGroups } from "./components/ProjectGroups";
import { StatusCard } from "./components/StatusCard";
import type { RuntimeState } from "./lib/types";

const windowLabel = getCurrentWindow().label;

function App() {
  const [state, setState] = useState<RuntimeState | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    async function load() {
      try {
        const payload = await invoke<RuntimeState>("get_runtime_state");
        setState(payload);
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
      unlisten = fn;
    });

    return () => {
      unlisten?.();
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
