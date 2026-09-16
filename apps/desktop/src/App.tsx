import brandIcon from "./assets/brand.png";
import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "./lib/desktop-api";
import type {
  ActivityEntry,
  DesktopError,
  DesktopState,
} from "./models/topology";
import { FirstRun } from "./features/onboarding/FirstRun";
import { ProductOnboarding, type ProductOnboardingStep } from "./features/onboarding/ProductOnboarding";
import { Dashboard } from "./features/dashboard/Dashboard";
import { ProjectsPanel } from "./features/projects/ProjectsPanel";
import { ConnectionPanel } from "./features/connection/ConnectionPanel";
import { ActivityPanel } from "./features/activity/ActivityPanel";
import { SettingsPanel } from "./features/settings/SettingsPanel";
import { LANGUAGES, useLocale } from "./i18n/locale";
import { activationErrorMessage, desktopErrorPresentation, normalizeDesktopError, operationLabel } from "./i18n/presentation";

type Navigation = "home" | "projects" | "connection" | "activity" | "settings";

const NAVIGATION: Navigation[] = ["home", "projects", "connection", "settings"];

const REGULAR_TUNNEL_OBSERVATION_INTERVAL_MS = 1_500;
const CHATGPT_ACTIVITY_OBSERVATION_INTERVAL_MS = 30_000;
const ACTIVE_OPERATION_OBSERVATION_INTERVAL_MS = 1_000;
const AUTO_REPAIR_RETRY_INTERVAL_MS = 10_000;

export const ONBOARDING_STORAGE_KEY = "codexlink.desktop.v1.onboarding";

type ProductOnboardingSession = {
  step: ProductOnboardingStep;
  required: boolean;
};

function storedRequiredOnboarding(): ProductOnboardingSession | null {
  try {
    const value = window.localStorage.getItem(ONBOARDING_STORAGE_KEY);
    if (value === "tunnel" || value === "chatgpt") return { step: value, required: true };
  } catch {
    // Tutorial progress is presentation state only. Existing activated users
    // remain usable even if localStorage is unavailable.
  }
  return null;
}

function persistRequiredOnboarding(value: ProductOnboardingStep | "complete") {
  try {
    window.localStorage.setItem(ONBOARDING_STORAGE_KEY, value);
  } catch {
    // Runtime readiness and licensing never depend on tutorial persistence.
  }
}

