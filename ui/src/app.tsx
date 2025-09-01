// ui/src/app.tsx
import { useEffect, useRef, useState } from 'preact/hooks';
import { VerdictPanel } from './components/VerdictPanel';
import { WhyPanel } from './components/WhyPanel';
import { EvidencePanel } from './components/EvidencePanel';
import type { EvidenceItem } from './components/EvidencePanel';
import { TrendPanel } from './components/TrendPanel';
import './app.css';

type Decision = 'BUY' | 'SELL' | 'HOLD';

type ApiEvidence = {
  title?: string;
  source?: string;
  url?: string;
  sentiment?: 'pos' | 'neg' | 'neu';
  time?: string;
};

type ApiResponse = {
  decision?: Decision;
  confidence?: number;
  reasons?: string[];
  evidence?: ApiEvidence[];
  contributors?: string[];
};

type Mode = 'decide' | 'analyze';

// --- Demo input for /decide (replace later with real data) ---
const DEMO_BATCH = {
  inputs: [
    {
      source: 'demo',
      author: 'system',
      text: 'FOMC holds rates; guidance dovish tilt; futures up pre-market.',
      weight: 1.0,
      time: new Date().toISOString(),
    },
    {
      source: 'demo',
      author: 'newswire',
      text: 'White House signals infrastructure tranche approval.',
      weight: 0.8,
      time: new Date().toISOString(),
    },
  ],
};

const DECIDE_ENDPOINT = '/decide';
const ANALYZE_ENDPOINT = '/analyze';
const LS_KEY = 'useDecide';
const TREND_MAX = 40;

// Read AI headers safely (case-insensitive fallback)
function readAiHeaders(headers: Headers) {
  const usedRaw = headers.get('X-AI-Used') ?? headers.get('x-ai-used');
  const reasonRaw = headers.get('X-AI-Reason') ?? headers.get('x-ai-reason');
  const aiUsed = usedRaw === '1';
  const aiReason = reasonRaw ?? '';
  return { aiUsed, aiReason };
}

// Normalize /decide response into a shape panels can work with
function normalizeDecidePayload(
  raw: unknown
): { decision?: Decision; confidence?: number; reasons: string[]; evidence: ApiEvidence[] } {
  let decision: Decision | undefined;
  if (typeof raw === 'string') {
    const upper = raw.toUpperCase();
    if (upper === 'BUY' || upper === 'SELL' || upper === 'HOLD') {
      decision = upper as Decision;
    }
  } else if (raw && typeof raw === 'object' && 'decision' in (raw as any)) {
    const d = (raw as any).decision;
    if (typeof d === 'string') {
      const upper = d.toUpperCase();
      if (upper === 'BUY' || upper === 'SELL' || upper === 'HOLD') {
        decision = upper as Decision;
      }
    }
  }
  return {
    decision,
    confidence: undefined,
    reasons: [],
    evidence: [],
  };
}

function decisionToValue(d?: Decision): number | null {
  if (!d) return null;
  if (d === 'BUY') return 1;
  if (d === 'SELL') return -1;
  return 0; // HOLD
}

