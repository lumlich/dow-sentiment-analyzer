// src/hooks/useDecide.ts
// Fetch /decide from API_BASE and normalize backend shape to UI shape.

import { useEffect, useMemo, useRef, useState } from "react";

export type Verdict = "BUY" | "SELL" | "HOLD";

export interface TrendPoint {
  ts: string;         // ISO UTC
  confidence: number; // 0.0 .. 1.0
  verdict: Verdict;
}

export interface EvidenceItem {
  // NOTE: UI panels používají vlastní tvar; ten si mapujeme v Home.tsx
  stance?: "POS" | "NEG" | "NEU";
  title: string;
  source: string;
  time: string;       // ISO UTC
  url?: string;
}

export interface DecideResponse {
  schema_version?: string;
  verdict: Verdict;            // normalized from backend `decision`
  confidence: number;          // 0.0 .. 1.0
  reasons?: string[];          // normalized from backend [{message,...}]
  evidence?: EvidenceItem[];   // passthrough if backend ever sends it
  trend?: TrendPoint[];        // passthrough if backend ever sends it
}

type Status = "idle" | "loading" | "success" | "error";

export function useDecide(options?: { refreshMs?: number }) {
  const { refreshMs = 0 } = options ?? {};
  const [status, setStatus] = useState<Status>("idle");
  const [data, setData] = useState<DecideResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const base = (import.meta.env.VITE_API_BASE as string | undefined) ?? "";
  const url = useMemo(() => base.replace(/\/+$/, "") + "/decide", [base]);

  const abortRef = useRef<AbortController | null>(null);

  const fetchOnce = async () => {
    abortRef.current?.abort();
    const ac = new AbortController();
    abortRef.current = ac;

    try {
      setStatus("loading");
      setError(null);

      const res = await fetch(url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({}),      // tvůj backend zvládá prázdné tělo
        signal: ac.signal,
      });

      if (!res.ok) {
        const txt = await safeText(res);
        throw new Error(`HTTP ${res.status} ${res.statusText} — ${txt || "no body"}`);
      }

      const raw: any = await res.json();

      // 🔹 Normalizace backend → UI
      const verdictRaw = (raw.verdict ?? raw.decision ?? "HOLD").toString().toUpperCase();
      const verdict: Verdict = verdictRaw === "BUY" || verdictRaw === "SELL" ? verdictRaw : "HOLD";

      const reasons: string[] = Array.isArray(raw.reasons)
        ? raw.reasons
            .map((r: any) => (typeof r === "string" ? r : r?.message))
            .filter(Boolean)
        : [];

      const normalized: DecideResponse = {
        schema_version: "v1",
        verdict,
        confidence: typeof raw.confidence === "number" ? raw.confidence : 0,
        reasons,
        evidence: Array.isArray(raw.evidence) ? raw.evidence : undefined,
        trend: Array.isArray(raw.trend) ? raw.trend : undefined,
      };

      setData(normalized);
      setStatus("success");
    } catch (e: any) {
      if (e?.name === "AbortError") return;
      setError(e?.message || "Unknown error");
      setStatus("error");
    }
  };

  useEffect(() => {
    fetchOnce();
    if (refreshMs > 0) {
      const id = setInterval(fetchOnce, refreshMs);
      return () => {
        clearInterval(id);
        abortRef.current?.abort();
      };
    }
    return () => {
      abortRef.current?.abort();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url, refreshMs]);

  return { status, data, error, refresh: fetchOnce };
}

async function safeText(res: Response) {
  try { return await res.text(); } catch { return ""; }
}
