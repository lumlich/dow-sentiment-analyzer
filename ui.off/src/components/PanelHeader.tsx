/* components/PanelHeader.tsx
 * Simple, accessible panel header with optional actions and subtitle/meta.
 */

import type { FunctionalComponent, ComponentChildren } from "preact";

interface PanelHeaderProps {
  title: string;
  subtitle?: string;
  actions?: ComponentChildren;
  id?: string; // allow external aria-labelledby targeting
  meta?: ComponentChildren; // small pills/chips on the right
}

const PanelHeader: FunctionalComponent<PanelHeaderProps> = ({ title, subtitle, actions, id, meta }) => {
  const headingId = id ?? `hdr-${Math.random().toString(36).slice(2)}`;
  return (
    <div className="panel-header" aria-labelledby={headingId}>
      <div className="panel-header-left">
        <h2 id={headingId}>{title}</h2>
        {subtitle ? <div className="panel-subtitle small">{subtitle}</div> : null}
      </div>
      {meta ? <div className="panel-meta">{meta}</div> : null}
      {actions ? <div className="panel-actions">{actions}</div> : null}
    </div>
  );
};

export default PanelHeader;
