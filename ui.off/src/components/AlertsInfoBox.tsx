// src/components/AlertsInfoBox.tsx
import { FunctionalComponent } from "preact";
import { ExternalLink, Bell } from "lucide-react";

type LinkBtnProps = {
  label: string;
  href?: string;
  enabled: boolean;
  titleWhenEnabled: string;
  titleWhenDisabled: string;
};

const LinkButton: FunctionalComponent<LinkBtnProps> = ({
  label,
  href,
  enabled,
  titleWhenEnabled,
  titleWhenDisabled,
}) => {
  // Render <a> as a real link only when enabled; otherwise behave like a disabled button.
  const commonProps: any = {
    className: "btn-cta",
    "aria-disabled": enabled ? "false" : "true",
    title: enabled ? titleWhenEnabled : titleWhenDisabled,
    onClick: (e: MouseEvent) => {
      if (!enabled) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
  };

  return enabled ? (
    <a {...commonProps} href={href} target="_blank" rel="noreferrer">
      <span>{label}</span>
      <ExternalLink width={16} height={16} />
    </a>
  ) : (
    <a {...commonProps} role="button" tabIndex={-1}>
      <span>{label}</span>
      <ExternalLink width={16} height={16} />
    </a>
  );
};

const AlertsInfoBox: FunctionalComponent = () => {
  const discordInvite = import.meta.env.VITE_DISCORD_INVITE as string | undefined;
  const slackInvite = import.meta.env.VITE_SLACK_INVITE as string | undefined;

  const hasDiscord = !!discordInvite;
  const hasSlack = !!slackInvite;

  return (
    <section className="section" aria-labelledby="alerts-title">
      <div className="panel-header">
        <h2 id="alerts-title" className="h2">Get instant alerts when sentiment changes</h2>
      </div>

      <p className="small" style={{ marginBottom: 12 }}>
        Connect a channel to be notified immediately when the decision or confidence shifts.
      </p>

      <div style={{ display: "flex", flexWrap: "wrap", gap: 12, alignItems: "center" }}>
        <LinkButton
          label="Join Discord"
          href={discordInvite}
          enabled={hasDiscord}
          titleWhenEnabled="Open Discord (invite/channel)"
          titleWhenDisabled="Set VITE_DISCORD_INVITE to enable"
        />

        <LinkButton
          label="Join Slack"
          href={slackInvite}
          enabled={hasSlack}
          titleWhenEnabled="Open Slack (invite/channel)"
          titleWhenDisabled="Set VITE_SLACK_INVITE to enable"
        />

        <span className="inline-note" style={{ marginLeft: 6 }}>
          <Bell width={16} height={16} />
          <span>Install the mobile app and enable OS notifications.</span>
        </span>
      </div>
    </section>
  );
};

export default AlertsInfoBox;
export { AlertsInfoBox };
