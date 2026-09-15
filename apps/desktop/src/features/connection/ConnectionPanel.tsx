import { useState } from "react";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";
import { useLocale } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";

export function ConnectionPanel({
  state,
  onState,
  onOpenSettings,
}: {
  state: DesktopState;
  onState: (state: DesktopState) => void;
  onOpenSettings: () => void;
}) {
  const { t } = useLocale();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const mutationBusy = busy || Boolean(state.current_operation);
  const runtimeReady = state.readiness.runtime_ready;
  const credentialsReady = state.openai_tunnel_configured;
  const tunnelStarted = Boolean(state.regular_tunnel);
  const secureLinkReady = state.regular_tunnel?.status === "ready"
    && Boolean(state.regular_tunnel.ready_for_chatgpt);
  const tunnelError = state.regular_tunnel?.status === "error";
  const chatgptAccessible = runtimeReady && secureLinkReady;
  const tunnelId = state.openai_tunnel_config.effective_tunnel_id ?? state.openai_tunnel_config.saved_tunnel_id;

  if (state.topology?.server.kind === "remote") {
    return (
      <section className="page-section rc5-page connection-page" aria-labelledby="connection-title" data-webcodex-page="connection">
        <div className="eyebrow">{t("connection.eyebrow")}</div>
        <h1 id="connection-title">{t("connection.title")}</h1>
        <p className="lede">{t("connection.simpleDescription")}</p>
        <article className="connection-status-card">
          <span className="section-kicker">{t("connection.current")}</span>
          <h2>{t("connection.remoteServer")}</h2>
          <dl className="connection-status-list">
            <ConnectionStatusRow label="Server" value={state.topology.server.url} ready />
            <ConnectionStatusRow label="Runner" value={t("connection.runnerThisComputer")} ready={state.readiness.runner === "ready"} />
            <ConnectionStatusRow label={t("connection.method")} value={t("connection.externalManagedRemote")} ready />
          </dl>
        </article>
      </section>
    );
  }

  if (state.topology?.experience === "quick_share") {
    const quickShareReady = Boolean(state.quick_share?.ready_for_chatgpt);
    return (
      <section className="page-section rc5-page connection-page" aria-labelledby="connection-title" data-webcodex-page="connection">
        <div className="eyebrow">{t("connection.eyebrow")}</div>
        <h1 id="connection-title">{t("connection.title")}</h1>
        <p className="lede">{t("connection.quickShareManaged")}</p>
        <article className={`connection-progress-card ${quickShareReady ? "ready" : "pending"}`}>
          <div className="connection-progress-heading">
            <div>
              <span className="section-kicker">Quick Share</span>
              <h2>{quickShareReady ? t("common.ready") : t("connection.inProgress")}</h2>
            </div>
            <span className={`connection-state-badge ${quickShareReady ? "ready" : "pending"}`}>
              <i className={`status-dot ${quickShareReady ? "ready" : "pending"}`} aria-hidden="true" />
              {quickShareReady ? t("common.ready") : t("connection.inProgress")}
            </span>
          </div>
          {state.quick_share?.mcp_url && <code className="quick-share-url">{state.quick_share.mcp_url}</code>}
        </article>
      </section>
    );
  }

  const run = async (operation: () => Promise<DesktopState>) => {
    if (mutationBusy) return;
    setBusy(true);
    setError(null);
    try {
      onState(await operation());
    } catch (value) {
      setError(normalizeDesktopError(value));
    } finally {
      setBusy(false);
    }
  };

  const restartSecureLink = async () => {
    if (mutationBusy || !runtimeReady || !credentialsReady) return;
    setBusy(true);
    setError(null);
    try {
      let next = state;
      if (next.regular_tunnel) {
        next = await desktopApi.stopRegularTunnel();
        onState(next);
      }
      next = await desktopApi.startRegularTunnel();
      onState(next);
    } catch (value) {
      setError(normalizeDesktopError(value));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section
      className="page-section rc5-page connection-page"
      aria-labelledby="connection-title"
      aria-busy={mutationBusy}
      data-webcodex-page="connection"
    >
      <div className="eyebrow">{t("connection.eyebrow")}</div>
      <h1 id="connection-title">{t("connection.title")}</h1>
      <p className="lede">{t("connection.description")}</p>
      {tunnelId && <span className="visually-hidden">{tunnelId}</span>}
      {chatgptAccessible && <span className="visually-hidden">{t("connection.chatgptAccessible")}</span>}

      <div className="connection-overview-grid">
        <article className={`connection-progress-card ${chatgptAccessible ? "ready" : "pending"}`}>
          <div className="connection-progress-heading">
            <div>
              <span className="section-kicker">CodexLink</span>
              <h2>{t("connection.connectTitle")}</h2>
            </div>
            <span className={`connection-state-badge ${chatgptAccessible ? "ready" : "pending"}`}>
              <i className={`status-dot ${chatgptAccessible ? "ready" : "pending"}`} aria-hidden="true" />
              {chatgptAccessible ? t("connection.chatgptAccessible") : t("connection.inProgress")}
            </span>
          </div>
          <ol className="connection-step-list">
            <ConnectionStep index={1} ready={runtimeReady} label={t("connection.stepRuntime")} description={t("home.runnerExplanation")} />
            <ConnectionStep index={2} ready={credentialsReady} label={t("connection.stepCredentials")} description={t("settings.tunnelHelp")} />
            <ConnectionStep index={3} ready={secureLinkReady} label={t("connection.stepSecureLink")} description={t("connection.secureLinkHelp")} />
            <ConnectionStep index={4} ready={chatgptAccessible} label={t("connection.stepChatGpt")} description={t("connection.createAppHelp")} />
          </ol>
        </article>

        <article className="connection-status-card">
          <div className="connection-status-hero">
            <span className="section-kicker">{t("connection.current")}</span>
            <div className={`connection-status-icon ${secureLinkReady ? "ready" : "pending"}`} aria-hidden="true"><span /></div>
            <div>
              <h2>{chatgptAccessible ? t("connection.normal") : t("connection.needsAction")}</h2>
              <p>{chatgptAccessible ? t("connection.chatgptAccessibleHelp") : t("connection.secureLinkHelp")}</p>
            </div>
          </div>
          <dl className="connection-status-list">
            <ConnectionStatusRow label={t("connection.runtimeStatus")} value={runtimeReady ? t("common.ready") : t("common.stopped")} ready={runtimeReady} />
            <ConnectionStatusRow label={t("connection.credentialsStatus")} value={credentialsReady ? t("common.configured") : t("connection.notConfigured")} ready={credentialsReady} />
            <ConnectionStatusRow label={t("connection.method")} value="OpenAI Tunnel" ready={secureLinkReady} />
            {state.tunnel_proxy.effective_url && <ConnectionStatusRow label={t("connection.networkRoute")} value={state.tunnel_proxy.effective_url} ready />}
          </dl>
        </article>

        <article className="connection-diagnostic-card">
          <span className="section-kicker">{t("settings.diagnostics")}</span>
          <h2>{runtimeReady ? t("connection.normal") : t("error.runtimeTitle")}</h2>
          <p>{runtimeReady ? t("connection.simpleDescription") : t("error.runtimeAction")}</p>
          {error && <LocalizedError error={error} />}
          <dl className="connection-status-list diagnostic-list">
            <ConnectionStatusRow label={t("connection.runtimeStatus")} value={runtimeReady ? t("common.ready") : t("common.error")} ready={runtimeReady} />
            <ConnectionStatusRow label={t("connection.credentialsStatus")} value={credentialsReady ? t("common.configured") : t("connection.notConfigured")} ready={credentialsReady} />
            {state.tunnel_proxy.effective_url && <ConnectionStatusRow label={t("connection.networkRoute")} value={state.tunnel_proxy.effective_url} ready />}
          </dl>
          {!credentialsReady && <button className="primary-button" onClick={onOpenSettings}>{t("connection.configureCredentials")}</button>}
          {credentialsReady && runtimeReady && (!secureLinkReady || tunnelError) && (
            <button className="primary-button" onClick={() => void restartSecureLink()} disabled={mutationBusy} data-webcodex-action="restart-regular-tunnel">
              {mutationBusy ? t("connection.tunnelStarting") : t("connection.retrySecureLink")}
            </button>
          )}
        </article>

        <article className={`secure-link-card ${secureLinkReady ? "ready" : tunnelError ? "error" : "pending"}`}>
          <div>
            <span className="section-kicker">OpenAI Secure Tunnel</span>
            <strong>{secureLinkReady ? t("connection.normal") : tunnelError ? t("common.error") : tunnelStarted ? t("connection.needsAction") : t("connection.secureLinkNotStarted")}</strong>
            <p>{secureLinkReady ? t("connection.chatgptAccessibleHelp") : credentialsReady ? t("connection.secureLinkHelp") : t("connection.credentialsFirst")}</p>
          </div>
          {tunnelStarted && (
            <button className="danger-button" onClick={() => void run(desktopApi.stopRegularTunnel)} disabled={mutationBusy} data-webcodex-action="stop-regular-tunnel">
              {t("connection.stopTunnel")}
            </button>
          )}
        </article>
      </div>
    </section>
  );
}

function ConnectionStep({ index, ready, label, description }: { index: number; ready: boolean; label: string; description: string }) {
  return (
    <li className={ready ? "complete" : "pending"} aria-label={label}>
      <span aria-hidden="true">{ready ? "✓" : index}</span>
      <div><strong>{label}</strong><small>{description}</small></div>
    </li>
  );
}

function ConnectionStatusRow({
  label,
  value,
  ready,
  mono = false,
}: {
  label: string;
  value: string;
  ready: boolean;
  mono?: boolean;
}) {
  return (
    <div>
      <dt>{label}</dt>
      <dd className={mono ? "mono" : ""}>
        <i className={`status-dot ${ready ? "ready" : "pending"}`} aria-hidden="true" />
        {value}
      </dd>
    </div>
  );
}

function LocalizedError({ error }: { error: DesktopError }) {
  const { t } = useLocale();
  const presentation = desktopErrorPresentation(error, t);
  return (
    <div className="error-card" role="alert">
      <strong>{presentation.title}</strong>
      <span>{presentation.action}</span>
      <details>
        <summary>{t("common.details")}</summary>
        <code>{error.code}</code>
        <p>{error.message}</p>
      </details>
    </div>
  );
}

