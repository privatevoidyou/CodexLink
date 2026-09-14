import type { DesktopState } from "../../models/topology";
import { useLocale } from "../../i18n/locale";
import {
  projectReadinessLabel,
  readinessNextAction,
  runnerReadinessLabel,
  serverReadinessLabel,
} from "../../i18n/presentation";

interface DashboardProps {
  state: DesktopState;
  refreshing: boolean;
  onRefresh: () => void;
  onResumeRuntime: () => void;
  onChooseProject: () => void;
  onChangeSetup: () => void;
  onNavigate: (page: "projects" | "connection" | "settings") => void;
  onStopRuntime: () => void;
}

export function Dashboard({
  state,
  refreshing,
  onRefresh,
  onResumeRuntime,
  onChooseProject,
  onChangeSetup,
  onNavigate,
  onStopRuntime,
}: DashboardProps) {
  const { t } = useLocale();
  const operationBusy = Boolean(state.current_operation);
  const runtimeReady = state.readiness.runtime_ready;
  const tunnelConfigured = state.openai_tunnel_configured;
  const secureLinkReady = state.regular_tunnel?.status === "ready"
    && Boolean(state.regular_tunnel.ready_for_chatgpt);
  const chatgptObserved = runtimeReady
    && state.regular_tunnel?.status !== "error"
    && Boolean(state.chatgpt_activity?.observed);
  const allSet = Boolean(state.project) && runtimeReady && tunnelConfigured && secureLinkReady;
  const canResumeRuntime = Boolean(state.topology) && !runtimeReady;
  const projectName = state.project?.path.split(/[\\/]/).filter(Boolean).pop() ?? t("home.noProject");

  return (
    <section
      className="page-section dashboard-page rc5-page"
      aria-labelledby="home-title"
      aria-busy={refreshing}
      data-webcodex-page="home"
    >
      <div className="page-heading-row rc5-heading-row">
        <div>
          <div className="eyebrow">{t("home.eyebrow")}</div>
          <h1 id="home-title">{allSet ? t("home.readyToUse") : t("home.finishSetup")}</h1>
          <p className="lede">
            {allSet ? t("workspace.description") : nextHomeMessage(state, t)}
          </p>
        </div>
        <button
          className="secondary-button compact-button"
          onClick={onRefresh}
          disabled={refreshing || operationBusy}
          data-webcodex-action="refresh-runtime"
        >
          {refreshing ? t("home.checking") : t("home.refresh")}
        </button>
      </div>

      <article className="workspace-project rc5-project-card">
        <span className={`project-availability ${allSet ? "ready" : "pending"}`}>
          <i className={`status-dot ${allSet ? "ready" : "pending"}`} aria-hidden="true" />
          {allSet ? t("home.available") : t("home.configuring")}
        </span>
        <div className="project-link-graphic" aria-hidden="true"><span /><span /></div>
        <div className="project-emblem" aria-hidden="true"><span className="nav-icon nav-projects" /></div>
        <div className="workspace-project-copy">
          <span className="section-kicker">{t("workspace.currentProject")}</span>
          <h2>{projectName}</h2>
          <p>{state.project?.path ?? t("setup.projectRequired")}</p>
          <div className="project-status-row">
            <StatusPill ready={secureLinkReady} label={t("home.aiConnection")} value={chatgptObserved ? t("common.connected") : secureLinkReady ? t("common.ready") : t("home.notConnected")} />
            <StatusPill ready={runtimeReady} label={t("home.runtimeStatus")} value={runtimeReady ? t("common.ready") : t("common.stopped")} />
            <StatusPill ready={Boolean(state.project)} label={t("home.projects")} value={state.project ? "1" : "0"} />
          </div>
        </div>
        <div className="project-card-actions">
          <button className="secondary-button" onClick={() => onNavigate("projects")} disabled={operationBusy}>{t("home.viewProject")}</button>
          <button className="secondary-button" onClick={() => onNavigate("connection")} disabled={operationBusy}>{t("home.connectionDetails")}</button>
        </div>
      </article>

      <section className="connection-chain-card" aria-labelledby="connection-chain-title">
        <div className="section-heading-row">
          <div>
            <span className="section-kicker">{t("home.overall")}</span>
            <h2 id="connection-chain-title">{t("home.connectionPath")}</h2>
          </div>
          {allSet && <span className="connection-chain-summary">{t("home.chainSummaryReady")}</span>}
        </div>
        <div className="connection-chain" role="list">
          <ChainNode label={t("home.localMachine")} ready={Boolean(state.project)} status={state.project ? t("common.ready") : t("home.configuring")} />
          <span className={`chain-line ${runtimeReady ? "ready" : ""}`} aria-hidden="true" />
          <ChainNode label="Engine" ready={runtimeReady} status={runtimeReady ? t("common.ready") : t("common.stopped")} />
          <span className={`chain-line ${secureLinkReady ? "ready" : ""}`} aria-hidden="true" />
          <ChainNode label="Secure Link" ready={secureLinkReady} status={secureLinkReady ? t("common.ready") : t("home.configuring")} />
          <span className={`chain-line ${chatgptObserved ? "ready" : ""}`} aria-hidden="true" />
          <ChainNode label="ChatGPT" ready={chatgptObserved} status={chatgptObserved ? t("common.ready") : t("home.configuring")} />
        </div>
      </section>

      <div className="dashboard-lower-grid">
        <section className="home-action-card" aria-labelledby="home-next-title">
          <span className="section-kicker">{t("home.nextActions")}</span>
          <h2 id="home-next-title">{allSet ? t("home.readyNextTitle") : t("home.finishSetup")}</h2>
          <div className="home-action-list">
            {!state.project && <button onClick={onChooseProject}>{t("project.add")}<span aria-hidden="true">→</span></button>}
            {canResumeRuntime && <button onClick={onResumeRuntime}>{t("home.resumeRuntime")}<span aria-hidden="true">→</span></button>}
            {runtimeReady && !tunnelConfigured && <button onClick={() => onNavigate("settings")}>{t("home.configureTunnel")}<span aria-hidden="true">→</span></button>}
            {runtimeReady && tunnelConfigured && <button onClick={() => onNavigate("connection")}>{t("home.manageConnection")}<span aria-hidden="true">→</span></button>}
            {state.project && <button onClick={() => onNavigate("projects")}>{t("home.viewProject")}<span aria-hidden="true">→</span></button>}
          </div>
        </section>

        <section className="system-status-card" aria-labelledby="home-system-title">
          <span className="section-kicker">{t("home.systemStatus")}</span>
          <h2 id="home-system-title">{t("home.overall")}</h2>
          <StatusLine label={t("home.service")} value={serverReadinessLabel(state.readiness.server, t)} ready={state.readiness.server === "ready"} />
          <StatusLine label={t("home.runner")} value={runnerReadinessLabel(state.readiness.runner, t)} ready={state.readiness.runner === "ready"} />
          <StatusLine label={t("home.projects")} value={projectReadinessLabel(state.readiness.project, t)} ready={state.readiness.project === "ready"} />
          <StatusLine label={t("home.connection")} value={chatgptObserved ? t("common.connected") : secureLinkReady ? t("common.ready") : t("home.notConnected")} ready={secureLinkReady} />
        </section>
      </div>

      <details className="product-advanced">
        <summary>{t("home.otherSetup")}</summary>
        <p>{t("home.runtimeOwnership")}</p>
        <div className="runtime-action-buttons">
          <button className="secondary-button" onClick={onChangeSetup} disabled={operationBusy} data-webcodex-action="change-runtime-setup">{t("home.changeSetup")}</button>
          {runtimeReady && (
            <button className="danger-button" onClick={onStopRuntime} disabled={operationBusy} data-webcodex-action="stop-runtime">{t("home.stopRuntime")}</button>
          )}
        </div>
      </details>
    </section>
  );
}

