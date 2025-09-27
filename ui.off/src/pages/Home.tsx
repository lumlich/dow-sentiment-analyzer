// src/pages/Home.tsx
import AlertsInfoBox from "../components/AlertsInfoBox";
import HowToReadBox from "../components/HowToReadBox";

import { VerdictPanel } from "../components/VerdictPanel";
import { WhyPanel } from "../components/WhyPanel";
import { EvidencePanel } from "../components/EvidencePanel";

import VerdictHero from "../components/VerdictHero";
import TrendPanel from "../components/TrendPanel";
import DebugStatus from "../components/DebugStatus";
import OverviewPanel from "../components/OverviewPanel";

import { useDecide } from "../hooks/useDecide";
import type { EvidenceItem as PanelEvidenceItem } from "../components/EvidencePanel";
import type { EvidenceItem as ApiEvidenceItem } from "../hooks/useDecide";

export default function Home() {
  const { data, status, error, refresh } = useDecide();

  const verdict = (data?.verdict ?? "HOLD") as "BUY" | "SELL" | "HOLD";
  const reasons = data?.reasons ?? [];

  // Map evidence API -> panel shape (with normalized UTC time label)
  const apiItems: ApiEvidenceItem[] = data?.evidence ?? [];
  const itemsForPanel: PanelEvidenceItem[] = apiItems.map(toPanelEvidenceItem);

  const trend = data?.trend ?? null;
  const overview = (data as any)?.overview ?? null;
  const evidenceCount = itemsForPanel.length;
  const updatedAtIso = trend?.length ? trend[trend.length - 1].ts : null;

  return (
    <main className="container dense">
      {/* 1) How-to-read (compact card; default collapse handled by the component) */}
      <section className="section section-compact">
        <HowToReadBox />
      </section>

      {/* 2) Alerts */}
      <section className="section section-compact">
        <AlertsInfoBox />
      </section>

      {/* 3) Hero verdict */}
      <section className="section">
        <VerdictHero verdict={verdict} />
      </section>

      {/* 4) Controls */}
      <section className="section section-compact">
        <VerdictPanel verdict={verdict} />
      </section>

      {/* 5) Top reasons */}
      <section className="section">
        <WhyPanel reasons={reasons} />
      </section>

      {/* 6) Evidence */}
      <section className="section">
        <EvidencePanel items={itemsForPanel} />
      </section>

      {/* 7) Trend */}
      <section className="section section-compact">
        <TrendPanel trend={trend || undefined} />
      </section>

      {/* 8) Overview */}
      <section className="section section-compact">
        <OverviewPanel
          overview={overview}
          verdict={verdict}
          confidence={data?.confidence ?? null}
          reasons={reasons}
          evidenceCount={evidenceCount}
          updatedAtIso={updatedAtIso}
        />
      </section>

      {/* 9) Debug */}
      <section className="section no-print section-compact">
        <DebugStatus />
        {status === "error" && (
          <p className="small" style={{ color: "#ff5d5d", marginTop: 8 }}>
            Trend/decide fetch failed: {error}{" "}
            <button className="btn-chip" onClick={refresh}>
              Retry
            </button>
          </p>
        )}
      </section>
    </main>
  );
}

/**
 * Convert API evidence item -> Panel evidence item.
 * Ensures `time` is always a "YYYY-MM-DD HH:MM UTC" label.
 */
function toPanelEvidenceItem(it: ApiEvidenceItem): PanelEvidenceItem {
  const raw = (it as any).stance ?? (it as any).sentiment ?? "NEU";
  const s = typeof raw === "string" ? raw.toLowerCase() : "neu";
  const sentiment: "pos" | "neg" | "neu" =
    s === "pos" || s === "neg" || s === "neu" ? (s as any) : "neu";

  const timeLabel = toUtcLabel(it.time);

  return {
    sentiment,
    title: it.title,
    source: it.source,
    time: timeLabel, // <- normalized UTC label
    url: it.url,
  } as PanelEvidenceItem;
}

/**
 * Build a "YYYY-MM-DD HH:MM UTC" label from a variety of inputs:
 * - "HH:MM"     -> uses today's UTC date + provided HH:MM
 * - ISO string  -> parsed as Date and formatted in UTC
 * - number/Date -> used directly
 * - undefined   -> uses current time (now) in UTC
 */
function toUtcLabel(input?: string | number | Date | null): string {
  let d: Date | null = null;

  if (typeof input === "string") {
    // "HH:MM" only?
    const hhmm = input.trim();
    const m = /^(\d{2}):(\d{2})$/.exec(hhmm);
    if (m) {
      const [_, hh, mm] = m;
      const now = new Date();
      // Construct today's date at provided HH:MM in UTC
      d = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate(), Number(hh), Number(mm), 0, 0));
    } else {
      // Try ISO or other parseable strings
      const parsed = new Date(input);
      if (!isNaN(parsed.getTime())) d = parsed;
    }
  } else if (typeof input === "number") {
    const parsed = new Date(input);
    if (!isNaN(parsed.getTime())) d = parsed;
  } else if (input instanceof Date) {
    d = input;
  }

  if (!d) d = new Date(); // fallback: now

  const y = d.getUTCFullYear();
  const mo = pad2(d.getUTCMonth() + 1);
  const da = pad2(d.getUTCDate());
  const hh = pad2(d.getUTCHours());
  const mm = pad2(d.getUTCMinutes());

  return `${y}-${mo}-${da} ${hh}:${mm} UTC`;
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}
