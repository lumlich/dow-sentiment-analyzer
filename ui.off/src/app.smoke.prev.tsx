/* app.tsx — Step 1: layout container + sections
 * Logic unchanged; only layout/styling wrappers.
 */

import { FunctionalComponent, h } from "preact";
import { useEffect } from "preact/hooks";

import DebugStatus from "./components/DebugStatus";
import AlertsInfoBox from "./components/AlertsInfoBox";

import "./app.css";

export const App: FunctionalComponent = () => {
  useEffect(() => {
    // Reserved for future UI-only effects
  }, []);

  return (
    <main className="container">
      {/* Header */}
      <header className="section" aria-label="Header">
        <h1>DOW Sentiment Analyzer</h1>
        <p className="small">Live decision engine UI — visual polish (Step 1)</p>
      </header>

      {/* Alerts / Info */}
      <section className="section" aria-label="Alerts and Info">
        <AlertsInfoBox />
      </section>

      {/* Placeholder for main panels */}
      <section className="section stack-4" aria-label="Main panels">
        <div className="card">
          <h2>Overview</h2>
          <p className="small">
            This section will host your main panels (Verdict, Reasons/Evidence, Trend) with unified card styling.
          </p>
        </div>
      </section>

      {/* Debug status (will be gated in prod later) */}
      <section className="section" aria-label="Debug">
        <DebugStatus />
      </section>
    </main>
  );
};

export default App;
