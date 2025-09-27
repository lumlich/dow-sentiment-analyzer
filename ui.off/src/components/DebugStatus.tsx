// src/components/DebugStatus.tsx
// Dev helper: call absolute API_BASE (no Vite proxy), show quick results.

import { useEffect, useMemo, useState } from "react";

type Ping = { ok: boolean; code?: number; text?: string };
type DecidePreview = { ok: boolean; code?: number; body?: any; err?: string };

export default function DebugStatus() {
  const apiBase = (import.meta.env.VITE_API_BASE as string | undefined)?.replace(/\/+$/, "") || "";
  const healthUrl = useMemo(() => (apiBase ? `${apiBase}/health` : "/health"), [apiBase]);
  const decideUrl = useMemo(() => (apiBase ? `${apiBase}/decide` : "/decide"), [apiBase]);

  const [ping, setPing] = useState<Ping | null>(null);
  const [decide, setDecide] = useState<DecidePreview | null>(null);
  const [loading, setLoading] = useState(false);

  // Health ping (GET)
  useEffect(() => {
    (async () => {
      try {
        const r = await fetch(healthUrl, { method: "GET" });
        const t = await r.text().catch(() => "");
        setPing({ ok: r.ok, code: r.status, text: t.trim() });
      } catch (e: any) {
        setPing({ ok: false, text: e?.message || "network error" });
      }
    })();
  }, [healthUrl]);

  // Manual /decide test (POST)
  const onTestDecide = async () => {
    setLoading(true);
    setDecide(null);
    try {
      const r = await fetch(decideUrl, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({}),
      });
      const body = await r.json().catch(async () => (await r.text()));
      setDecide({ ok: r.ok, code: r.status, body });
    } catch (e: any) {
      setDecide({ ok: false, err: e?.message || "network error" });
    } finally {
      setLoading(false);
    }
  };

  return (
    <section className="section no-print" aria-labelledby="dbg-title">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
        <h2 id="dbg-title">Debug</h2>
        <span className="small" style={{ opacity: 0.7 }}>
          API_BASE: {apiBase || "(proxy/relative)"}
        </span>
      </div>

      <div className="stack-4" style={{ marginTop: 8 }}>
        <div className="card">
          <div className="small">
            Health:{" "}
            {ping
              ? ping.ok
                ? <span style={{ color: "#2fbf71" }}>OK ({ping.code})</span>
                : <span style={{ color: "#ff5d5d" }}>FAIL ({ping.code ?? "net"})</span>
              : "…"}
            {ping?.text ? <span style={{ marginLeft: 8, opacity: 0.8 }}>“{ping.text}”</span> : null}
          </div>
        </div>

        <div className="card">
          <button
            onClick={onTestDecide}
            disabled={loading}
            className="btn-chip"
            aria-pressed={loading ? "true" : "false"}
          >
            {loading ? "Testing…" : "Test /decide"}
          </button>

          {decide && (
            <div className="small" style={{ marginTop: 10, whiteSpace: "pre-wrap" }}>
              {decide.ok ? (
                <>
                  <div style={{ color: "#2fbf71" }}>OK ({decide.code})</div>
                  <code style={{ display: "block", marginTop: 6 }}>
                    {typeof decide.body === "string"
                      ? decide.body
                      : JSON.stringify(decide.body, null, 2)}
                  </code>
                </>
              ) : (
                <>
                  <div style={{ color: "#ff5d5d" }}>FAIL ({decide.code ?? "net"})</div>
                  {decide.err ? <div>{decide.err}</div> : null}
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
