/** @jsx h */
/* app.tsx — SMOKE TEST (fixed JSX) */
import { h } from "preact";
import type { FunctionalComponent } from "preact";
import "./app.css";

export const App: FunctionalComponent = () => {
  return (
    <main className="container">
      <header className="section" aria-label="Header">
        <h1>Hello UI</h1>
        <p className="small">If you can read this, mount & JSX work.</p>
      </header>
    </main>
  );
};

export default App;
