import { useEffect, useId, useState } from "react";
import { useLocale } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { desktopApi } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";

export function TunnelConfigDiagnostics({
  state,
  onState,
}: {
  state: DesktopState;
  onState: (state: DesktopState) => void;
}) {
  const { t } = useLocale();
  const config = state.openai_tunnel_config;
  const inputId = useId();
  const [tunnelId, setTunnelId] = useState(config.saved_tunnel_id ?? config.effective_tunnel_id ?? "");
  const [runtimeKey, setRuntimeKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);

  useEffect(() => {
    setTunnelId(config.saved_tunnel_id ?? config.effective_tunnel_id ?? "");
  }, [config.effective_tunnel_id, config.saved_tunnel_id]);

  const save = async () => {
    if (busy) return;
    const key = runtimeKey;
    setRuntimeKey("");
    setBusy(true);
    setSaved(false);
    setError(null);
    try {
      let next = await desktopApi.updateTunnelConfig({
        action: "save",
        tunnelId,
        apiKey: key || null,
      });
      onState(next);

      const localRuntimeReady = next.topology?.experience === "full"
        && next.topology.server.kind === "local"
        && next.readiness.runtime_ready;
      if (localRuntimeReady && next.openai_tunnel_configured) {
        if (next.regular_tunnel) {
          next = await desktopApi.stopRegularTunnel();
          onState(next);
        }
        next = await desktopApi.startRegularTunnel();
        onState(next);
      }
      setSaved(true);
    } catch (value) {
      setError(normalizeDesktopError(value));
    } finally {
      setBusy(false);
    }
  };

  const useEnvironment = async () => {
    if (busy) return;
    setRuntimeKey("");
    setBusy(true);
    setSaved(false);
    setError(null);
    try {
      onState(await desktopApi.updateTunnelConfig({ action: "use_environment" }));
    } catch (value) {
      setError(normalizeDesktopError(value));
    } finally {
      setBusy(false);
    }
  };

  const recheck = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      onState(await desktopApi.getState());
    } catch (value) {
      setError(normalizeDesktopError(value));
    } finally {
      setBusy(false);
    }
  };

  const savedKeyExists = config.source === "file" && config.api_key_present;
  const canSave = tunnelId.trim().length > 0 && (savedKeyExists || runtimeKey.trim().length > 0);
  return (
    <article
      className="tunnel-credentials-card"
      data-webcodex-component="tunnel-config-diagnostics"
      onKeyDown={(event) => {
        if (event.key === "Enter" && event.target instanceof HTMLInputElement) event.preventDefault();
      }}
    >
      <div className="tunnel-credentials-heading">
        <div>
          <span className="section-kicker">OpenAI Tunnel</span>
          <h3>{state.openai_tunnel_configured ? t("tunnelConfig.configuredTitle") : t("tunnelConfig.title")}</h3>
        </div>
        <span className={`credential-state ${state.openai_tunnel_configured ? "ready" : "pending"}`}>
          <i className={`status-dot ${state.openai_tunnel_configured ? "ready" : "pending"}`} aria-hidden="true" />
          {state.openai_tunnel_configured ? t("common.configured") : t("connection.notConfigured")}
        </span>
      </div>
      <p className="tunnel-config-guidance">{t("tunnelConfig.fileFirst")}</p>
      <div className="credential-fields">
        <div className="field-group">
          <label htmlFor={`${inputId}-id`}>{t("tunnelConfig.tunnelId")}</label>
          <input
            id={`${inputId}-id`}
            value={tunnelId}
            onChange={(event) => { setTunnelId(event.target.value); setSaved(false); }}
            placeholder="tunnel_…"
            maxLength={256}
            autoComplete="off"
            spellCheck={false}
            disabled={busy}
          />
        </div>
        <div className="field-group">
          <label htmlFor={`${inputId}-key`}>{t("tunnelConfig.apiKey")}</label>
          <input
            id={`${inputId}-key`}
            type="password"
            value={runtimeKey}
            onChange={(event) => { setRuntimeKey(event.target.value); setSaved(false); }}
            placeholder={savedKeyExists ? t("tunnelConfig.keepKey") : t("tunnelConfig.enterKey")}
            maxLength={8192}
            autoComplete="new-password"
            spellCheck={false}
            disabled={busy}
            aria-describedby={`${inputId}-key-help`}
          />
          <span className="field-help" id={`${inputId}-key-help`}>{t("tunnelConfig.storage")}</span>
        </div>
      </div>
      <div className="tunnel-config-actions">
        <button type="button" className="primary-button" onClick={() => void save()} disabled={busy || !canSave}>
          {busy ? t("common.checking") : t("tunnelConfig.save")}
        </button>
        {saved && <span className="save-confirmation" role="status">✓ {t("tunnelConfig.saved")}</span>}
      </div>
      {error && (
        <div className="error-card" role="alert">
          <strong>{desktopErrorPresentation(error, t).title}</strong>
          <span>{desktopErrorPresentation(error, t).action}</span>
        </div>
      )}
      <details className="credential-advanced">
        <summary>{t("settings.advanced")}</summary>
        <p>{t(config.source === "file" ? "tunnelConfig.sourceFile" : config.source === "invalid" ? "tunnelConfig.sourceInvalid" : "tunnelConfig.sourceEnvironment")}</p>
        <div className="tunnel-config-actions">
          <button type="button" className="secondary-button" onClick={() => void recheck()} disabled={busy} data-webcodex-action="recheck-tunnel-config">
            {t("tunnelConfig.recheck")}
          </button>
          {config.source !== "environment" && (
            <button type="button" className="secondary-button" onClick={() => void useEnvironment()} disabled={busy}>
              {t("tunnelConfig.useEnvironment")}
            </button>
          )}
        </div>
      </details>
    </article>
  );
}
