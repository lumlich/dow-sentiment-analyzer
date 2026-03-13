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

type BackendDecideResponse = {
  decision?: unknown;
  verdict?: unknown;
  confidence?: unknown;
  reasons?: unknown;
  top_contributors?: unknown;
  trend?: unknown;
};

async function parseJson(resp: Response): Promise<unknown> {
  const text = await resp.text();
  if (!text) {
    throw new Error("Empty response body from /api/decide");
  }
  try {
    return JSON.parse(text);
  } catch {
    throw new Error(`Invalid JSON from /api/decide: ${text}`);
  }
}

async function ensureOk(resp: Response): Promise<void> {
  if (resp.ok) return;
  const text = await resp.text();
  throw new Error(`HTTP ${resp.status} ${resp.statusText}: ${text || "<empty body>"}`);
}

function normalizeReasons(raw: unknown): Reason[] {
  if (!Array.isArray(raw)) return [];
  return raw.map((r: any) => ({
    message: typeof r === "string" ? r : String(r?.message ?? ""),
    weight: typeof r?.weight === "number" ? r.weight : undefined,
    kind: typeof r?.kind === "string" ? r.kind : undefined
  }));
}

function normalizeDecideResponse(raw: BackendDecideResponse): DecideResponse {
  const decisionRaw =
    typeof raw?.decision === "string"
      ? raw.decision
      : typeof raw?.verdict === "string"
      ? raw.verdict
      : null;

  if (!decisionRaw) {
    throw new Error("Malformed /api/decide response: missing decision");
  }

  const verdict = decisionRaw.toUpperCase();
  if (verdict !== "BUY" && verdict !== "SELL" && verdict !== "HOLD") {
    throw new Error(`Malformed /api/decide response: invalid decision '${decisionRaw}'`);
  }

  if (typeof raw?.confidence !== "number") {
    throw new Error("Malformed /api/decide response: missing numeric confidence");
  }

  const top_reasons = normalizeReasons(raw.reasons);

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
    Array.isArray((raw.trend as any)?.values) &&
    Array.isArray((raw.trend as any)?.labels)
      ? {
          values: (raw.trend as any).values as number[],
          labels: (raw.trend as any).labels as string[]
        }
      : undefined;

  return {
    verdict: verdict as Decision,
    confidence: raw.confidence,
    top_reasons,
    evidence,
    trend
  };
}

// GET /api/decide returns latest stable backend state for homepage cold start.
export async function fetchLatestDecision(): Promise<DecideResponse> {
  const resp = await fetch(DECIDE_URL, { method: "GET" });
  await ensureOk(resp);
  const raw = await parseJson(resp);
  return normalizeDecideResponse(raw as BackendDecideResponse);
}

// POST /api/decide executes a decision run with optional input payload.
export async function runDecision(req: DecideRequest = {}): Promise<DecideResponse> {
  const resp = await fetch(DECIDE_URL, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(req)
  });
  await ensureOk(resp);
  const raw = await parseJson(resp);
  return normalizeDecideResponse(raw as BackendDecideResponse);
}
