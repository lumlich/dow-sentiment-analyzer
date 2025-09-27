import { Info } from "lucide-react";

export type Verdict = "BUY" | "SELL" | "HOLD";

export interface OverviewPanelProps {
  overview?: string | null;
  verdict: Verdict;
  confidence?: number | null;      // 0.0 .. 1.0
  reasons?: string[] | null;
  evidenceCount?: number | null;
  updatedAtIso?: string | null;    // optional ISO timestamp
}

export default function OverviewPanel({
  overview,
  verdict,
  confidence = null,
  reasons = null,
  evidenceCount = null,
  updatedAtIso = null,
}: OverviewPanelProps) {
  const pct = confidence == null ? null : Math.round(clamp01(confidence) * 100);
  const reasonsShort = (reasons ?? []).slice(0, 2);
  const fallback = composeFallback({ verdict, pct, reasons: reasonsShort, evidenceCount });

  const lastUpdate =
    updatedAtIso
      ? new Date(updatedAtIso).toLocaleString(undefined, {
          hour12: false,
          year: "numeric",
          month: "2-digit",
          day: "2-digit",
          hour: "2-digit",
          minute: "2-digit",
        })
      : null;

  return (
    <section
      aria-labelledby="overview-title"
      className="rounded-2xl border border-slate-200 bg-white p-4 dark:border-slate-700/60 dark:bg-slate-900/60"
    >
      <div className="mb-2 flex items-center gap-2">
        <Info className="h-5 w-5 text-slate-700 dark:text-slate-300" />
        <h2 id="overview-title" className="text-lg font-semibold text-slate-900 dark:text-slate-100">
          Overview
        </h2>
      </div>

      <p className="text-sm leading-6 text-slate-700 dark:text-slate-200">
        {overview?.trim() || fallback}
      </p>

      <div className="mt-3 text-xs text-slate-500 dark:text-slate-400">
        {lastUpdate ? <span>Last update: {lastUpdate}</span> : null}
      </div>
    </section>
  );
}

function clamp01(v: number) {
  if (Number.isNaN(v)) return 0;
  return v < 0 ? 0 : v > 1 ? 1 : v;
}

function composeFallback(opts: { verdict: Verdict; pct: number | null; reasons: string[]; evidenceCount: number | null }) {
  const { verdict, pct, reasons, evidenceCount } = opts;
  const verdictTxt = verdict;
  const confTxt = pct == null ? "" : ` with ${pct}% confidence`;
  const evTxt = typeof evidenceCount === "number" ? ` based on ${evidenceCount} evidence item${evidenceCount === 1 ? "" : "s"}` : "";
  const reasonsTxt = reasons.length ? ` Key drivers: ${reasons.join("; ")}.` : "";
  return `${verdictTxt}${confTxt}${evTxt}.` + (reasonsTxt ? ` ${reasonsTxt}` : "");
}
