// ui/src/components/EvidencePanel.tsx
import { useMemo, useState } from "preact/hooks";

export interface EvidenceItem {
  title: string;
  source: string;     // e.g., "Reuters"
  url?: string;       // optional
  sentiment: "pos" | "neg" | "neu";
  time: string;       // ISO timestamp preferred
}

interface EvidencePanelProps {
  items: EvidenceItem[];
}

/** Format to strict UTC: 2025-09-18 09:05 UTC */
function formatUtc(input?: string) {
  if (!input) return "";
  const d = new Date(input);
  if (Number.isNaN(d.getTime())) return input;

  const pad = (n: number) => String(n).padStart(2, "0");
  const y = d.getUTCFullYear();
  const m = pad(d.getUTCMonth() + 1);
  const day = pad(d.getUTCDate());
  const hh = pad(d.getUTCHours());
  const mm = pad(d.getUTCMinutes());
  return `${y}-${m}-${day} ${hh}:${mm} UTC`;
}

/** Accept only real http(s) links (avoid "#" hash jumps). */
function httpLink(u?: string): string | undefined {
  return u && /^https?:\/\//i.test(u) ? u : undefined;
}

/** Map raw sentiment to readable label + class. */
function sentimentLabel(s: EvidenceItem["sentiment"]) {
  const up = (s ?? "neu").toUpperCase();
  return {
    text: up, // POS/NEG/NEU
    cls:
      s === "pos" ? "evidence-sent sent-pos" :
      s === "neg" ? "evidence-sent sent-neg" :
                    "evidence-sent sent-neu",
  };
}

export function EvidencePanel({ items }: EvidencePanelProps) {
  const [panelOpen, setPanelOpen] = useState(true);
  const [openMap, setOpenMap] = useState<Record<number, boolean>>({});

  const allOpen = useMemo(
    () => items.length > 0 && items.every((_, i) => openMap[i]),
    [items, openMap]
  );

  const toggleItem = (i: number) =>
    setOpenMap((m) => ({ ...m, [i]: !m[i] }));

  const setAll = (open: boolean) =>
    setOpenMap(() =>
      items.reduce<Record<number, boolean>>((acc, _, i) => {
        acc[i] = open;
        return acc;
      }, {})
    );

  return (
    <section class="section" aria-label="Evidence">
      <div class="panel-header">
        <h2 class="h2">Evidence</h2>
        <div class="panel-actions">
          <button
            type="button"
            class="btn-chip"
            onClick={() => setAll(!allOpen)}
            disabled={!panelOpen || items.length === 0}
            aria-disabled={!panelOpen || items.length === 0}
            title={allOpen ? "Collapse all evidence items" : "Expand all evidence items"}
          >
            {allOpen ? "Collapse all" : "Expand all"}
            <svg class="icon chev" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
              <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" stroke-width="2" />
            </svg>
          </button>

          <button
            type="button"
            class="btn-chip"
            onClick={() => setPanelOpen((o) => !o)}
            aria-pressed={panelOpen ? "true" : "false"}
            aria-controls="evidence-list"
            title={panelOpen ? "Collapse panel" : "Expand panel"}
          >
            {panelOpen ? "Collapse" : "Expand"}
            <svg class="icon chev" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
              <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" stroke-width="2" />
            </svg>
          </button>
        </div>
      </div>

      {panelOpen && (
        <ul id="evidence-list" class="evidence">
          {items.map((it, i) => {
            const isOpen = !!openMap[i];
            const href = httpLink(it.url);
            const timeStr = formatUtc(it.time);
            const s = sentimentLabel(it.sentiment);

            return (
              <li key={i} class={`evidence-item ${isOpen ? "open" : "collapsed"}`}>
                <div class="evidence-row">
                  {/* accordion toggle */}
                  <button
                    type="button"
                    class="btn-chip"
                    onClick={() => toggleItem(i)}
                    aria-expanded={isOpen}
                    aria-controls={`evidence-body-${i}`}
                    title={isOpen ? "Collapse" : "Expand"}
                  >
                    <span>{isOpen ? "Hide" : "Show"}</span>
                    <svg class="icon chev" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
                      <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" stroke-width="2" />
                    </svg>
                  </button>

                  {/* title (linked if valid URL) */}
                  <div class="evidence-title truncate">
                    {href ? (
                      <a href={href} target="_blank" rel="noreferrer">
                        {it.title || "(untitled)"}
                      </a>
                    ) : (
                      it.title || "(untitled)"
                    )}
                  </div>

                  {/* sentiment chip */}
                  <span class={s.cls}>{s.text}</span>
                </div>

                {isOpen && (
                  <div id={`evidence-body-${i}`} class="evidence-meta">
                    <span class="source truncate">{it.source || "—"}</span>
                    {timeStr && (
                      <>
                        <span class="dot">•</span>
                        <time dateTime={it.time}>{timeStr}</time>
                      </>
                    )}
                    {href && (
                      <>
                        <span class="dot">•</span>
                        <a class="visit-link" href={href} target="_blank" rel="noreferrer">
                          Open source
                        </a>
                      </>
                    )}
                  </div>
                )}
              </li>
            );
          })}

          {items.length === 0 && (
            <li class="evidence-empty small">No evidence available.</li>
          )}
        </ul>
      )}
    </section>
  );
}