function StatusPill({ ready, label, value }: { ready: boolean; label: string; value: string }) {
  return (
    <span className={`project-status-pill ${ready ? "ready" : "pending"}`}>
      <i className={`status-dot ${ready ? "ready" : "pending"}`} aria-hidden="true" />
      <span>{label}</span>
      <strong>{value}</strong>
    </span>
  );
}

function ChainNode({ label, ready, status }: { label: string; ready: boolean; status: string }) {
  return (
    <div className={`chain-node ${ready ? "ready" : "pending"}`} role="listitem" aria-label={label}>
      <span className="chain-node-mark" aria-hidden="true"><i /></span>
      <strong>{label}</strong>
      <small>{status}</small>
    </div>
  );
}

function StatusLine({ label, value, ready }: { label: string; value: string; ready: boolean }) {
  return (
    <div className="system-status-line">
      <span>{label}</span>
      <strong><i className={`status-dot ${ready ? "ready" : "pending"}`} aria-hidden="true" />{value}</strong>
    </div>
  );
}

type Translate = ReturnType<typeof useLocale>["t"];

function nextHomeMessage(state: DesktopState, t: Translate) {
  if (state.topology?.experience === "quick_share") {
    return readinessNextAction(state.readiness.next_action_kind, state.readiness.next_action, t)
      || state.readiness.next_action
      || t("home.needRuntime");
  }
  if (!state.project) return t("home.needProject");
  if (!state.readiness.runtime_ready) return t("home.needRuntime");
  if (!state.openai_tunnel_configured) return t("home.needTunnelCredentials");
  if (state.regular_tunnel?.status !== "ready") return t("home.needSecureLink");
  return t("home.needChatGpt");
}
