import { useEffect, useState } from 'preact/hooks';
import './app.css';

export function App() {
  const [status, setStatus] = useState<string>('checking...');

  useEffect(() => {
    fetch('/api/health')
      .then((r) => r.text())
      .then((text) => setStatus(text))
      .catch(() => setStatus('error'));
  }, []);

  return (
    <main style={{ padding: '24px', fontFamily: 'system-ui, sans-serif' }}>
      <h1>Dow Sentiment Analyzer — Hello world</h1>
      <p>
        Frontend is served by Axum/Shuttle. Backend API lives under <code>/api</code>.
      </p>
      <p>
        <strong>API status:</strong> {status}
      </p>
    </main>
  );
}