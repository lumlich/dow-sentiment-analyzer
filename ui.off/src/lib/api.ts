// ui.off/src/lib/api.ts
export type Decision = "BUY" | "SELL" | "HOLD";

export type Reason = {
  message: string;
  weight?: number;
  kind?: string;
  source?: string;
  url?: string;
  time?: string;
  sentiment?: "pos" | "neg" | "neu";
};

export type DecideTrend = { values: number[]; labels: string[] };

export type DecideResponse = {
  verdict: Decision;
  confidence: number;
  top_reasons?: Reason[];
  evidence?: Reason[];
  trend?: DecideTrend;
};

export type DecideRequest = {
  inputs?: Array<{
    source: string;
    author?: string;
    text: string;
    weight?: number;
    time?: string;
  }>;
};

const API_BASE = (import.meta as any).env?.VITE_API_BASE ?? "/api";
const DECIDE_URL = `${API_BASE.replace(/\/+$/,"")}/decide`;

async function fetchJson<T>(input: RequestInfo | URL, init?: RequestInit): Promise<T> {
  const resp = await fetch(input, init);
  const text = await resp.text();
  try {
    return JSON.parse(text) as T;
  } catch {
    // backend může občas vrátit jen "BUY"/"SELL"/"HOLD"
    return text as unknown as T;
  }
}

/** Zavolá /api/decide a vrátí sjednocený DecideResponse. */
export async function fetchDecide(req: DecideRequest = {}): Promise<DecideResponse> {
  const body = Object.keys(req).length ? JSON.stringify(req) : "{}";
  const raw = await fetchJson<any>(DECIDE_URL, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body
  });

  // Podpora tvarů: "HOLD" | { decision, confidence, reasons, top_contributors, trend }
  const decisionRaw =
    typeof raw === "string"
      ? raw
      : typeof raw?.decision === "string"
      ? raw.decision
      : "HOLD";

  const verdict = (decisionRaw as string).toUpperCase() as Decision;

  const confidence = typeof raw?.confidence === "number" ? raw.confidence : 0.5;

  const top_reasons: Reason[] = Array.isArray(raw?.reasons)
    ? raw.reasons.map((r: any) => ({
        message: typeof r === "string" ? r : String(r?.message ?? ""),
        weight: typeof r?.weight === "number" ? r.weight : undefined,
        kind: typeof r?.kind === "string" ? r.kind : undefined
      }))
    : [];

  const evidence: Reason[] = Array.isArray(raw?.top_contributors)
    ? raw.top_contributors.map((c: any) => ({
        message: String(c?.text ?? ""),
        source: String(c?.source ?? ""),
        url: typeof c?.url === "string" ? c.url : undefined,
        time: typeof c?.ts === "string" ? c.ts : undefined,
        sentiment: "neu"
      }))
    : [];

  const trend: DecideTrend | undefined =
    raw?.trend &&
    Array.isArray(raw.trend?.values) &&
    Array.isArray(raw.trend?.labels)
      ? { values: raw.trend.values as number[], labels: raw.trend.labels as string[] }
      : undefined;

  return { verdict, confidence, top_reasons, evidence, trend };
}
