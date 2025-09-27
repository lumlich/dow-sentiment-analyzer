/* TrendPanel.tsx
 * Minimal, dependency-free sparkline for decision confidence over time.
 * Expects real data from backend; shows an empty state when no data yet.
 */

import { LineChart, Activity } from "lucide-react";

export type Verdict = "BUY" | "SELL" | "HOLD";

export interface TrendPoint {
  ts: string;         // ISO 8601 (UTC)
  confidence: number; // 0.0 .. 1.0
  verdict: Verdict;
}

export interface TrendPanelProps {
  trend?: TrendPoint[] | null;
  title?: string;
  maxPoints?: number;
}

export default function TrendPanel({ trend, title = "Trend", maxPoints = 120 }: TrendPanelProps) {
  const points = Array.isArray(trend) ? trend.slice(-maxPoints) : [];

  const W = 720;
  const H = 160;
  const P = 14;

  const xs = points.map((p) => new Date(p.ts).getTime());
  const ys = points.map((p) => clamp01(p.confidence));

  if (!points.length) {
    return (
      <section
        aria-labelledby="trend-title"
        className="rounded-2xl border border-slate-200 bg-white p-4 dark:border-slate-700/60 dark:bg-slate-900/60"
      >
        <div className="mb-2 flex items-center gap-2">
          <LineChart className="h-5 w-5 text-slate-700 dark:text-slate-300" />
          <h2 id="trend-title" className="text-lg font-semibold text-slate-900 dark:text-slate-100">
            {title}
          </h2>
        </div>

        <p className="text-sm text-slate-600 dark:text-slate-400">
          No data yet — the chart appears after the first few decisions. Check back once the analyzer starts producing results.
        </p>
      </section>
    );
  }

  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const spanX = Math.max(1, maxX - minX);

  const minY = 0;
  const maxY = 1;

  const mapX = (t: number) => P + ((t - minX) / spanX) * (W - 2 * P);
  const mapY = (v: number) => {
    const y01 = (v - minY) / (maxY - minY);
    return H - P - y01 * (H - 2 * P);
  };

  const pathD = points
    .map((p, i) => {
      const x = mapX(new Date(p.ts).getTime());
      const y = mapY(clamp01(p.confidence));
      return `${i === 0 ? "M" : "L"} ${round2(x)} ${round2(y)}`;
    })
    .join(" ");

  const last = points[points.length - 1];
  const lastPct = Math.round(clamp01(last.confidence) * 100);
  const showDots = points.length <= 40;

  return (
    <section
      aria-labelledby="trend-title"
      className="rounded-2xl border border-slate-200 bg-white p-4 dark:border-slate-700/60 dark:bg-slate-900/60"
    >
      <div className="mb-2 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <LineChart className="h-5 w-5 text-slate-700 dark:text-slate-300" />
          <h2 id="trend-title" className="text-lg font-semibold text-slate-900 dark:text-slate-100">
            {title}
          </h2>
        </div>

        <div className="flex items-center gap-2 text-sm text-slate-700 dark:text-slate-300">
          <Activity className="h-4 w-4 opacity-80" />
          <span>
            Last: <span className="font-semibold text-slate-900 dark:text-slate-100">{lastPct}%</span>
          </span>
        </div>
      </div>

      <div className="mt-2 overflow-hidden rounded-xl border border-slate-200 bg-slate-50 dark:border-slate-800/60 dark:bg-slate-950/40">
        <svg
          role="img"
          aria-label="Decision confidence over time"
          viewBox={`0 0 ${W} ${H}`}
          className="h-48 w-full"
          preserveAspectRatio="none"
        >
          {([0.25, 0.5, 0.75] as const).map((p) => {
            const y = mapY(p);
            return (
              <line
                key={p}
                x1={P}
                x2={W - 2 * P + P}
                y1={y}
                y2={y}
                stroke="currentColor"
                opacity={0.15}
                className="text-slate-500"
              />
            );
          })}

          <rect
            x={P}
            y={P}
            width={W - 2 * P}
            height={H - 2 * P}
            fill="none"
            stroke="currentColor"
            opacity={0.2}
            className="text-slate-300 dark:text-slate-700"
            rx={8}
          />

          <path
            d={pathD}
            fill="none"
            stroke="currentColor"
            className="text-slate-700 dark:text-slate-200"
            strokeWidth={2}
            strokeLinejoin="round"
            strokeLinecap="round"
          />

          {showDots &&
            points.map((p, i) => {
              const x = mapX(new Date(p.ts).getTime());
              const y = mapY(clamp01(p.confidence));
              return (
                <circle
                  key={i}
                  cx={x}
                  cy={y}
                  r={2.5}
                  fill="currentColor"
                  className="text-slate-600 dark:text-slate-300"
                />
              );
            })}
        </svg>
      </div>

      <div className="mt-2 flex items-center justify-between text-xs text-slate-500 dark:text-slate-400">
        <span>Older</span>
        <span>Newer</span>
      </div>
    </section>
  );
}

function clamp01(v: number) {
  if (Number.isNaN(v)) return 0;
  return v < 0 ? 0 : v > 1 ? 1 : v;
}
function round2(n: number) {
  return Math.round(n * 100) / 100;
}
