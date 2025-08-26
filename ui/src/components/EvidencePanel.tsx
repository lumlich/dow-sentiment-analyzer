// ui/src/components/EvidencePanel.tsx
import { useMemo, useState } from 'preact/hooks';

export interface EvidenceItem {
  title: string;
  source: string;     // e.g., "Reuters"
  url?: string;       // optional
  sentiment: 'pos' | 'neg' | 'neu';
  time: string;       // ISO or human
}

interface EvidencePanelProps {
  items: EvidenceItem[];
}

/** Try to pretty-print time if it's ISO; otherwise return as-is. */
function formatTime(input?: string) {
  if (!input) return '';
  const d = new Date(input);
  if (Number.isNaN(d.getTime())) return input;
  return new Intl.DateTimeFormat(undefined, {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(d);
}

export function EvidencePanel({ items }: EvidencePanelProps) {
  const [panelOpen, setPanelOpen] = useState(true);

  // Per-item accordion state (default collapsed)
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
    <section class="panel evidence" aria-label="Evidence">
      <div class="panel-header row">
        <span>Evidence</span>
        <div class="row gap-8">
          <button
            class="btn-toggle"
            onClick={() => setAll(!allOpen)}
            disabled={!panelOpen || items.length === 0}
            aria-label={allOpen ? 'Collapse all evidence items' : 'Expand all evidence items'}
          >
            {allOpen ? 'Collapse all' : 'Expand all'}
          </button>
          <button
            class="btn-toggle"
            onClick={() => setPanelOpen((o) => !o)}
            aria-expanded={panelOpen}
            aria-controls="evidence-list"
          >
            {panelOpen ? 'Collapse' : 'Expand'}
          </button>
        </div>
      </div>

      {panelOpen && (
        <ul id="evidence-list" class="evidence-list">
          {items.map((it, i) => {
            const isOpen = !!openMap[i];
            const sentiment = (it.sentiment ?? 'neu').toUpperCase();
            const time = formatTime(it.time);

            return (
              <li key={i} class={`evidence-item ${isOpen ? 'open' : 'collapsed'}`}>
                <div class="evidence-row">
                  <button
                    class="accordion-toggle"
                    onClick={() => toggleItem(i)}
                    aria-expanded={isOpen}
                    aria-controls={`evidence-body-${i}`}
                    title={isOpen ? 'Collapse' : 'Expand'}
                  >
                    <span class={`chevron ${isOpen ? 'down' : 'right'}`} aria-hidden="true">▸</span>
                  </button>

                  <div class="evidence-title">
                    {it.url ? (
                      <a href={it.url} target="_blank" rel="noreferrer">
                        {it.title || '(untitled)'}
                      </a>
                    ) : (
                      it.title || '(untitled)'
                    )}
                  </div>

                  <span class={`badge ${it.sentiment}`}>{sentiment}</span>
                </div>

                {isOpen && (
                  <div id={`evidence-body-${i}`} class="evidence-meta">
                    <span class="source">{it.source || '—'}</span>
                    {time && (
                      <>
                        <span class="dot">•</span>
                        <time dateTime={it.time}>{time}</time>
                      </>
                    )}
                    {it.url && (
                      <>
                        <span class="dot">•</span>
                        <a class="visit-link" href={it.url} target="_blank" rel="noreferrer">
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
            <li class="evidence-empty">No evidence available.</li>
          )}
        </ul>
      )}
    </section>
  );
}
