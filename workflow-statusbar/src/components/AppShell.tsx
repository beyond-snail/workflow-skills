import type { PropsWithChildren } from "react";

type AppShellProps = PropsWithChildren<{
  compact?: boolean;
}>;

export function AppShell({ compact = false, children }: AppShellProps) {
  const isWindows = typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent);
  const shellClass = [
    "app-shell",
    compact ? "app-shell--compact" : "",
    isWindows ? "app-shell--windows" : "",
  ].filter(Boolean).join(" ");

  const surfaceClass = [
    "surface",
    compact ? "surface--compact" : "",
    isWindows ? "surface--windows" : "",
  ].filter(Boolean).join(" ");

  return (
    <main className={shellClass}>
      <section className={surfaceClass}>
        {isWindows && !compact ? (
          <div className="window-dragbar" data-tauri-drag-region aria-hidden="true" />
        ) : null}
        {children}
      </section>
    </main>
  );
}
