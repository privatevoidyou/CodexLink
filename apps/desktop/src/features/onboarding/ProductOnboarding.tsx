import { useEffect, useState } from "react";
import brandIcon from "../../assets/brand.png";
import { ChatGptAppTutorials, TunnelCredentialTutorials } from "./TutorialGallery";
import { useLocale } from "../../i18n/locale";
import { desktopErrorPresentation, normalizeDesktopError } from "../../i18n/presentation";
import { desktopApi, type ProductLinkTarget } from "../../lib/desktop-api";
import type { DesktopError, DesktopState } from "../../models/topology";

export type ProductOnboardingStep = "tunnel" | "chatgpt";

interface ProductOnboardingProps {
  state: DesktopState;
  step: ProductOnboardingStep;
  onState: (state: DesktopState) => void;
  onStep: (step: ProductOnboardingStep) => void;
  onComplete: () => void;
}

export function ProductOnboarding({
  state,
  step,
  onState,
  onStep,
  onComplete,
}: ProductOnboardingProps) {
  const { t } = useLocale();
  const config = state.openai_tunnel_config;
  const [tunnelId, setTunnelId] = useState(config.saved_tunnel_id ?? config.effective_tunnel_id ?? "");
  const [runtimeKey, setRuntimeKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);

  useEffect(() => {
    setTunnelId(config.saved_tunnel_id ?? config.effective_tunnel_id ?? "");
  }, [config.effective_tunnel_id, config.saved_tunnel_id]);

  const currentTunnelId = config.saved_tunnel_id ?? config.effective_tunnel_id ?? "";
  const savedKeyExists = config.source === "file" && config.api_key_present;
  const existingCredentialsReady = state.openai_tunnel_configured
    && config.api_key_present
    && currentTunnelId.trim().length > 0
    && tunnelId.trim() === currentTunnelId.trim()
    && runtimeKey.trim().length === 0;
  const canSave = tunnelId.trim().length > 0 && (savedKeyExists || runtimeKey.trim().length > 0);
  const canContinue = existingCredentialsReady || canSave;
  const mutationBusy = busy || Boolean(state.current_operation);

  const openProductLink = async (target: ProductLinkTarget) => {
    setError(null);
    try {
      await desktopApi.openProductLink(target);
    } catch (value) {
      setError(normalizeDesktopError(value));
    }
  };

  const saveAndContinue = async () => {
    if (mutationBusy || !canContinue) return;
    if (existingCredentialsReady) {
      setError(null);
      onStep("chatgpt");
      return;
    }
    const key = runtimeKey;
    setRuntimeKey("");
    setBusy(true);
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
      onStep("chatgpt");
    } catch (value) {
      setError(normalizeDesktopError(value));
    } finally {
      setBusy(false);
    }
  };

  const presentation = error ? desktopErrorPresentation(error, t) : null;

  return (
    <main className="onboarding-shell" data-webcodex-page="product-onboarding">
      <section className="onboarding-card" aria-labelledby="onboarding-title">
        <div className="onboarding-brand">
          <img className="brand-mark" src={brandIcon} alt="" />
          <div><strong>CodexLink</strong><span>{t("app.subtitle")}</span></div>
        </div>
        <div className="onboarding-step-row" aria-label={t("onboarding.progress", { step: step === "tunnel" ? 1 : 2 })}>
          <div className={step === "tunnel" ? "active" : "complete"}><span>1</span>{t("onboarding.stepTunnel")}</div>
          <i aria-hidden="true" />
          <div className={step === "chatgpt" ? "active" : ""}><span>2</span>{t("onboarding.stepChatgpt")}</div>
        </div>

        {step === "tunnel" ? (
          <>
            <span className="section-kicker">{t("onboarding.connectEyebrow")}</span>
            <h1 id="onboarding-title">{t("onboarding.connectTitle")}</h1>
            <p className="lede onboarding-lede">{t("onboarding.connectDescription")}</p>

            <div className="onboarding-credentials">
              <div className="field-group">
                <label htmlFor="onboarding-tunnel-id">{t("tunnelConfig.tunnelId")}</label>
                <input
                  id="onboarding-tunnel-id"
                  value={tunnelId}
                  onChange={(event) => setTunnelId(event.target.value)}
                  placeholder="tunnel_…"
                  maxLength={256}
                  autoComplete="off"
                  spellCheck={false}
                  disabled={mutationBusy}
                  data-webcodex-control="onboarding-tunnel-id"
                />
                <button className="onboarding-link" type="button" onClick={() => void openProductLink("openai_tunnels")}>
                  {t("onboarding.createTunnel")} <span aria-hidden="true">↗</span>
                </button>
              </div>
              <div className="field-group">
                <label htmlFor="onboarding-runtime-key">{t("tunnelConfig.apiKey")}</label>
                <input
                  id="onboarding-runtime-key"
                  type="password"
                  value={runtimeKey}
                  onChange={(event) => setRuntimeKey(event.target.value)}
                  placeholder={savedKeyExists ? t("tunnelConfig.keepKey") : t("tunnelConfig.enterKey")}
                  maxLength={8192}
                  autoComplete="new-password"
                  spellCheck={false}
                  disabled={mutationBusy}
                  data-webcodex-control="onboarding-runtime-key"
                />
                <button className="onboarding-link" type="button" onClick={() => void openProductLink("openai_api_keys")}>
                  {t("onboarding.createKey")} <span aria-hidden="true">↗</span>
                </button>
              </div>
            </div>
            <p className="onboarding-security-note">{t("onboarding.credentialStorage")}</p>
            <details className="onboarding-details tutorial-details">
              <summary>{t("onboarding.credentialGuide")}</summary>
              <p>{t("onboarding.tutorialPreviewHint")}</p>
              <TunnelCredentialTutorials />
            </details>
            {error && (
              <div className="error-card" role="alert">
                <strong>{presentation?.title}</strong>
                <span>{presentation?.action}</span>
              </div>
            )}
            <div className="onboarding-actions">
              <button className="text-button onboarding-skip" type="button" onClick={() => onStep("chatgpt")} data-webcodex-action="onboarding-skip-configured">
                {t("onboarding.skipConfigured")}
              </button>
              <button className="primary-button" type="button" onClick={() => void saveAndContinue()} disabled={mutationBusy || !canContinue} data-webcodex-action="onboarding-next">
                {mutationBusy ? t("common.checking") : t("onboarding.next")}
              </button>
            </div>
          </>
        ) : (
          <>
            <span className="section-kicker">{t("onboarding.chatgptEyebrow")}</span>
            <h1 id="onboarding-title">{t("onboarding.chatgptTitle")}</h1>
            <p className="lede onboarding-lede">{t("onboarding.chatgptDescription")}</p>
            <ol className="onboarding-guide-list">
              <li><span>1</span><strong>{t("onboarding.chatgptStep1")}</strong></li>
              <li><span>2</span><strong>{t("onboarding.chatgptStep2")}</strong></li>
              <li><span>3</span><strong>{t("onboarding.chatgptStep3")}</strong></li>
              <li><span>4</span><strong>{t("onboarding.chatgptStep4")}</strong></li>
            </ol>
            <div className="onboarding-chatgpt-actions">
              <button className="secondary-button" type="button" onClick={() => void openProductLink("chatgpt")} data-webcodex-action="onboarding-open-chatgpt">
                {t("onboarding.openChatgpt")} <span aria-hidden="true">↗</span>
              </button>
            </div>
            <details className="onboarding-details">
              <summary>{t("onboarding.detailedGuide")}</summary>
              <p>{t("onboarding.detailIntro")}</p>
              <p className="tutorial-preview-hint">{t("onboarding.tutorialPreviewHint")}</p>
              <ChatGptAppTutorials />
              <ol>
                <li>{t("onboarding.detail1")}</li>
                <li>{t("onboarding.detail2")}</li>
                <li>{t("onboarding.detail3")}</li>
                <li>{t("onboarding.detail4")}</li>
              </ol>
            </details>
            {error && (
              <div className="error-card" role="alert">
                <strong>{presentation?.title}</strong>
                <span>{presentation?.action}</span>
              </div>
            )}
            <div className="onboarding-actions">
              <button className="text-button onboarding-skip" type="button" onClick={onComplete} data-webcodex-action="onboarding-skip-added">
                {t("onboarding.skipAdded")}
              </button>
              <button className="primary-button" type="button" onClick={onComplete} data-webcodex-action="onboarding-done">
                {t("onboarding.done")}
              </button>
            </div>
          </>
        )}
      </section>
    </main>
  );
}
