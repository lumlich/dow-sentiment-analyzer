// ui.off/src/components/DebugStatus.tsx
import { useEffect, useState } from 'preact/hooks';

type DecideResp = {
  decision?: 'BUY' | 'SELL' | 'HOLD';
  confidence?: number;
};

export default function DebugStatus() {
  const apiBase = import.meta.env.VITE_API_BASE ?? '';
  const [health, setHealth] = useState<'checking' | 'ok' | 'fail'>('checking');
  const [healthDetail, setHealthDetail] = useState<string>('');
  const [last, setLast] = useState<DecideResp | null>(null);
  const [error, setError] = useState<string>('');

  useEffect(() => {
    const url = `${apiBase}/health`.replace(/\/{2,}/g, '/').replace(':/', '://');
    fetch(url)
      .then(r => (r.ok ? r.text() : Promise.reject(`${r.status} ${r.statusText}`)))
      .then(txt => {
        setHealth('ok');
        setHealthDetail(txt.trim());
      })
      .catch(e => {
        setHealth('fail');
        setHealthDetail(String(e));
      });
  }, [apiBase]);

  const testDecide = async () => {
    setError('');
    try {
      const url = `${apiBase}/decide`.replace(/\/{2,}/g, '/').replace(':/', '://');
      const payload = {
        inputs: [
          {
            source: 'debug',
            author: 'ui',
            text: 'FOMC holds rates; dovish tilt; futures up pre-market',
            weight: 1.0,
            time: new Date().toISOString(),
          },
        ],
      };
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      const json = await res.json();
      setLast({ decision: json?.decision, confidence: json?.confidence });
    } catch (e) {
      setError(String(e));
    }
  };

  const barColor =
    health === 'ok' ? '#e6ffed' : health === 'fail' ? '#ffecec' : '#fffbe6';
  const text =
    health === 'ok'
      ? `Backend OK (${healthDetail || 'OK'})`
      : health === 'fail'
      ? `Backend FAIL (${healthDetail})`
      : 'Checking backend…';

  return (
    <div
      style={{
        background: barColor,
        border: '1px solid #ddd',
        padding: '8px 12px',
        borderRadius: 8,
        marginBottom: 12,
        fontFamily: 'system-ui, sans-serif',
      }}
    >
      <div
        style={{
          display: 'flex',
          gap: 12,
          alignItems: 'center',
          flexWrap: 'wrap',
        }}
      >
        <strong>Debug:</strong>
        <span>{text}</span>
        <button
          onClick={testDecide}
          style={{
            padding: '6px 10px',
            borderRadius: 6,
            border: '1px solid #ccc',
            cursor: 'pointer',
          }}
        >
          Test /decide
        </button>
        {last && (
          <span>
            Last: {last.decision ?? '—'} (
            {Number.isFinite(last.confidence ?? NaN)
              ? (last.confidence as number).toFixed(3)
              : '—'}
            )
          </span>
        )}
        {error && <span style={{ color: '#a00' }}>Error: {error}</span>}
        <span style={{ opacity: 0.6, marginLeft: 'auto' }}>
          API_BASE: {apiBase || '(proxy)'}
        </span>
      </div>
    </div>
  );
}
