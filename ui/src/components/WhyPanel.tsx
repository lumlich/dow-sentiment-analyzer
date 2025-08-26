// ui/src/components/WhyPanel.tsx
interface WhyPanelProps {
  reasons: string[]; // show top 3
}

export function WhyPanel({ reasons }: WhyPanelProps) {
  const top = reasons.slice(0, 3);
  return (
    <section class="panel why" aria-label="Why">
      <div class="panel-header">Top reasons</div>
      <ol class="why-list">
        {top.map((r, i) => (
          <li key={i} class="why-item">{r}</li>
        ))}
      </ol>
    </section>
  );
}
