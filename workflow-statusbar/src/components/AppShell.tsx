import type { PropsWithChildren } from "react";

type AppShellProps = PropsWithChildren<{
  compact?: boolean;
  onMouseLeave?: () => void;
}>;

export function AppShell({ compact = false, onMouseLeave, children }: AppShellProps) {
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
    <main className={shellClass} onMouseLeave={onMouseLeave}>
      <section className={surfaceClass}>
        {children}
      </section>
    </main>
  );
}
