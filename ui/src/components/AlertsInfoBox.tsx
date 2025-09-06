// ui/src/components/AlertsInfoBox.tsx
// Dark-friendly info box with CTA buttons for Discord/Slack.
// Reads invite URLs from Vite envs: VITE_DISCORD_INVITE, VITE_SLACK_INVITE.

console.log(
  "VITE env check",
  import.meta.env.VITE_DISCORD_INVITE,
  import.meta.env.VITE_SLACK_INVITE
);

const DISCORD_INVITE = import.meta.env.VITE_DISCORD_INVITE as string | undefined;
const SLACK_INVITE = import.meta.env.VITE_SLACK_INVITE as string | undefined;

// Small helper: detect accidental webhook URLs (these must NOT be in frontend)
function looksLikeWebhook(url?: string) {
  if (!url) return false;
  const u = url.toLowerCase();
  return u.includes("discord.com/api/webhooks") || u.includes("hooks.slack.com/services");
}

export default function AlertsInfoBox() {
  const hasDiscord = Boolean(DISCORD_INVITE && DISCORD_INVITE.trim().length > 0 && !looksLikeWebhook(DISCORD_INVITE));
  const hasSlack = Boolean(SLACK_INVITE && SLACK_INVITE.trim().length > 0 && !looksLikeWebhook(SLACK_INVITE));

  const showMisconfig =
    looksLikeWebhook(DISCORD_INVITE) || looksLikeWebhook(SLACK_INVITE);

  return (
    <section className="mx-auto max-w-5xl px-1 sm:px-2">
      <div
        className="rounded-2xl border p-6 shadow-sm"
        style={{
          background: "var(--panel)",
          color: "var(--text)",
          borderColor: "var(--border)",
        }}
      >
        <h2 className="text-2xl font-semibold">Get instant alerts when sentiment changes</h2>

        <div className="mt-3 space-y-3 text-sm leading-6 opacity-95 max-w-3xl">
          <p>
            <strong>Discord</strong> — join via invite, open <code>#alerts</code> and set channel
            notifications to <em>All messages</em> (recommended).
          </p>
          <p>
            <strong>Slack</strong> — join the workspace, follow <code>#alerts</code> and set channel
            notifications to <em>All new messages</em> or add keyword alerts (e.g., <code>BUY</code>,{" "}
            <code>SELL</code>, <code>DOW</code>).
          </p>
          <p>📲 Install the mobile app and allow OS notifications.</p>
          <p>🔔 You&apos;re set — alerts are instant.</p>
        </div>

        <div className="mt-4 flex flex-wrap gap-2">
          <a
            href={hasDiscord ? DISCORD_INVITE : undefined}
            target="_blank"
            rel="noreferrer"
            aria-disabled={!hasDiscord}
            className={`inline-flex items-center gap-2 rounded-xl border px-4 py-2 text-sm font-medium transition
              ${hasDiscord ? "cursor-pointer hover:bg-white/5" : "cursor-not-allowed opacity-50"}
            `}
            style={{ borderColor: "var(--border)", color: "var(--text)" }}
          >
            <span>Join Discord</span>
            {/* Fixed-size icon to avoid global svg rules */}
            <svg
              aria-hidden
              viewBox="0 0 24 24"
              width="16"
              height="16"
              preserveAspectRatio="xMidYMid meet"
              style={{ width: 16, height: 16, flex: "0 0 auto" }}
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path d="M7 17L17 7M7 7h10v10" />
            </svg>
          </a>

          <a
            href={hasSlack ? SLACK_INVITE : undefined}
            target="_blank"
            rel="noreferrer"
            aria-disabled={!hasSlack}
            className={`inline-flex items-center gap-2 rounded-xl border px-4 py-2 text-sm font-medium transition
              ${hasSlack ? "cursor-pointer hover:bg-white/5" : "cursor-not-allowed opacity-50"}
            `}
            style={{ borderColor: "var(--border)", color: "var(--text)" }}
          >
            <span>Join Slack</span>
            <svg
              aria-hidden
              viewBox="0 0 24 24"
              width="16"
              height="16"
              preserveAspectRatio="xMidYMid meet"
              style={{ width: 16, height: 16, flex: "0 0 auto" }}
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path d="M7 17L17 7M7 7h10v10" />
            </svg>
          </a>
        </div>

        {/* Config hints */}
        {(!hasDiscord && !hasSlack) && !showMisconfig && (
          <p className="mt-2 text-xs opacity-70">
            Invite links are not configured yet. Set <code>VITE_DISCORD_INVITE</code> and/or{" "}
            <code>VITE_SLACK_INVITE</code> in <code>ui/.env.local</code> and restart the dev server.
          </p>
        )}

        {showMisconfig && (
          <p className="mt-2 text-xs text-amber-300">
            It looks like you set <em>webhook</em> URLs. For the frontend, use <strong>invite</strong> links:
            Discord <code>https://discord.gg/…</code>, Slack <code>https://join.slack.com/…</code>. Keep webhook URLs only in the backend.
          </p>
        )}
      </div>
    </section>
  );
}
