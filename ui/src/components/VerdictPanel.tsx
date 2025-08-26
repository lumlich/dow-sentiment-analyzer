// ui/src/components/VerdictPanel.tsx
import { useMemo } from 'preact/hooks';

type Verdict = 'BUY' | 'SELL' | 'HOLD';

interface VerdictPanelProps {
  verdict: Verdict;
  confidence?: number; // 0..1
}

export function VerdictPanel({ verdict, confidence = 0.74 }: VerdictPanelProps) {
  const colorClass = useMemo(() => {
    switch (verdict) {
      case 'BUY':
        return 'panel-verdict buy';
      case 'SELL':
        return 'panel-verdict sell';
      default:
        return 'panel-verdict hold';
    }
  }, [verdict]);

  const confidencePct = Math.round(confidence * 100);

  return (
    <section class={colorClass} aria-label="Verdict">
      <div class="panel-header">Verdict</div>
      <div class="verdict-value">{verdict}</div>
      <div class="confidence">Confidence: {confidencePct}%</div>
    </section>
  );
}
