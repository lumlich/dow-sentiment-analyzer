// ui/src/app.tsx
import { VerdictPanel } from './components/VerdictPanel';
import { WhyPanel } from './components/WhyPanel';
import { EvidencePanel } from './components/EvidencePanel';
import type { EvidenceItem } from './components/EvidencePanel';
import './app.css';

export function App() {
  // Static demo data for Step 2
  const verdict = 'BUY' as const;
  const reasons = [
    'Futures rebound after dovish remarks in FOMC minutes',
    'Positive breadth in Dow components during pre-market',
    'Sentiment shift detected in key sources (low noise)'
  ];

  const evidence: EvidenceItem[] = [
    {
      title: 'Fed signals patience; markets react positively',
      source: 'Reuters',
      url: '#',
      sentiment: 'pos',
      time: '2025-08-26 08:10'
    },
    {
      title: 'Dow futures edge higher amid earnings beats',
      source: 'Bloomberg',
      url: '#',
      sentiment: 'pos',
      time: '2025-08-26 08:05'
    },
    {
      title: 'Mixed commentary on industrials; net neutral',
      source: 'WSJ',
      url: '#',
      sentiment: 'neu',
      time: '2025-08-26 07:58'
    }
  ];

  return (
    <main class="shell">
      <header class="topbar">
        <h1>Dow Sentiment Analyzer</h1>
        <span class="sub">Phase 2 · Static layout</span>
      </header>

      <div class="grid">
        <VerdictPanel verdict={verdict} confidence={0.74} />
        <WhyPanel reasons={reasons} />
        <EvidencePanel items={evidence} />
      </div>

      <footer class="foot">
        <span>© 2025</span>
      </footer>
    </main>
  );
}