export function App() {
  const [data, setData] = useState<ApiResponse>({
    decision: undefined,
    confidence: undefined,
    reasons: [],
    evidence: [],
    contributors: [],
  });

  const [mode, setMode] = useState<Mode>(() => {
    const saved = typeof localStorage !== 'undefined' ? localStorage.getItem(LS_KEY) : null;
    return saved === '1' ? 'decide' : 'analyze';
  });

  const [aiUsed, setAiUsed] = useState<boolean>(false);
  const [aiReason, setAiReason] = useState<string>('');

  // Simple trend buffer of recent decisions (BUY=1, HOLD=0, SELL=-1)
  const [trend, setTrend] = useState<Array<{ time: string; value: number }>>([]);

  // Blink + sound on verdict change
  const lastDecisionRef = useRef<Decision | undefined>(undefined);
  const verdictWrapRef = useRef<HTMLDivElement | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);

  // Lazy init sound
  useEffect(() => {
    if (!audioRef.current) {
      const a = new Audio('/pling.mp3');
      audioRef.current = a;
    }
  }, []);

  // Polling (switchable /decide vs /analyze)
  useEffect(() => {
    let isMounted = true;
    let timer: number | undefined;

    const fetchTick = async () => {
      try {
        if (mode === 'decide') {
          const resp = await fetch(DECIDE_ENDPOINT, {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify(DEMO_BATCH),
          });

          const { aiUsed, aiReason } = readAiHeaders(resp.headers);
          if (!isMounted) return;
          setAiUsed(aiUsed);
          setAiReason(aiReason);

          let json: unknown = null;
          try {
            json = await resp.json();
          } catch {
            json = null;
          }

          const norm = normalizeDecidePayload(json);

          // Blink & sound on change
          const prev = lastDecisionRef.current;
          if (norm.decision && prev && norm.decision !== prev) {
            if (verdictWrapRef.current) {
              verdictWrapRef.current.classList.add('blink');
              window.setTimeout(() => {
                verdictWrapRef.current && verdictWrapRef.current.classList.remove('blink');
              }, 600);
            }
            if (audioRef.current) {
              void audioRef.current.play().catch(() => {});
            }
          }
          lastDecisionRef.current = norm.decision;

          setData((old) => ({
            ...old,
            decision: norm.decision ?? old.decision,
            confidence: norm.confidence ?? old.confidence,
            reasons: norm.reasons,
            evidence: norm.evidence,
          }));

          // Trend update on each tick when we have a decision
          const val = decisionToValue(norm.decision ?? lastDecisionRef.current);
          if (val !== null) {
            setTrend((arr) => {
              const next = [...arr, { time: new Date().toISOString(), value: val }];
              if (next.length > TREND_MAX) next.shift();
              return next;
            });
          }
        } else {
          // /analyze – expects full ApiResponse
          const resp = await fetch(ANALYZE_ENDPOINT, {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({}),
          });

          if (!isMounted) return;
          setAiUsed(false);
          setAiReason('');

          let json: ApiResponse | null = null;
          try {
            json = (await resp.json()) as ApiResponse;
          } catch {
            json = null;
          }

          const nextDecision = json?.decision;
          const prev = lastDecisionRef.current;

          if (nextDecision && prev && nextDecision !== prev) {
            if (verdictWrapRef.current) {
              verdictWrapRef.current.classList.add('blink');
              window.setTimeout(() => {
                verdictWrapRef.current && verdictWrapRef.current.classList.remove('blink');
              }, 600);
            }
            if (audioRef.current) {
              void audioRef.current.play().catch(() => {});
            }
          }
          lastDecisionRef.current = nextDecision ?? lastDecisionRef.current;

          if (json) {
            setData((old) => ({
              ...old,
              decision: json.decision ?? old.decision,
              confidence: json.confidence ?? old.confidence,
              reasons: json.reasons ?? old.reasons,
              evidence: json.evidence ?? old.evidence,
              contributors: json.contributors ?? old.contributors,
            }));
          }

          const val = decisionToValue(nextDecision ?? lastDecisionRef.current);
          if (val !== null) {
            setTrend((arr) => {
              const next = [...arr, { time: new Date().toISOString(), value: val }];
              if (next.length > TREND_MAX) next.shift();
              return next;
            });
          }
        }
      } catch {
        // Silent fail – keep previous UI state
      }
    };

    // Immediate tick then interval
    void fetchTick();
    timer = window.setInterval(fetchTick, 15000) as unknown as number;

    return () => {
      isMounted = false;
      if (timer) window.clearInterval(timer);
    };
  }, [mode]);

  // EvidencePanel requires EvidenceItem[]
  const evidenceItems: EvidenceItem[] = (data.evidence ?? []).map((e) => ({
    title: e.title ?? '',
    source: e.source ?? '',
    url: e.url ?? '',
    sentiment: e.sentiment ?? 'neu',
    time: e.time ?? '',
  }));

  const useDecide = mode === 'decide';

  const toggleMode = () => {
    const next = useDecide ? 'analyze' : 'decide';
    setMode(next);
    localStorage.setItem(LS_KEY, next === 'decide' ? '1' : '0');
  };

  return (
    <div id="app" class="p-4">
      {/* Header with AI badge + settings */}
      <header class="flex items-center justify-between mb-3">
        <div class="flex items-baseline gap-2">
          <h1 class="text-xl" style={{ color: 'var(--text)' }}>
            Dow Sentiment Analyzer
          </h1>
          {useDecide && aiUsed && <span class="ai-badge">AI</span>}
        </div>

        {/* Settings chip */}
        <button
          onClick={toggleMode}
          title="Use /decide instead of /analyze"
          style={{
            background: 'var(--panel)',
            color: 'var(--text)',
            border: `1px solid var(--border)`,
            borderRadius: '999px',
            padding: '6px 10px',
            cursor: 'pointer',
            fontSize: '12px',
          }}
        >
          Settings: {useDecide ? 'Use /decide ✓' : 'Use /decide ✗'}
        </button>
      </header>

      {useDecide && aiUsed && aiReason && (
        <div class="ai-hint" style={{ marginBottom: '12px' }}>
          {aiReason}
        </div>
      )}

      {/* Verdict wrap for blink effect */}
      <div ref={verdictWrapRef} class="verdict-wrap">
        <VerdictPanel verdict={data.decision ?? 'HOLD'} confidence={data.confidence} />
      </div>

      <WhyPanel reasons={data.reasons ?? []} />
      <EvidencePanel items={evidenceItems} />

      {/* Trend panel (simple sparkline of recent decisions) */}
      <div style={{ marginTop: '12px' }}>
        <TrendPanel series={trend} height={72} emphasizeLast={true} title="Sentiment trend" />
      </div>
    </div>
  );
}
