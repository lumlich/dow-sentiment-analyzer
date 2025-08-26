// ui/src/app.tsx
import { useEffect, useRef, useState } from 'preact/hooks';
import { VerdictPanel } from './components/VerdictPanel';
import { WhyPanel } from './components/WhyPanel';
import { EvidencePanel } from './components/EvidencePanel';
import type { EvidenceItem } from './components/EvidencePanel';
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

const API_ENDPOINT = '/analyze';
const POLL_MS = 15_000;
const LAST_VERDICT_KEY = 'dsa:lastVerdict';
const PLING_SRC = '/pling.mp3'; // place file in /ui/public/pling.mp3

export function App() {
  const [verdict, setVerdict] = useState<Decision>('HOLD');
  const [confidence, setConfidence] = useState<number>(0);
  const [reasons, setReasons] = useState<string[]>([]);
  const [evidence, setEvidence] = useState<EvidenceItem[]>([]);
  const [contributors, setContributors] = useState<string[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<string | null>(null);

  // Step 4 memory of last verdict
  const lastVerdictRef = useRef<Decision | null>(null);

  // Step 5: audio + blink state
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [blink, setBlink] = useState<boolean>(false);

  // Initialize persisted last verdict
  useEffect(() => {
    const persisted = localStorage.getItem(LAST_VERDICT_KEY) as Decision | null;
    if (persisted === 'BUY' || persisted === 'SELL' || persisted === 'HOLD') {
      lastVerdictRef.current = persisted;
    }
  }, []);

  // Prepare audio element and gently "unlock" it on first user interaction
  useEffect(() => {
    const a = new Audio(PLING_SRC);
    a.preload = 'auto';
    a.volume = 0.6; // adjust if needed
    audioRef.current = a;

    // Try to unlock autoplay policies with a first tap/click
    const onFirstPointer = () => {
      a.play()
        .then(() => {
          a.pause();
          a.currentTime = 0;
        })
        .catch(() => {
          /* ignore - user will interact later */
        });
      window.removeEventListener('pointerdown', onFirstPointer);
    };
    window.addEventListener('pointerdown', onFirstPointer, { once: true });

    return () => {
      window.removeEventListener('pointerdown', onFirstPointer);
      audioRef.current = null;
    };
  }, []);

  // Helper: trigger alert (sound + brief CSS blink)
  function triggerChangeAlert() {
    // Play sound
    const a = audioRef.current;
    if (a) {
      // Clone to allow overlapping plays if changes come quickly
      const clone = a.cloneNode(true) as HTMLAudioElement;
      clone.volume = a.volume;
      clone.play().catch(() => {
        /* ignore play errors due to autoplay policies */
      });
    }

    // Blink animation class
    setBlink(true);
    // Match to your CSS animation duration (e.g., 600ms)
    window.setTimeout(() => setBlink(false), 600);
  }

  async function fetchAnalysis(signal?: AbortSignal) {
    setError(null);
    try {
      const res = await fetch(API_ENDPOINT, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: 'UI polling check (Phase 2 / Step 4)' }),
        signal,
      });
      if (!res.ok) throw new Error(`API error: ${res.status} ${res.statusText}`);

      const data: ApiResponse = await res.json();

      // Decision & confidence
      const nextDecision: Decision = (data.decision ?? 'HOLD') as Decision;
      setVerdict(nextDecision);
      setConfidence(typeof data.confidence === 'number' ? data.confidence : 0);

      // Reasons
      setReasons(
        Array.isArray(data.reasons) && data.reasons.length
          ? data.reasons
          : ['No reasons returned by API'],
      );

      // Evidence mapping
      const mapped: EvidenceItem[] = Array.isArray(data.evidence)
        ? data.evidence.map((e, i) => ({
            title: e.title ?? `Evidence #${i + 1}`,
            source: e.source ?? 'Unknown',
            url: e.url ?? '#',
            sentiment: e.sentiment ?? 'neu',
            time: e.time ?? new Date().toISOString().slice(0, 16).replace('T', ' '),
          }))
        : [];
      setEvidence(mapped);

      // Contributors (optional)
      setContributors(Array.isArray(data.contributors) ? data.contributors : []);

      // Step 5: change detection -> alert
      const prev = lastVerdictRef.current;
      if (prev !== nextDecision) {
        lastVerdictRef.current = nextDecision;
        localStorage.setItem(LAST_VERDICT_KEY, nextDecision);
        if (prev !== null) {
          // Only alert on actual changes after we have a baseline
          triggerChangeAlert();
        }
      }

      setLastUpdated(new Date().toLocaleString());
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    }
  }

  // Initial fetch + polling every 15s
  useEffect(() => {
    const ac = new AbortController();
    setLoading(true);
    fetchAnalysis(ac.signal).finally(() => setLoading(false));

    const id = setInterval(() => {
      fetchAnalysis(ac.signal);
    }, POLL_MS);

    // Refresh immediately when tab becomes visible
    const onVis = () => {
      if (document.visibilityState === 'visible') {
        fetchAnalysis(ac.signal);
      }
    };
    document.addEventListener('visibilitychange', onVis);

    return () => {
      clearInterval(id);
      document.removeEventListener('visibilitychange', onVis);
      ac.abort();
    };
  }, []);

  return (
    <main class="shell">
      <header class="topbar">
        <h1>Dow Sentiment Analyzer</h1>
        <span class="sub">
          Phase 2 · Auto-refresh 15s {loading ? '· Loading…' : error ? '· Error' : '· Live'}
          {lastUpdated ? ` · Updated: ${lastUpdated}` : ''}
        </span>
      </header>

      {error ? (
        <div class="error-banner" role="alert">
          {error}
        </div>
      ) : null}

      <div class="grid">
        {/* Wrap verdict panel so we can toggle a blink class */}
        <div class={`verdict-wrap ${blink ? 'blink' : ''}`}>
          <VerdictPanel verdict={verdict} confidence={confidence} />
        </div>
        <WhyPanel reasons={reasons} />
        <EvidencePanel items={evidence} />
      </div>

      <footer class="foot">
        <span>© 2025</span>
        {contributors.length ? <span class="meta"> · Contributors: {contributors.join(', ')}</span> : null}
      </footer>
    </main>
  );
}
