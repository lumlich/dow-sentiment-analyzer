/* components/VerdictHero.tsx
 * UI-only hero panel for verdict + confidence bar + optional status chips.
 * No data fetching here; everything comes via props.
 */

import type { FunctionalComponent, ComponentChildren } from "preact";

type VerdictType = "BUY" | "SELL" | "HOLD";

interface VerdictHeroProps {
  verdict: VerdictType;
  confidence?: number;           // 0..1
  loading?: boolean;             // shows spinner and "Updating…"
  updatedAtLabel?: string | null; // preformatted UTC label from parent
  // Optional right-side status chip, e.g., "Fresh ≤ 5m" / "Stale · 12m old"
  freshnessLabel?: string | null;
  freshnessTone?: "fresh" | "stale" | "info";
  rightExtra?: ComponentChildren; // optional extra meta on the right
}

const labelFor = (v: VerdictType) => (v === "BUY" ? "BUY" : v === "SELL" ? "SELL" : "HOLD");
const badgeClass = (v: VerdictType) => (v === "BUY" ? "buy" : v === "SELL" ? "sell" : "hold");
const clamp01 = (n: number) => Math.max(0, Math.min(1, n));

const VerdictHero: FunctionalComponent<VerdictHeroProps> = ({
  verdict,
  confidence,
  loading,
  updatedAtLabel,
  freshnessLabel,
  freshnessTone = "info",
  rightExtra,
}) => {
  const conf = confidence == null ? undefined : clamp01(confidence);
  const pct = conf == null ? undefined : Math.round(conf * 100);

  return (
    <div className="verdict" aria-label="Verdict summary">
      <div className="verdict-main">
        <span className={`verdict-badge ${badgeClass(verdict)}`} aria-label={`Decision badge: ${verdict}`}>
          {labelFor(verdict)}
        </span>
        <div className="verdict-title" aria-live="polite" aria-atomic="true">
          {labelFor(verdict)} <span className="small">decision</span>
        </div>
      </div>

      {/* Right-side meta: freshness/status chip or custom content */}
      <div className="verdict-meta-right" aria-live="polite">
        {freshnessLabel ? (
          <span
            className={`status-chip ${freshnessTone === "fresh" ? "fresh" : freshnessTone === "stale" ? "stale" : ""}`}
          >
            {freshnessLabel}
          </span>
        ) : null}
        {rightExtra ?? null}
      </div>

      <div className="confwrap" aria-hidden={conf == null}>
        <div className="confmeta">
          <span>Confidence</span>
          <span>{pct != null ? `${pct}%` : "—"}</span>
        </div>
        <div className="confbar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct ?? 0}>
          <div className="confbar-fill" style={{ width: pct != null ? `${pct}%` : "0%" }} />
        </div>

        {/* Updated / Loading note */}
        {loading ? (
          <p className="small" aria-live="polite">
            <span className="spinner" aria-hidden="true" /> <span className="sr-only">Updating</span> Updating…
          </p>
        ) : updatedAtLabel ? (
          <p className="small" aria-live="polite">Last updated {updatedAtLabel}</p>
        ) : null}
      </div>
    </div>
  );
};

export default VerdictHero;
