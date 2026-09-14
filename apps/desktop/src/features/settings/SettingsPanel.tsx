import { useEffect, useState } from "react";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState, TunnelProxyMode } from "../../models/topology";
import { LANGUAGES, useLocale } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { TunnelConfigDiagnostics } from "../connection/TunnelConfigDiagnostics";
import { PowerShellInstallGuidance } from "./PowerShellInstallGuidance";

export function SettingsPanel({
  state,
  onState,
}: {
  state: DesktopState;
  onState: (state: DesktopState) => void;
}) {
  const { locale, setLocale, t } = useLocale();
  const [proxyMode, setProxyMode] = useState<TunnelProxyMode>(state.tunnel_proxy.mode);
  const [customProxy, setCustomProxy] = useState(state.tunnel_proxy.custom_url ?? "");
  const [savingProxy, setSavingProxy] = useState(false);
  const [proxyError, setProxyError] = useState<DesktopError | null>(null);
  const [launchAtLogin, setLaunchAtLogin] = useState<boolean | null>(null);
  const [savingLaunchAtLogin, setSavingLaunchAtLogin] = useState(false);
  const [launchAtLoginError, setLaunchAtLoginError] = useState<DesktopError | null>(null);
  const operationBusy = Boolean(state.current_operation);
  const effectiveTunnelRoute = state.tunnel_proxy.effective_url
    ?? (state.tunnel_proxy.mode === "direct"
      ? t("settings.tunnelProxyDirectValue")
      : state.tunnel_proxy.mode === "auto"
        ? t("settings.tunnelProxyAutoPending")
        : state.tunnel_proxy.custom_url ?? "—");

  useEffect(() => {
    let cancelled = false;
    void desktopApi.getLaunchAtLogin().then((enabled) => {
      if (!cancelled) setLaunchAtLogin(enabled);
    }).catch((value) => {
      if (!cancelled) setLaunchAtLoginError(normalizeDesktopError(value));
    });
    return () => { cancelled = true; };
  }, []);

  const saveProxy = async () => {
    if (operationBusy) return;
    setSavingProxy(true);
    setProxyError(null);
    try {
      onState(await desktopApi.updateTunnelProxy(proxyMode, customProxy));
    } catch (value) {
      setProxyError(normalizeDesktopError(value));
    } finally {
      setSavingProxy(false);
    }
  };

  const updateLaunchAtLogin = async (enabled: boolean) => {
    if (savingLaunchAtLogin) return;
    setSavingLaunchAtLogin(true);
    setLaunchAtLoginError(null);
    try {
      setLaunchAtLogin(await desktopApi.setLaunchAtLogin(enabled));
    } catch (value) {
      setLaunchAtLoginError(normalizeDesktopError(value));
    } finally {
      setSavingLaunchAtLogin(false);
    }
  };

  return (
    <section className="page-section rc5-page settings-page" aria-labelledby="settings-title" data-webcodex-page="settings">
      <div className="eyebrow">{t("settings.eyebrow")}</div>
      <h1 id="settings-title" aria-label={t("settings.eyebrow")}>{t("settings.title")}</h1>
      <p className="lede">{t("settings.description")}</p>

      <section className="settings-group" aria-labelledby="settings-general-title">
        <div className="settings-group-heading">
          <h2 id="settings-general-title">{t("settings.interface")}</h2>
        </div>
        <div className="settings-surface">
          <div className="setting-line">
            <div>
              <label htmlFor="desktop-settings-locale">{t("locale.label")}</label>
              <span>{t("settings.languageHelp")}</span>
            </div>
            <select
              id="desktop-settings-locale"
              value={locale}
              onChange={(event) => setLocale(event.target.value as typeof locale)}
              data-webcodex-control="locale"
            >
              {LANGUAGES.map((language) => <option key={language.value} value={language.value}>{language.label}</option>)}
            </select>
          </div>
          <div className="setting-line">
            <div>
              <label htmlFor="desktop-launch-at-login">{t("settings.launchAtLoginDisplay")}</label>
              <span>{t("settings.launchAtLoginHelp")}</span>
            </div>
            <label className="switch-control" htmlFor="desktop-launch-at-login">
              <input
                id="desktop-launch-at-login"
                type="checkbox"
                aria-label={t("settings.launchAtLogin")}
                checked={launchAtLogin ?? false}
                onChange={(event) => void updateLaunchAtLogin(event.target.checked)}
                disabled={launchAtLogin === null || savingLaunchAtLogin}
                data-webcodex-control="launch-at-login"
              />
              <span aria-hidden="true" />
            </label>
          </div>
        </div>
        {launchAtLoginError && <SettingsError error={launchAtLoginError} />}
      </section>

      <section className="settings-group" aria-labelledby="settings-tunnel-title">
        <div className="settings-group-heading">
          <div>
            <h2 id="settings-tunnel-title">{t("settings.tunnel")}</h2>
            <p>{t("settings.tunnelHelp")}</p>
          </div>
        </div>
        <TunnelConfigDiagnostics state={state} onState={onState} />
      </section>

      <details className="settings-advanced settings-network" open>
        <summary>{t("settings.networkAdvanced")}</summary>
        <article className="detail-card tunnel-proxy-settings">
          <div className="field-group">
            <label htmlFor="desktop-tunnel-proxy-mode">{t("settings.tunnelProxy")}</label>
            <select
              id="desktop-tunnel-proxy-mode"
              value={proxyMode}
              onChange={(event) => setProxyMode(event.target.value as TunnelProxyMode)}
              disabled={savingProxy || operationBusy}
              data-webcodex-control="tunnel-proxy-mode"
            >
              <option value="auto">{t("settings.tunnelProxyAuto")}</option>
              <option value="direct">{t("settings.tunnelProxyDirect")}</option>
              <option value="custom">{t("settings.tunnelProxyCustom")}</option>
            </select>
            {proxyMode === "auto" && <span className="field-help">{t(navigator.platform.startsWith("Win") ? "settings.tunnelProxyAutoHelp" : "settings.tunnelProxyAutoHelpLocal")}</span>}
          </div>
          {proxyMode === "custom" && (
            <div className="field-group">
              <label htmlFor="desktop-tunnel-proxy-url">{t("settings.tunnelProxyCustomUrl")}</label>
              <input
                id="desktop-tunnel-proxy-url"
                value={customProxy}
                onChange={(event) => setCustomProxy(event.target.value)}
                placeholder="http://127.0.0.1:7890"
                disabled={savingProxy || operationBusy}
                spellCheck={false}
                data-webcodex-control="tunnel-proxy-url"
              />
              <span className="field-help">{t("settings.tunnelProxyCustomHelp")}</span>
            </div>
          )}
          <div className="tunnel-proxy-observation" aria-live="polite">
            <div>
              <span>{t("settings.tunnelProxyEffective")}</span>
              <code>{effectiveTunnelRoute}</code>
            </div>
            {state.tunnel_proxy.detected_url && (
              <div>
                <span>{t("settings.tunnelProxyDetected")}</span>
                <code>{state.tunnel_proxy.detected_url}</code>
              </div>
            )}
          </div>
          <button
            type="button"
            className="secondary-button tunnel-proxy-save"
            onClick={() => void saveProxy()}
            disabled={savingProxy || operationBusy || (proxyMode === "custom" && !customProxy.trim())}
            data-webcodex-action="save-tunnel-proxy"
          >
            {savingProxy ? t("common.checking") : t("settings.saveTunnelProxy")}
          </button>
          {proxyError && <SettingsError error={proxyError} />}
        </article>
      </details>

      <details className="settings-advanced settings-diagnostics">
        <summary>{t("settings.diagnostics")}</summary>
        <PowerShellInstallGuidance state={state} onState={onState} />
        <article className="detail-card">
          {state.binaries ? (
            <dl className="detail-list">
              <div><dt>{t("settings.version")}</dt><dd>{state.binaries.version}</dd></div>
              <div><dt>{t("settings.sourceRevision")}</dt><dd>{state.binaries.git_commit}</dd></div>
              <div><dt>{t("settings.binaryDirectory")}</dt><dd>{state.binaries.directory}</dd></div>
              <div><dt>{t("settings.runtimeProjectId")}</dt><dd>{state.project?.runtime_project_id ?? t("settings.notEstablished")}</dd></div>
            </dl>
          ) : (
            <p>{t("settings.binariesPending")}</p>
          )}
        </article>
      </details>
    </section>
  );
}

function SettingsError({ error }: { error: DesktopError }) {
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

