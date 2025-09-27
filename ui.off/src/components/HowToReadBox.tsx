// src/components/HowToReadBox.tsx
import { useState } from "preact/hooks";

export default function HowToReadBox() {
  // Defaultně zavřené – zabírá málo místa, jde rozbalit na vyžádání.
  const [open, setOpen] = useState(false);

  const toggle = () => setOpen(o => !o);
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      toggle();
    }
  };

  return (
    <section class="section" aria-labelledby="howto-title">
      <div class="panel-header">
        <h2 id="howto-title">How to read this analyzer</h2>
        <button
          type="button"
          class="btn-chip"
          aria-expanded={open}
          aria-controls="howto-content"
          onClick={toggle}
          onKeyDown={onKey as any}
          title={open ? "Collapse helper" : "Expand helper"}
        >
          {open ? "Collapse" : "Expand"}
          <span
            aria-hidden
            class="icon chev"
            style={{
              display: "inline-block",
              transform: open ? "rotate(180deg)" : "none",
            }}
          >
            ▾
          </span>
        </button>
      </div>

      {open && (
        <div id="howto-content" class="stack-4" style={{ marginTop: 8 }}>
          <p class="small" style={{ margin: 0 }}>
            The analyzer ingests fresh market headlines and signals and outputs a
            <strong> decision</strong> for the Dow — <strong>BUY</strong>,{" "}
            <strong>SELL</strong>, or <strong>HOLD</strong> — together with a
            confidence score. It’s a directional aid, not financial advice.
          </p>

          {/* Co sbíráme */}
          <div class="card" style={{ padding: 14 }}>
            <div class="small" style={{ fontWeight: 600, marginBottom: 8 }}>
              What it ingests (inputs)
            </div>
            <ul
              class="small"
              style={{
                margin: 0,
                paddingLeft: 18,
                lineHeight: 1.75,
              }}
            >
              <li>
                <strong>News flow:</strong> major outlets (e.g., Reuters,
                Bloomberg, WSJ) with timestamps and source reliability.
              </li>
              <li>
                <strong>Macro prints &amp; policy tone:</strong> CPI/ISM, Fed
                commentary summaries.
              </li>
              <li>
                <strong>Market micro:</strong> breadth/sector performance, index
                futures levels, basic volatility cues (e.g., VIX mentions).
              </li>
            </ul>
          </div>

          {/* Jak rozhodujeme */}
          <div class="card" style={{ padding: 14 }}>
            <div class="small" style={{ fontWeight: 600, marginBottom: 8 }}>
              How it decides (process)
            </div>
            <ul
              class="small"
              style={{
                margin: 0,
                paddingLeft: 18,
                lineHeight: 1.75,
              }}
            >
              <li>
                Each item is classified as <strong>POS</strong> /{" "}
                <strong>NEG</strong> / <strong>NEU</strong> and scored by{" "}
                <em>recency</em>, <em>source</em>, and <em>salience</em>.
              </li>
              <li>
                Scores are aggregated into a rolling{" "}
                <strong>confidence&nbsp;%</strong> and mapped to a{" "}
                <strong>BUY/SELL/HOLD</strong> verdict.
              </li>
              <li>
                When new evidence lands, the verdict/confidence can update and
                alerts are triggered (if connected).
              </li>
            </ul>
          </div>

          {/* Co uvidíš v UI */}
          <div class="card" style={{ padding: 14 }}>
            <div class="small" style={{ fontWeight: 600, marginBottom: 8 }}>
              What you’ll see (panels)
            </div>
            <ul
              class="small"
              style={{
                margin: 0,
                paddingLeft: 18,
                lineHeight: 1.75,
              }}
            >
              <li>
                <strong>Verdict</strong> — the current recommendation for the
                Dow.
              </li>
              <li>
                <strong>Confidence&nbsp;%</strong> — signal strength (≈{" "}
                <em>0–49 low</em>, <em>50–69 moderate</em>, <em>70+ high</em>).
              </li>
              <li>
                <strong>Top reasons</strong> — distilled summaries carrying the
                most weight in the decision.
              </li>
              <li>
                <strong>Evidence</strong> — the raw news items (POS/NEG/NEU)
                with <em>source</em> and <em>time</em>.
              </li>
              <li>
                <strong>Trend</strong> — confidence over time once live data
                accumulates.
              </li>
              <li>
                <strong>Alerts</strong> — Discord/Slack notifications on decision
                or confidence changes.
              </li>
            </ul>
          </div>

          <p class="small" style={{ margin: 0 }}>
            Tip: click any evidence headline to open the source. Use alerts to
            catch shifts quickly; the analyzer updates as fresh information
            arrives.
          </p>
        </div>
      )}
    </section>
  );
}
