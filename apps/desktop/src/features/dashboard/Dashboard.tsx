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
  const isRemote = state.topology?.experience === "full" && state.topology.server.kind === "remote";
  const tunnelConfigured = state.openai_tunnel_configured;
  const localSecureLinkReady = state.regular_tunnel?.status === "ready"
    && Boolean(state.regular_tunnel.ready_for_chatgpt);
  const remoteServerReady = isRemote
    && state.readiness.server === "ready"
    && state.readiness.exposure === "remote_ready";
  const linkReady = isRemote ? remoteServerReady : localSecureLinkReady;
  const authorizedProjects = state.projects ?? [];
  const chatgptAccessible = isRemote
    ? state.readiness.ready_for_chatgpt
    : runtimeReady && authorizedProjects.length > 0 && localSecureLinkReady;
  const connectionHealthy = isRemote
    ? ["ready", "degraded"].includes(state.readiness.server) && state.readiness.runner === "ready"
    : runtimeReady && tunnelConfigured && localSecureLinkReady;
  const allSet = authorizedProjects.length > 0 && chatgptAccessible;
  const canResumeRuntime = Boolean(state.topology) && !runtimeReady;
  const needsConnectionRepair = Boolean(
    state.runtime_autostart
      && state.topology?.experience === "full"
      && state.topology.server.kind === "local"
      && (
        !runtimeReady
        || (
          state.preferred_connection === "open_ai_tunnel"
          && tunnelConfigured
          && state.regular_tunnel?.status !== "ready"
        )
      ),
  );
  const authorizedPathTitle = authorizedProjects[0]?.path ?? t("home.waitingAuthorizedPath");

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
        <div className="home-heading-actions">
          <span className={`home-overall-status ${allSet ? "ready" : "pending"}`}>
            <i className={`status-dot ${allSet ? "ready" : "pending"}`} aria-hidden="true" />
            {allSet ? t("home.readyToUse") : t("home.configuring")}
          </span>
          <button
            className="secondary-button compact-button"
            onClick={onRefresh}
            disabled={refreshing || operationBusy}
            data-webcodex-action="refresh-runtime"
          >
            {refreshing ? t("home.checking") : t("home.refresh")}
          </button>
        </div>
      </div>

      <article className="workspace-project rc5-project-card">
        <span className={`project-availability ${connectionHealthy ? "ready" : "pending"}`}>
          <i className={`status-dot ${connectionHealthy ? "ready" : "pending"}`} aria-hidden="true" />
          {connectionHealthy ? t("common.ready") : t("home.configuring")}
        </span>
        <div className="project-link-graphic" aria-hidden="true"><span /><span /></div>
        <div className="project-emblem" aria-hidden="true"><span className="nav-icon nav-projects" /></div>
        <div className="workspace-project-copy">
          <span className="section-kicker">CodexLink</span>
          <h2 className="authorized-path-title" title={authorizedPathTitle}>{authorizedPathTitle}</h2>
          <p>{t("home.authorizedPathsSummary", { count: authorizedProjects.length })}</p>
          <div className="project-status-row">
            <StatusPill ready={chatgptAccessible} label={t("home.aiConnection")} value={chatgptAccessible ? t("connection.chatgptAccessible") : t("home.notConnected")} />
            <StatusPill ready={runtimeReady} label={t("home.runtimeStatus")} value={runtimeReady ? t("common.ready") : t("common.stopped")} />
            <StatusPill ready={runtimeReady} label={t("home.projects")} value={String(authorizedProjects.length)} />
          </div>
        </div>
        <div className="project-card-actions">
          <button
            className={needsConnectionRepair ? "primary-button" : "secondary-button"}
            onClick={needsConnectionRepair
              ? onResumeRuntime
              : authorizedProjects.length === 0
                ? onChooseProject
                : () => onNavigate("projects")}
            disabled={operationBusy}
            data-webcodex-action={needsConnectionRepair ? "repair-managed-connection" : undefined}
          >
            {needsConnectionRepair
              ? t("home.repairConnection")
              : authorizedProjects.length === 0
                ? t("project.add")
                : t("home.viewProject")}
          </button>
          <button className="secondary-button" onClick={() => onNavigate("connection")} disabled={operationBusy}>{t("home.connectionDetails")}</button>
        </div>
      </article>

      <section className="connection-chain-card" aria-labelledby="connection-chain-title">
        <div className="section-heading-row">
          <div>
            <span className="section-kicker">{t("home.overall")}</span>
            <h2 id="connection-chain-title">{t("home.connectionPath")}</h2>
          </div>
          {connectionHealthy && <span className="connection-chain-summary">{t("home.chainSummaryReady")}</span>}
        </div>
        <div className="connection-chain" role="list">
          <ChainNode label={t("home.localMachine")} ready={runtimeReady} status={runtimeReady ? t("common.ready") : t("common.stopped")} />
          <span className={`chain-line ${runtimeReady ? "ready" : ""}`} aria-hidden="true" />
          <ChainNode label="Engine" ready={runtimeReady} status={runtimeReady ? t("common.ready") : t("common.stopped")} />
          <span className={`chain-line ${linkReady ? "ready" : ""}`} aria-hidden="true" />
          <ChainNode label={isRemote ? t("connection.remoteServer") : "Secure Link"} ready={linkReady} status={linkReady ? t("common.ready") : t("home.configuring")} />
          <span className={`chain-line ${chatgptAccessible ? "ready" : ""}`} aria-hidden="true" />
          <ChainNode label="ChatGPT" ready={chatgptAccessible} status={chatgptAccessible ? t("connection.chatgptAccessible") : t("home.configuring")} />
        </div>
      </section>

      <div className="dashboard-lower-grid">
        <section className="home-action-card" aria-labelledby="home-next-title">
          <h2 id="home-next-title">{t("home.nextActions")}</h2>
          <div className="home-action-list">
            {authorizedProjects.length === 0 && <HomeActionButton icon="project" label={t("project.add")} description={t("home.addProjectHint")} onClick={onChooseProject} />}
            {needsConnectionRepair
              ? <HomeActionButton icon="runtime" label={t("home.repairConnection")} description={t("home.repairConnectionHint")} onClick={onResumeRuntime} />
              : canResumeRuntime && <HomeActionButton icon="runtime" label={t("home.resumeRuntime")} description={t("home.resumeRuntimeHint")} onClick={onResumeRuntime} />}
            {authorizedProjects.length > 0 && <HomeActionButton icon="project" label={t("home.viewProject")} description={t("home.viewProjectHint")} onClick={() => onNavigate("projects")} />}
            <HomeActionButton icon="connection" label={t("home.manageConnection")} description={t("home.manageConnectionHint")} onClick={() => onNavigate("connection")} />
          </div>
        </section>

        <section className="system-status-card" aria-labelledby="home-system-title">
          <h2 id="home-system-title">{t("home.systemStatus")}</h2>
          <StatusLine label={t("home.service")} value={serverReadinessLabel(state.readiness.server, t)} ready={state.readiness.server === "ready"} />
          <StatusLine label={t("home.runner")} value={runnerReadinessLabel(state.readiness.runner, t)} ready={state.readiness.runner === "ready"} />
          <StatusLine label={t("home.projects")} value={projectReadinessLabel(state.readiness.project, t)} ready={state.readiness.project === "ready"} />
          <StatusLine label={t("home.connection")} value={chatgptAccessible ? t("common.connected") : t("home.notConnected")} ready={chatgptAccessible} />
        </section>
      </div>

      <div className="runtime-ownership-row">
        <p>{t("home.runtimeOwnership")}</p>
        <div className="runtime-action-buttons">
          <button className="secondary-button" onClick={onChangeSetup} disabled={operationBusy} data-webcodex-action="change-runtime-setup">{t("home.otherSetup")}</button>
          <button className="secondary-button" onClick={onStopRuntime} disabled={operationBusy || !runtimeReady} data-webcodex-action="stop-runtime">{t("home.stopRuntime")}</button>
        </div>
      </div>
    </section>
  );
}

function HomeActionButton({
  icon,
  label,
  description,
  onClick,
}: {
  icon: "project" | "runtime" | "connection";
  label: string;
  description: string;
  onClick: () => void;
}) {
  return (
    <button onClick={onClick}>
      <span className="home-action-label">
        <span className={`home-action-icon ${icon}`} aria-hidden="true">
          {icon === "project" && <span className="nav-icon nav-projects" />}
          {icon === "connection" && <span className="nav-icon nav-connection" />}
          {icon === "runtime" && <span className="home-action-play" />}
        </span>
        <span className="home-action-copy">
          <strong>{label}</strong>
          <small>{description}</small>
        </span>
      </span>
      <span className="home-action-arrow" aria-hidden="true">→</span>
    </button>
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
  if ((state.projects ?? []).length === 0) return t("home.needProject");
  if (!state.readiness.runtime_ready) return t("home.needRuntime");
  if (state.topology?.server.kind === "remote") return t("home.needChatGpt");
  if (!state.openai_tunnel_configured) return t("home.needTunnelCredentials");
  if (state.regular_tunnel?.status !== "ready") return t("home.needSecureLink");
  return t("home.needChatGpt");
}
