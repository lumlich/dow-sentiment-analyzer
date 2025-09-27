/* app.tsx — production wiring + quick-wins (Freshness TTL, spinner, A11y) */
import { useEffect, useMemo, useState } from "preact/hooks";
import type { FunctionalComponent } from "preact";

import HowToReadBox from "./components/HowToReadBox";
import AlertsInfoBox from "./components/AlertsInfoBox";
import VerdictHero from "./components/VerdictHero";
import PanelHeader from "./components/PanelHeader";
import EvidenceList from "./components/EvidenceList";
import TrendPanel, { type TrendPanelProps } from "./components/TrendPanel";
import DebugStatus from "./components/DebugStatus";

import { fetchDecide, type Reason, type DecideResponse } from "./lib/api";
import "./app.css";

const HIDE_DEV = ((import.meta as any).env?.VITE_HIDE_DEV ?? "") === "1";
const FRESH_TTL_MS = Number((import.meta as any).env?.VITE_FRESH_TTL_MS ?? 5 * 60 * 1000); // default 5 min

export const App: FunctionalComponent = () => {
  // UI-only state
  const [reasonsExpanded, setReasonsExpanded] = useState(true);
  const [evidenceExpanded, setEvidenceExpanded] = useState(true);

  // Data state (real backend)
  const [verdict, setVerdict] = useState<"BUY" | "SELL" | "HOLD">("HOLD");
  const [confidence, setConfidence] = useState<number>(0.5);
  const [topReasons, setTopReasons] = useState<Reason[]>([]);
  const [evidence, setEvidence] = useState<Reason[]>([]);
  const [trendVals, setTrendVals] = useState<number[]>([]);
  const [trendLabels, setTrendLabels] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<string | null>(null);

  // Tick to recompute "fresh/stale" age labels
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNowMs(Date.now()), 15_000); // gentle 15s
    return () => clearInterval(t);
  }, []);

  // Fetch once on mount
  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        setLoading(true);
        setError(null);
        const data: DecideResponse = await fetchDecide();
        if (!mounted) return;
        setVerdict(data.verdict);
        setConfidence(data.confidence);
        setTopReasons(data.top_reasons ?? []);
        setEvidence(data.evidence ?? []);
        setTrendVals(data.trend?.values ?? []);
        setTrendLabels(data.trend?.labels ?? []);
        setUpdatedAt(new Date().toISOString());
      } catch (e: any) {
        setError(String(e?.message ?? e));
      } finally {
        setLoading(false);
      }
    })();
    return () => { mounted = false; };
  }, []);

  function fmtUtc(iso?: string | null): string {
    if (!iso) return "";
    const d = new Date(iso);
    const pad = (n: number) => (n < 10 ? `0${n}` : String(n));
    return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(
      d.getUTCMinutes()
    )} UTC`;
  }

  function fmtAge(ms: number): string {
    if (!isFinite(ms) || ms < 0) return "0s";
    const s = Math.floor(ms / 1000);
    const m = Math.floor(s / 60);
    const remS = s % 60;
    if (m <= 0) return `${remS}s`;
    return `${m}m ${remS}s`;
  }

  const freshness = useMemo(() => {
    if (!updatedAt) return { label: null as string | null, tone: "info" as const };
    const age = nowMs - new Date(updatedAt).getTime();
    const tone = age <= FRESH_TTL_MS ? "fresh" : "stale";
    const label = tone === "fresh" ? `Fresh ≤ ${Math.round(FRESH_TTL_MS / 60000)}m` : `Stale · ${fmtAge(age)} old`;
    return { label, tone };
  }, [nowMs, updatedAt]);

  // TrendPanel props adapter
  const trendProps = {
    title: "Decision confidence trend",
    series: trendVals,
    data: trendVals,
    labels: trendLabels,
    categories: trendLabels,
  } as unknown as TrendPanelProps;

  const Chevron = () => (
    <svg className="icon chev" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );

  return (
    <main className="container">
      <header className="section" aria-label="Header">
        <h1>DOW Sentiment Analyzer</h1>
        <p className="small">Live decision engine UI — production wiring</p>
      </header>

      <section className="section section-compact" aria-label="How to read this analyzer">
        <HowToReadBox />
      </section>

      <section className="section" aria-label="Get instant alerts">
        <AlertsInfoBox />
        {/* copy hint: "You’ll get a ping on verdict/confidence change." */}
      </section>

      <section className="section stack-4" aria-label="Main panels">
        <div className="card" aria-label="Verdict hero">
          <VerdictHero
            verdict={verdict}
            confidence={confidence}
            loading={loading}
            updatedAtLabel={fmtUtc(updatedAt)}
            freshnessLabel={freshness.label}
            freshnessTone={freshness.tone as any}
          />
          {error && <p className="small error" aria-live="assertive">Error: {error}</p>}
        </div>

        <div className="card" aria-label="Top reasons">
          <PanelHeader
            title="Top reasons"
            subtitle="Most influential signals driving the decision"
            actions={
              <button
                className="btn-chip"
                type="button"
                aria-pressed={reasonsExpanded}
                aria-controls="reasons-content"
                onClick={() => setReasonsExpanded((v) => !v)}
                title={reasonsExpanded ? "Collapse reasons" : "Expand reasons"}
              >
                {reasonsExpanded ? "Collapse" : "Expand"} <Chevron />
              </button>
            }
          />
          <div id="reasons-content" role="region" aria-live="polite">
            {reasonsExpanded ? <EvidenceList items={(topReasons as any) ?? []} /> : <p className="small">Collapsed.</p>}
          </div>
        </div>

        <div className="card" aria-label="Evidence">
          <PanelHeader
            title="Evidence"
            subtitle="Raw items captured from sources and system rules"
            actions={
              <button
                className="btn-chip"
                type="button"
                aria-pressed={evidenceExpanded}
                aria-controls="evidence-content"
                onClick={() => setEvidenceExpanded((v) => !v)}
                title={evidenceExpanded ? "Collapse evidence" : "Expand evidence"}
              >
                {evidenceExpanded ? "Collapse" : "Expand"} <Chevron />
              </button>
            }
          />
          <div id="evidence-content" role="region" aria-live="polite">
            {evidenceExpanded ? (
              (evidence?.length ?? 0) > 0 ? (
                <EvidenceList items={(evidence as any) ?? []} />
              ) : (
                <div className="stack-4">
                  <p className="small">No evidence yet — waiting for fresh items.</p>
                  <p className="small" style={{ opacity: 0.8 }}>Ingest: ON, next tick in ~5 min.</p>
                </div>
              )
            ) : (
              <p className="small">Collapsed.</p>
            )}
          </div>
        </div>

        <div className="card" aria-label="Trend">
          <PanelHeader title="Trend" />
          {trendVals.length > 0 ? <TrendPanel {...trendProps} /> : <p className="small">No trend yet.</p>}
        </div>
      </section>

      {!HIDE_DEV && import.meta.env.DEV && (
        <section className="section no-print" aria-label="Debug">
          <DebugStatus />
        </section>
      )}
    </main>
  );
};

export default App;
