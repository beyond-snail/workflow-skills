type EmptyStateProps = {
  title: string;
  detail: string;
};

export function EmptyState({ title, detail }: EmptyStateProps) {
  return (
    <section className="card card--empty">
      <span className="eyebrow">Workflow Statusbar</span>
      <h1>{title}</h1>
      <p>{detail}</p>
    </section>
  );
}
