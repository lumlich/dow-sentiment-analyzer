// ui.off/src/setupFetch.ts
// Patch: vždy posílej {} na /api/decide a schovej backendové chyby za fallback.

const origFetch = window.fetch.bind(window);

window.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
  const url = typeof input === 'string' ? input : input.toString();
  const isDecide = /\/api\/decide$/.test(url);

  if (!isDecide) {
    return origFetch(input, init);
  }

  // Vždy POST + JSON + aspoň prázdné tělo {}
  const method = ((init?.method ?? 'POST') + '').toUpperCase();
  const headers = new Headers(init?.headers ?? {});
  if (!headers.has('content-type')) headers.set('content-type', 'application/json');

  try {
    const resp = await origFetch(input, {
      ...init,
      method,
      headers,
      body: init?.body ?? '{}',
    });

    // Pokud server vrátí chybu, pošli uživateli jemný fallback místo chybové hlášky
    if (!resp.ok) {
      const fallback = {
        decision: 'HOLD',
        confidence: 0.5,
        reasons: [],
        evidence: [],
        note: 'temporarily_unavailable',
      };
      return new Response(JSON.stringify(fallback), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }

    return resp;
  } catch {
    // Síťové chyby -> stejný fallback
    const fallback = {
      decision: 'HOLD',
      confidence: 0.5,
      reasons: [],
      evidence: [],
      note: 'network_issue',
    };
    return new Response(JSON.stringify(fallback), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  }
};
