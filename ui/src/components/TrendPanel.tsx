// ui/src/components/TrendPanel.tsx
import { useMemo } from 'preact/hooks';

type Point = { time: string; value: number };

export type TrendPanelProps = {
  /** Časová řada pro vykreslení (posloupnost hodnot v čase). */
  series: Point[];
  /** Výška SVG plochy v pixelech (šířka je 100%). */
  height?: number;
  /** Tloušťka čáry sparkline. */
  strokeWidth?: number;
  /** Zvýraznit poslední bod (větší tečka + jemný pulse). */
  emphasizeLast?: boolean;
  /** Popisek panelu (volitelné). */
  title?: string;
};

/**
 * TrendPanel – minimalistický sparkline s výraznějším posledním bodem.
 * - Responsivní na šířku (SVG width=100%).
 * - Bez externích knihoven.
 * - Respektuje prefers-reduced-motion (vypne puls).
 */
export function TrendPanel({
  series,
  height = 64,
  strokeWidth = 2,
  emphasizeLast = true,
  title = 'Sentiment trend',
}: TrendPanelProps) {
  const hasData = series && series.length > 0;

  const { pathD, min, max } = useMemo(() => {
    if (!hasData) {
      return { pathD: '', min: 0, max: 1 };
    }

    const n = series.length;
    const values = series.map((p) => p.value);
    const min = Math.min(...values);
    const max = Math.max(...values);
    const span = max - min || 1;

    // Normalized coordinates in [0..1]; y inverted so higher values are visually higher
    const xs = values.map((_, i) => (n === 1 ? 0.5 : i / (n - 1)));
    const ys = values.map((v) => 1 - (v - min) / span);

    const cmds: string[] = [];
    xs.forEach((x, i) => {
      const y = ys[i];
      cmds.push(`${i === 0 ? 'M' : 'L'} ${x} ${y}`);
    });

    return { pathD: cmds.join(' '), min, max };
  }, [hasData, series]);

  // Last point (normalized 0..1)
  const lastIdx = hasData ? series.length - 1 : -1;

  let lastX = 0.5;
  let lastY = 0.5;

  if (hasData) {
    const n = series.length;
    lastX = n === 1 ? 0.5 : lastIdx / (n - 1);

    const values = series.map((p) => p.value);
    const span = (max - min) || 1;
    lastY = 1 - (values[lastIdx] - min) / span;

    // clamp
    lastX = Math.max(0, Math.min(1, lastX));
    lastY = Math.max(0, Math.min(1, lastY));
  }

  // Colors via CSS vars with fallbacks
  const strokeColor = 'var(--text, #e8ecf1)';
  const gridColor = 'var(--border, #1f2933)';
  const fillArea = 'none';
  const vb = `0 0 1 1`;

  return (
    <section
      aria-label={title}
      style={{
        background: 'var(--panel)',
        border: `1px solid var(--border)`,
        borderRadius: '12px',
        padding: '10px 12px',
        color: 'var(--text)',
      }}
    >
      <style>{`
        @keyframes trend-pulse {
          0% { r: 0; opacity: 0.6; }
          70% { r: 0.08; opacity: 0.15; }
          100% { r: 0.1; opacity: 0; }
        }
        [data-trend] .trend-last-dot {
          vector-effect: non-scaling-stroke;
        }
        @media (prefers-reduced-motion: no-preference) {
          [data-trend] .trend-pulse {
            animation: trend-pulse 1.8s ease-out infinite;
            transform-origin: center;
          }
        }
      `}</style>

      <div class="flex items-center justify-between mb-2">
        <h2 class="text-sm" style={{ color: 'var(--muted)' }}>{title}</h2>
        {hasData && (
          <div class="text-xs" style={{ color: 'var(--muted)' }}>
            Min {min.toFixed(2)} · Max {max.toFixed(2)}
          </div>
        )}
      </div>

      <div style={{ width: '100%', height }}>
        <svg
          data-trend
          viewBox={vb}
          width="100%"
          height="100%"
          preserveAspectRatio="none"
          style={{ display: 'block', background: 'transparent' }}
        >
          {/* subtle baseline */}
          <line x1="0" y1="1" x2="1" y2="1" stroke={gridColor} stroke-width="0.004" />

          {/* path */}
          {hasData && (
            <path
              d={pathD}
              fill={fillArea}
              stroke={strokeColor}
              stroke-width={strokeWidth / 100}
              vector-effect="non-scaling-stroke"
            />
          )}

          {/* last point highlight */}
          {hasData && emphasizeLast && (
            <>
              <circle class="trend-pulse" cx={lastX} cy={lastY} r="0.06" fill={strokeColor} opacity="0.12" />
              <circle class="trend-last-dot" cx={lastX} cy={lastY} r="0.022" stroke={strokeColor} stroke-width="0.012" fill="var(--bg, #0b0f14)" />
              <circle class="trend-last-dot" cx={lastX} cy={lastY} r="0.01" fill={strokeColor} />
            </>
          )}
        </svg>
      </div>
    </section>
  );
}

export default TrendPanel;
