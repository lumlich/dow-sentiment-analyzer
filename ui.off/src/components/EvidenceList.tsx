/* components/EvidenceList.tsx
 * Presentational list for evidence/reasons with improved a11y and safe links.
 */

import type { FunctionalComponent } from "preact";

type Sent = "pos" | "neg" | "neu";

export type EvidenceRow = {
  // Evidence shape
  title?: string;
  source?: string;
  url?: string;
  sentiment?: Sent;
  time?: string | number | Date;

  // Reason shape
  message?: string;
  kind?: string;
};

type Props = { items: EvidenceRow[] };

function sentLabel(s?: string): "POS" | "NEG" | "NEU" {
  const v = (s ?? "neu").toLowerCase();
  return v === "pos" ? "POS" : v === "neg" ? "NEG" : "NEU";
}

function kindPretty(kind?: string): string | null {
  if (!kind) return null;
  const k = kind.toLowerCase();
  if (k === "threshold") return "system";
  if (k === "rule") return "system";
  return k;
}

function toUtcLabel(input?: string | number | Date | null): string | null {
  if (!input) return null;
  let d: Date | null = null;
  if (typeof input === "string") {
    const m = /^(\d{2}):(\d{2})$/.exec(input.trim());
    if (m) {
      const now = new Date();
      d = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate(), Number(m[1]), Number(m[2]), 0, 0));
    } else {
      const parsed = new Date(input);
      if (!isNaN(parsed.getTime())) d = parsed;
    }
  } else if (typeof input === "number") {
    const parsed = new Date(input);
    if (!isNaN(parsed.getTime())) d = parsed;
  } else if (input instanceof Date) {
    d = input;
  }
  if (!d) return null;
  const pad = (n: number) => (n < 10 ? `0${n}` : String(n));
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(
    d.getUTCMinutes()
  )} UTC`;
}

const EvidenceList: FunctionalComponent<Props> = ({ items = [] }) => {
  return (
    <div className="evidence" role="list">
      {items.map((it, i) => {
        const title = (it.title ?? it.message ?? "").trim();
        const sent = (it.sentiment ?? "neu") as Sent;
        const time = toUtcLabel(it.time);
        const kind = kindPretty(it.kind ?? undefined);

        const bits: string[] = [];
        if (it.source) bits.push(it.source);
        if (time) bits.push(time);
        if (kind) bits.push(kind); // e.g. "system"

        const meta = bits.join(" · ");
        const metaId = `ev-meta-${i}`;
        const common = (
          <>
            <span className={`evidence-sent sent-${sent}`} aria-hidden="true">{sentLabel(sent)}</span>
            <span className="evidence-title truncate">{title || "—"}</span>
            <span id={metaId} className="evidence-meta">{meta || ""}</span>
          </>
        );

        return it.url ? (
          <a
            key={i}
            className="evidence-row"
            role="listitem"
            href={it.url}
            target="_blank"
            rel="noopener noreferrer"
            aria-label={title || "item"}
            aria-describedby={meta ? metaId : undefined}
          >
            {common}
          </a>
        ) : (
          <div key={i} className="evidence-row" role="listitem" aria-label={title || "item"} aria-describedby={meta ? metaId : undefined}>
            {common}
          </div>
        );
      })}
    </div>
  );
};

export default EvidenceList;
