/* components/SettingsBar.tsx */
import type { FunctionalComponent } from "preact";

interface SettingsBarProps {
  useDecide: boolean;
  onToggleUseDecide: () => void;
}

const SettingsBar: FunctionalComponent<SettingsBarProps> = ({ useDecide, onToggleUseDecide }) => {
  return (
    <div className="toolbar" aria-label="Settings">
      <div className="small">Settings</div>
      <div className="toolbar-right">
        <button
          type="button"
          className="btn-chip"
          aria-pressed={useDecide}
          onClick={onToggleUseDecide}
          title="Toggle using /decide endpoint (UI only)"
        >
          Use /decide
        </button>
      </div>
    </div>
  );
};

export default SettingsBar;