export default function App() {
  const { locale, setLocale, t } = useLocale();
  const [state, setState] = useState<DesktopState | null>(null);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [navigation, setNavigation] = useState<Navigation>("home");
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [cancelSubmittingId, setCancelSubmittingId] = useState<string | null>(null);
  const [showSetup, setShowSetup] = useState(false);
  const [chooseSetupMode, setChooseSetupMode] = useState(false);
  const [productOnboarding, setProductOnboarding] = useState<ProductOnboardingSession | null>(storedRequiredOnboarding);
  const [startupAttempt, setStartupAttempt] = useState(0);
  const [windowFocused, setWindowFocused] = useState(true);
  const stateVersionRef = useRef(0);
  const autoRepairInFlightRef = useRef(false);
  const autoRepairNotBeforeRef = useRef(0);
  const mainRef = useRef<HTMLElement>(null);
  const hasRegularTunnel = Boolean(state?.regular_tunnel);
  const hasCurrentOperation = Boolean(state?.current_operation);
  const hasLoadedState = Boolean(state);
  const shouldObserveChatgptActivity = Boolean(
    state?.readiness.runtime_ready
      && state.readiness.project === "ready"
      && !state.chatgpt_activity?.observed
      && !hasCurrentOperation
      && !refreshing
      && windowFocused,
  );

  useEffect(() => {
    void desktopApi.setUiLocale(locale).catch(() => {
      // Tray localization is presentation-only; the in-window UI remains usable
      // even if the native shell cannot refresh its menu.
    });
  }, [locale]);

  useEffect(() => {
    const onFocus = () => setWindowFocused(true);
    const onBlur = () => setWindowFocused(false);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  useEffect(() => {
    mainRef.current?.focus({ preventScroll: true });
    mainRef.current?.scrollTo?.({ top: 0 });
  }, [navigation, showSetup, productOnboarding?.step]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<unknown>("desktop:navigate", (event) => {
      if (event.payload !== "activity" && event.payload !== "settings") return;
      setShowSetup(false);
      setChooseSetupMode(false);
      setNavigation(event.payload);
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    }).catch(() => {
      // Host navigation is optional. Ordinary in-window navigation remains
      // usable if the native event subscription is unavailable.
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const navigateWithKeyboard = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey || event.repeat) return;
      const page = NAVIGATION[Number(event.key) - 1];
      if (!page) return;
      event.preventDefault();
      setNavigation(page);
    };
    window.addEventListener("keydown", navigateWithKeyboard);
    return () => window.removeEventListener("keydown", navigateWithKeyboard);
  }, []);

  const openSetup = () => {
    setChooseSetupMode(true);
    setShowSetup(true);
    setNavigation("home");
  };

  const commitState = useCallback((next: DesktopState) => {
    stateVersionRef.current += 1;
    setState(next);
  }, []);

  const commitActivatedState = useCallback((next: DesktopState) => {
    commitState(next);
    setShowSetup(false);
    setChooseSetupMode(false);
    const step: ProductOnboardingStep = next.openai_tunnel_configured ? "chatgpt" : "tunnel";
    persistRequiredOnboarding(step);
    setProductOnboarding({ step, required: true });
    setStartupAttempt((attempt) => attempt + 1);
  }, [commitState]);

  const advanceProductOnboarding = (step: ProductOnboardingStep) => {
    setProductOnboarding((current) => {
      if (!current) return current;
      if (current.required) persistRequiredOnboarding(step);
      return { ...current, step };
    });
  };

  const completeProductOnboarding = () => {
    if (productOnboarding?.required) persistRequiredOnboarding("complete");
    setProductOnboarding(null);
    setShowSetup(false);
    setChooseSetupMode(false);
    setNavigation("home");
  };

  const openProductOnboardingFromSettings = () => {
    setShowSetup(false);
    setChooseSetupMode(false);
    setNavigation("home");
    setProductOnboarding({ step: "tunnel", required: false });
  };

  const openOtherSetupFromOnboarding = () => {
    if (productOnboarding?.required) persistRequiredOnboarding("complete");
    setProductOnboarding(null);
    openSetup();
  };

  const commitChatgptActivity = useCallback((next: DesktopState) => {
    stateVersionRef.current += 1;
    setState((current) => {
      if (!current) return next;
      if (current.chatgpt_activity?.observed && !next.chatgpt_activity?.observed) return current;
      return { ...current, chatgpt_activity: next.chatgpt_activity };
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const initial = await desktopApi.getState();
        if (cancelled) return;
        commitState(initial);
        if (initial.license.status !== "activated") return;
        if (initial.current_operation) return;
        const shouldPrepareLocalRuntime = !initial.topology;
        const resumeExisting = Boolean(
          shouldPrepareLocalRuntime
          || (initial.topology
            && initial.runtime_autostart
            && initial.topology.experience === "full"),
        );
        if (!resumeExisting) return;

        setRefreshing(true);
        try {
          const next = await restoreManagedConnection();
          if (cancelled) return;
          commitState(next);
          setShowSetup(false);
        } catch (value) {
          if (!cancelled) setError(normalizeDesktopError(value));
        } finally {
          if (!cancelled) setRefreshing(false);
        }
      } catch (value) {
        if (!cancelled) setError(normalizeDesktopError(value));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [commitState, startupAttempt]);

  useEffect(() => {
    if (!hasLoadedState || state?.license.status !== "activated") return;

    let cancelled = false;
    let timeoutId: number | undefined;
    const interval = hasCurrentOperation || refreshing
      ? ACTIVE_OPERATION_OBSERVATION_INTERVAL_MS
      : REGULAR_TUNNEL_OBSERVATION_INTERVAL_MS;

    const scheduleObservation = () => {
      timeoutId = window.setTimeout(() => {
        void (async () => {
          const observedVersion = stateVersionRef.current;
          try {
            const next = await desktopApi.getState();
            if (!cancelled && stateVersionRef.current === observedVersion) {
              commitState(next);
              if (
                shouldAutoRepairConnection(next)
                && !autoRepairInFlightRef.current
                && Date.now() >= autoRepairNotBeforeRef.current
              ) {
                autoRepairInFlightRef.current = true;
                autoRepairNotBeforeRef.current = Date.now() + AUTO_REPAIR_RETRY_INTERVAL_MS;
                try {
                  const repaired = await restoreManagedConnection();
                  if (!cancelled) {
                    commitState(repaired);
                    setError(null);
                  }
                } catch {
                  // Automatic healing is best-effort. Keep the degraded state visible
                  // so the unified manual recovery action remains available.
                } finally {
                  autoRepairInFlightRef.current = false;
                }
              }
            }
          } catch {
            // Observation is best-effort. A transient invoke failure must not
            // create an error storm or a second concurrent observer.
          } finally {
            if (!cancelled) scheduleObservation();
          }
        })();
      }, interval);
    };

    scheduleObservation();
    return () => {
      cancelled = true;
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    };
  }, [commitState, hasCurrentOperation, hasLoadedState, hasRegularTunnel, refreshing, state?.license.status]);

  useEffect(() => {
    if (!shouldObserveChatgptActivity) return;

    let cancelled = false;
    let timeoutId: number | undefined;
    const observe = async () => {
      try {
        const next = await desktopApi.observeChatgptActivity();
        if (!cancelled) commitChatgptActivity(next);
      } catch {
        // Observation is best-effort. Keep the runtime usable and retry only
        // while the Desktop window remains focused.
      } finally {
        if (!cancelled) {
          timeoutId = window.setTimeout(
            () => void observe(),
            CHATGPT_ACTIVITY_OBSERVATION_INTERVAL_MS,
          );
        }
      }
    };

    void observe();
    return () => {
      cancelled = true;
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    };
  }, [commitChatgptActivity, shouldObserveChatgptActivity]);

  useEffect(() => {
    if (navigation === "activity") {
      void desktopApi.activity().then(setActivity).catch(() => undefined);
    }
  }, [navigation, state?.activity_sequence]);

  const runStateOperation = async (operation: () => Promise<DesktopState>) => {
    setError(null);
    try {
      commitState(await operation());
    } catch (value) {
      setError(normalizeDesktopError(value));
    }
  };

  const chooseLocalProject = async () => {
    if (!state || state.current_operation) return;
    setError(null);
    try {
      const selection = await open({
        directory: true,
        multiple: false,
        title: t("setup.chooseProject"),
      });
      if (typeof selection !== "string") return;

      const topology = state.topology;
      const canSwitchInPlace = Boolean(
        topology
        && topology.experience === "full"
        && state.readiness.runtime_ready,
      );
      if (!canSwitchInPlace) {
        if (topology?.server.kind === "remote") {
          commitState(await desktopApi.configureRemote(topology.server.url, "", selection));
        } else {
          commitState(await desktopApi.configureLocal(selection));
        }
        setShowSetup(false);
        return;
      }

      try {
        commitState(await desktopApi.activateLocalProject(selection));
      } catch (value) {
        const normalized = normalizeDesktopError(value);
        if (
          normalized.code !== "project_activation_capability_unavailable" &&
          normalized.code !== "project_activation_restart_required"
        ) {
          throw normalized;
        }
        // Legacy managed Runners may need the bounded topology-specific setup
        // fallback to refresh only the Desktop-owned Runner without re-pairing.
        if (topology?.server.kind === "remote") {
          commitState(await desktopApi.configureRemote(topology.server.url, "", selection));
        } else {
          commitState(await desktopApi.configureLocal(selection));
        }
      }
      setShowSetup(false);
    } catch (value) {
      setError(normalizeDesktopError(value));
    }
  };

  const refresh = async () => {
    setRefreshing(true);
    try {
      await runStateOperation(desktopApi.refresh);
    } finally {
      setRefreshing(false);
    }
  };

  const resumeRuntime = async () => {
    setRefreshing(true);
    setError(null);
    try {
      commitState(await restoreManagedConnection());
    } catch (value) {
      setError(normalizeDesktopError(value));
    } finally {
      setRefreshing(false);
    }
  };

  const cancelCurrentOperation = async () => {
    const observed = state?.current_operation;
    if (!observed || !observed.cancellable || observed.phase === "cancelling") return;
    setCancelSubmittingId(observed.id);
    setError(null);
    try {
      commitState(await desktopApi.cancelOperation(observed.id));
    } catch (value) {
      setError(normalizeDesktopError(value));
    } finally {
      setCancelSubmittingId((current) => current === observed.id ? null : current);
    }
  };

  if (!state) {
    return (
      <main className="splash">
        <img className="brand-mark" src={brandIcon} alt="" />
        {error ? (
          <section className="startup-error" aria-label="CodexLink">
            <AppError error={error} />
            <button
              className="primary-button"
              type="button"
              onClick={() => {
                setError(null);
                setStartupAttempt((attempt) => attempt + 1);
              }}
              data-webcodex-action="retry-desktop-startup"
            >
              {t("common.retry")}
            </button>
          </section>
        ) : (
          <span role="status">{t("app.loading")}</span>
        )}
      </main>
    );
  }

  if (state.license.status !== "activated") {
    return (
      <ActivationGate
        state={state}
        onActivated={commitActivatedState}
      />
    );
  }

  if (productOnboarding) {
    return (
      <ProductOnboarding
        state={state}
        step={productOnboarding.step}
        onState={commitState}
        onStep={advanceProductOnboarding}
        onComplete={completeProductOnboarding}
        onOtherSetup={openOtherSetupFromOnboarding}
      />
    );
  }

  const needsSetup = showSetup;

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand"><img className="brand-mark" src={brandIcon} alt="" /><div><strong>CodexLink</strong><span>{t("app.subtitle")}</span></div></div>
        <nav aria-label={t("nav.main")}>
          {NAVIGATION.map((item, index) => (
            <button
              key={item}
              className={navigation === item ? "active" : ""}
              onClick={() => setNavigation(item)}
              aria-current={navigation === item ? "page" : undefined}
              aria-keyshortcuts={`Control+${index + 1} Meta+${index + 1}`}
              title={`${t(`nav.${item}`)} (⌘ / Ctrl + ${index + 1})`}
              data-webcodex-action={`navigate-${item}`}
            >
              <span className={`nav-icon nav-${item}`} aria-hidden="true" />
              {t(`nav.${item}`)}
              <kbd aria-hidden="true">{index + 1}</kbd>
            </button>
          ))}
        </nav>
        <div className="sidebar-locale">
          <label htmlFor="desktop-sidebar-locale">{t("locale.label")}</label>
          <select
            id="desktop-sidebar-locale"
            aria-label={t("locale.label")}
            value={locale}
            onChange={(event) => setLocale(event.target.value as typeof locale)}
            data-webcodex-control="sidebar-locale"
          >
            {LANGUAGES.map((language) => <option key={language.value} value={language.value}>{language.label}</option>)}
          </select>
        </div>
      </aside>

      <div className="workspace-shell">

        <main className="main-content" ref={mainRef} tabIndex={-1}>
        {navigation === "home" && chooseSetupMode && state.topology && (
          <button className="back-button" onClick={() => { setShowSetup(false); setChooseSetupMode(false); }}>
            <span aria-hidden="true">← </span>{t("home.backToOverview")}
          </button>
        )}
        {state.current_operation && (
          <section
            className={`operation-status ${state.current_operation.phase}`}
            role="status"
            aria-live="polite"
            aria-label={t("operation.statusLabel")}
            data-webcodex-operation={state.current_operation.kind}
          >
            <div>
              <span className="section-kicker">
                {state.current_operation.phase === "cancelling"
                  ? t("operation.cancelling")
                  : t("operation.running")}
              </span>
              <strong>{operationLabel(state.current_operation.kind, t)}</strong>
              <span>{t("operation.cancelNote")}</span>
            </div>
            {state.current_operation.cancellable && (
              <button
                className="secondary-button"
                type="button"
                disabled={
                  state.current_operation.phase === "cancelling" ||
                  cancelSubmittingId === state.current_operation.id
                }
                onClick={() => void cancelCurrentOperation()}
                data-webcodex-action="cancel-desktop-operation"
              >
                {state.current_operation.phase === "cancelling"
                  ? t("operation.cancelling")
                  : t("operation.cancel")}
              </button>
            )}
          </section>
        )}
        {error && <AppError error={error} />}
        {navigation === "home" && (needsSetup ? (
          <FirstRun
            state={state}
            onState={commitState}
            chooseModeFirst={chooseSetupMode}
            onComplete={() => { setShowSetup(false); setChooseSetupMode(false); }}
          />
        ) : (
          <Dashboard
            state={state}
            refreshing={refreshing}
            onRefresh={() => void refresh()}
            onResumeRuntime={() => void resumeRuntime()}
            onChooseProject={() => void chooseLocalProject()}
            onChangeSetup={openSetup}
            onNavigate={setNavigation}
            onStopRuntime={() => void runStateOperation(desktopApi.stopLocalRuntime)}
          />
        ))}
        {navigation === "projects" && (
          <ProjectsPanel
            state={state}
            onChooseProject={() => void chooseLocalProject()}
            onRemoveProject={(projectPath) => void runStateOperation(() => desktopApi.removeLocalProject(projectPath))}
          />
        )}
        {navigation === "connection" && (
          <ConnectionPanel
            state={state}
            onState={commitState}
            onOpenSettings={() => setNavigation("settings")}
          />
        )}
        {navigation === "activity" && <ActivityPanel activity={activity} />}
        {navigation === "settings" && (
          <SettingsPanel
            state={state}
            onState={commitState}
            onOpenOnboarding={openProductOnboardingFromSettings}
          />
        )}
        </main>
      </div>
    </div>
  );
}

function ActivationGate({ state, onActivated }: { state: DesktopState; onActivated: (state: DesktopState) => void }) {
  const { locale, setLocale, t } = useLocale();
  const [activationCode, setActivationCode] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [activationError, setActivationError] = useState<string | null>(null);
  const recovery = state.license.status === "invalid";

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (submitting) return;
    setSubmitting(true);
    setActivationError(null);
    try {
      const next = await desktopApi.activateLicense(activationCode);
      setActivationCode("");
      onActivated(next);
    } catch (value) {
      const normalized = normalizeDesktopError(value);
      setActivationCode("");
      setActivationError(activationErrorMessage(normalized, t));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <main className="activation-shell">
      <section className="activation-card" aria-labelledby="activation-title">
        <div className="activation-brand">
          <img className="brand-mark" src={brandIcon} alt="" />
          <div><strong>CodexLink</strong><span>{t("app.subtitle")}</span></div>
        </div>
        <span className="section-kicker">{t("activation.eyebrow")}</span>
        <h1 id="activation-title">{recovery ? t("activation.recoveryTitle") : t("activation.title")}</h1>
        <p className="lede">{recovery ? t("activation.recoveryBody") : t("activation.firstUse")}</p>
        <form className="activation-form" onSubmit={(event) => void submit(event)}>
          <div className="field-group">
            <label htmlFor="codexlink-activation-code">{t("activation.codeLabel")}</label>
            <input
              id="codexlink-activation-code"
              value={activationCode}
              onChange={(event) => setActivationCode(event.target.value)}
              placeholder="MYWS-XXXXX-XXXXX-XXXXX-XXXXX"
              autoComplete="off"
              spellCheck={false}
              disabled={submitting}
              data-webcodex-control="license-activation-code"
            />
            <span className="field-help">{t("activation.codeHelp")}</span>
          </div>
          {activationError && <p className="activation-recovery" role="alert">{activationError}</p>}
          <button
            className="primary-button"
            type="submit"
            disabled={submitting || activationCode.trim().length === 0}
            data-webcodex-action="activate-license"
          >
            {submitting ? t("activation.activating") : t("activation.submit")}
          </button>
        </form>
        <div className="activation-locale">
          <label htmlFor="activation-locale">{t("locale.label")}</label>
          <select
            id="activation-locale"
            value={locale}
            onChange={(event) => setLocale(event.target.value as typeof locale)}
          >
            {LANGUAGES.map((language) => <option key={language.value} value={language.value}>{language.label}</option>)}
          </select>
        </div>
      </section>
    </main>
  );
}

async function restoreManagedConnection(): Promise<DesktopState> {
  let next = await desktopApi.resumeSavedRuntime();
  if (shouldStartPreferredTunnel(next)) {
    next = await desktopApi.startRegularTunnel();
  }
  return next;
}

function shouldAutoRepairConnection(state: DesktopState) {
  return state.license.status === "activated"
    && !state.current_operation
    && state.runtime_autostart
    && state.topology?.experience === "full"
    && state.topology.server.kind === "local"
    && (!state.readiness.runtime_ready || shouldStartPreferredTunnel(state));
}

function shouldStartPreferredTunnel(state: DesktopState) {
  return state.license.status === "activated" &&
    state.preferred_connection === "open_ai_tunnel" &&
    state.topology?.experience === "full" &&
    state.topology.server.kind === "local" &&
    state.readiness.runtime_ready &&
    state.openai_tunnel_configured &&
    state.regular_tunnel?.status !== "ready";
}

function AppError({ error }: { error: DesktopError }) {
  const { t } = useLocale();
  const presentation = desktopErrorPresentation(error, t);
  return (
    <div className="error-card app-error" role="alert">
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
