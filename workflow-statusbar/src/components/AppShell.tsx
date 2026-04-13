import type { PropsWithChildren } from "react";

type AppShellProps = PropsWithChildren<{
  compact?: boolean;
}>;

export function AppShell({ compact = false, children }: AppShellProps) {
  return (
    <main className={compact ? "app-shell app-shell--compact" : "app-shell"}>
      <section className={compact ? "surface surface--compact" : "surface"}>
        {children}
      </section>
    </main>
  );
}
