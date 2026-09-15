use crate::desktop_shell::{self, NavigationTarget};
use crate::models::{
    DesktopOperationPhase, DesktopStateSnapshot, Experience, RunnerReadiness, ServerReadiness,
    ServerTopology,
};
use crate::state::AppState;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
#[cfg(target_os = "windows")]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{AppHandle, Manager};

const TRAY_ID: &str = "webcodex-desktop";
const OPEN_ID: &str = "tray.open";
const ACTIVITY_ID: &str = "tray.activity";
const SETTINGS_ID: &str = "tray.settings";
const RESUME_RUNTIME_ID: &str = "tray.resume_runtime";
const STOP_RUNTIME_ID: &str = "tray.stop_runtime";
const CONNECT_ID: &str = "tray.connect";
const DISCONNECT_ID: &str = "tray.disconnect";
const STOP_QUICK_SHARE_ID: &str = "tray.stop_quick_share";
const CANCEL_OPERATION_PREFIX: &str = "tray.cancel_operation:";
const LAUNCH_AT_LOGIN_ID: &str = "tray.launch_at_login";
const QUIT_ID: &str = "tray.quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeStatus {
    Ready,
    Stopped,
    NeedsAttention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionStatus {
    Ready,
    ObservedUse,
    WaitingForChatGpt,
    NotConnected,
    NeedsAttention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeAction {
    Resume,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionAction {
    Connect,
    Disconnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrayProjection {
    runtime_status: RuntimeStatus,
    connection_status: ConnectionStatus,
    runtime_action: Option<RuntimeAction>,
    connection_action: Option<ConnectionAction>,
    stop_quick_share: bool,
    cancel_operation_id: Option<String>,
    cancel_operation_enabled: bool,
    operation_busy: bool,
    launch_at_login: Option<bool>,
}

#[derive(Default)]
pub struct TrayPresentationCache {
    projection: Mutex<Option<TrayProjection>>,
    launch_at_login: Mutex<Option<bool>>,
    locale: Mutex<String>,
}

#[derive(Debug, Clone, Copy)]
struct TrayTexts {
    open: &'static str,
    quit: &'static str,
}

fn normalize_tray_locale(locale: &str) -> &'static str {
    match locale {
        "en-US" => "en-US",
        "ja-JP" => "ja-JP",
        "ko-KR" => "ko-KR",
        "de-DE" => "de-DE",
        "fr-FR" => "fr-FR",
        _ => "zh-CN",
    }
}

fn tray_texts(locale: &str) -> TrayTexts {
    match normalize_tray_locale(locale) {
        "en-US" => TrayTexts {
            open: "Open CodexLink",
            quit: "Quit CodexLink",
        },
        "ja-JP" => TrayTexts {
            open: "CodexLink を開く",
            quit: "CodexLink を終了",
        },
        "ko-KR" => TrayTexts {
            open: "CodexLink 열기",
            quit: "CodexLink 종료",
        },
        "de-DE" => TrayTexts {
            open: "CodexLink öffnen",
            quit: "CodexLink beenden",
        },
        "fr-FR" => TrayTexts {
            open: "Ouvrir CodexLink",
            quit: "Quitter CodexLink",
        },
        _ => TrayTexts {
            open: "打开 CodexLink",
            quit: "退出 CodexLink",
        },
    }
}

impl TrayProjection {
    fn from_snapshot(snapshot: &DesktopStateSnapshot, launch_at_login: Option<bool>) -> Self {
        let local_full = snapshot.topology.as_ref().is_some_and(|topology| {
            topology.experience == Experience::Full
                && matches!(&topology.server, ServerTopology::Local)
        });
        let runtime_stopped = matches!(snapshot.readiness.server, ServerReadiness::Stopped)
            && matches!(snapshot.readiness.runner, RunnerReadiness::Stopped);
        let licensed = snapshot.license.is_active();
        let runtime_status = if snapshot.readiness.runtime_ready {
            RuntimeStatus::Ready
        } else if snapshot.topology.is_none() || runtime_stopped {
            RuntimeStatus::Stopped
        } else {
            RuntimeStatus::NeedsAttention
        };
        let tunnel_error = snapshot
            .regular_tunnel
            .as_ref()
            .is_some_and(|tunnel| tunnel.status == crate::models::RegularTunnelStatus::Error);
        let observed_use = snapshot.readiness.runtime_ready
            && snapshot
                .chatgpt_activity
                .as_ref()
                .is_some_and(|activity| activity.observed);
        let connection_status = if snapshot.readiness.ready_for_chatgpt {
            ConnectionStatus::Ready
        } else if tunnel_error {
            ConnectionStatus::NeedsAttention
        } else if observed_use {
            ConnectionStatus::ObservedUse
        } else if snapshot.regular_tunnel.as_ref().is_some_and(|tunnel| {
            tunnel.status == crate::models::RegularTunnelStatus::Ready && tunnel.ready_for_chatgpt
        }) {
            ConnectionStatus::WaitingForChatGpt
        } else if snapshot.regular_tunnel.is_some() {
            ConnectionStatus::NeedsAttention
        } else {
            ConnectionStatus::NotConnected
        };
        let runtime_action = if local_full && !runtime_stopped {
            Some(RuntimeAction::Stop)
        } else if licensed && local_full && runtime_stopped {
            Some(RuntimeAction::Resume)
        } else {
            None
        };
        let connection_action = if local_full && snapshot.regular_tunnel.is_some() {
            Some(ConnectionAction::Disconnect)
        } else if licensed
            && local_full
            && snapshot.readiness.runtime_ready
            && snapshot.openai_tunnel_configured
            && snapshot.regular_tunnel_available
        {
            Some(ConnectionAction::Connect)
        } else {
            None
        };
        let cancel_operation_id = snapshot
            .current_operation
            .as_ref()
            .filter(|operation| operation.cancellable)
            .map(|operation| operation.id.clone());
        let cancel_operation_enabled =
            snapshot
                .current_operation
                .as_ref()
                .is_some_and(|operation| {
                    operation.cancellable && operation.phase == DesktopOperationPhase::Running
                });
        Self {
            runtime_status,
            connection_status,
            runtime_action,
            connection_action,
            stop_quick_share: snapshot.quick_share.is_some(),
            cancel_operation_id,
            cancel_operation_enabled,
            operation_busy: snapshot.current_operation.is_some(),
            launch_at_login,
        }
    }
}

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = app.state::<AppState>().get_state();
    let launch_at_login = desktop_shell::launch_at_login_enabled(app).ok();
    app.state::<TrayPresentationCache>()
        .launch_at_login
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone_from(&launch_at_login);
    let projection = TrayProjection::from_snapshot(&snapshot, launch_at_login);
    let locale = current_locale(app);
    let menu = build_menu(app, &projection, &locale)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("CodexLink")
        .icon_as_template(cfg!(target_os = "macos"))
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            observe_launch_at_login(app);
            let snapshot = app.state::<AppState>().get_state();
            refresh_from_snapshot(app, &snapshot);
            let _ = &event;
            #[cfg(target_os = "windows")]
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                let _ = desktop_shell::show_main_window(app);
            }
        });
    #[cfg(target_os = "windows")]
    {
        if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/24x24.png")) {
            builder = builder.icon(icon);
        } else if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
    }
    #[cfg(not(target_os = "windows"))]
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.state::<TrayPresentationCache>()
        .projection
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .replace(projection);
    Ok(())
}

pub fn refresh_from_snapshot(app: &AppHandle, snapshot: &DesktopStateSnapshot) {
    let cache = app.state::<TrayPresentationCache>();
    let launch_at_login = *cache
        .launch_at_login
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let projection = TrayProjection::from_snapshot(snapshot, launch_at_login);
    {
        let cached = cache
            .projection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cached.as_ref() == Some(&projection) {
            return;
        }
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let locale = current_locale(app);
    match build_menu(app, &projection, &locale).and_then(|menu| tray.set_menu(Some(menu))) {
        Ok(()) => {
            cache
                .projection
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .replace(projection);
        }
        Err(error) => eprintln!("CodexLink tray refresh failed: {error}"),
    }
}

pub fn set_launch_at_login_observation(app: &AppHandle, observed: Option<bool>) {
    let cache = app.state::<TrayPresentationCache>();
    cache
        .launch_at_login
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone_from(&observed);
    let snapshot = app.state::<AppState>().get_state();
    refresh_from_snapshot(app, &snapshot);
}

fn observe_launch_at_login(app: &AppHandle) {
    set_launch_at_login_observation(app, desktop_shell::launch_at_login_enabled(app).ok());
}

fn current_locale(app: &AppHandle) -> String {
    let cache = app.state::<TrayPresentationCache>();
    let locale = cache
        .locale
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    normalize_tray_locale(locale.as_str()).to_string()
}

pub fn set_ui_locale(app: &AppHandle, locale: &str) {
    let normalized = normalize_tray_locale(locale).to_string();
    let cache = app.state::<TrayPresentationCache>();
    let changed = {
        let mut current = cache
            .locale
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *current == normalized {
            false
        } else {
            *current = normalized;
            true
        }
    };
    if !changed {
        return;
    }
    cache
        .projection
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    let snapshot = app.state::<AppState>().get_state();
    refresh_from_snapshot(app, &snapshot);
}

fn build_menu(
    app: &AppHandle,
    _projection: &TrayProjection,
    locale: &str,
) -> tauri::Result<Menu<tauri::Wry>> {
    let text = tray_texts(locale);
    let menu = Menu::new(app)?;
    let open = MenuItem::with_id(app, OPEN_ID, text.open, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, text.quit, true, None::<&str>)?;
    menu.append_items(&[&open, &quit])?;
    Ok(menu)
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        OPEN_ID => {
            let _ = desktop_shell::show_main_window(app);
        }
        ACTIVITY_ID => {
            let _ = desktop_shell::navigate(app, NavigationTarget::Activity);
        }
        SETTINGS_ID => {
            let _ = desktop_shell::navigate(app, NavigationTarget::Settings);
        }
        RESUME_RUNTIME_ID => spawn_state_action(app, TrayStateAction::ResumeRuntime),
        STOP_RUNTIME_ID => spawn_state_action(app, TrayStateAction::StopRuntime),
        CONNECT_ID => spawn_state_action(app, TrayStateAction::StartRegularTunnel),
        DISCONNECT_ID => spawn_state_action(app, TrayStateAction::StopRegularTunnel),
        STOP_QUICK_SHARE_ID => spawn_state_action(app, TrayStateAction::StopQuickShare),
        LAUNCH_AT_LOGIN_ID => match desktop_shell::launch_at_login_enabled(app) {
            Ok(current) => match desktop_shell::set_launch_at_login(app, !current) {
                Ok(enabled) => set_launch_at_login_observation(app, Some(enabled)),
                Err(_) => {
                    observe_launch_at_login(app);
                    let _ = desktop_shell::navigate(app, NavigationTarget::Settings);
                }
            },
            Err(_) => {
                set_launch_at_login_observation(app, None);
                let _ = desktop_shell::navigate(app, NavigationTarget::Settings);
            }
        },
        QUIT_ID => desktop_shell::request_application_exit(app),
        _ => {
            if let Some(action) = cancel_action_from_menu_id(id) {
                spawn_state_action(app, action);
            }
        }
    }
}

fn cancel_action_from_menu_id(id: &str) -> Option<TrayStateAction> {
    id.strip_prefix(CANCEL_OPERATION_PREFIX)
        .filter(|operation_id| !operation_id.is_empty())
        .map(|operation_id| TrayStateAction::CancelOperation(operation_id.to_owned()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TrayStateAction {
    ResumeRuntime,
    StopRuntime,
    StartRegularTunnel,
    StopRegularTunnel,
    StopQuickShare,
    CancelOperation(String),
}

fn spawn_state_action(app: &AppHandle, action: TrayStateAction) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = match action {
            TrayStateAction::ResumeRuntime => state.resume_saved_runtime().await,
            TrayStateAction::StopRuntime => state.stop_local_runtime().await,
            TrayStateAction::StartRegularTunnel => state.start_regular_tunnel().await,
            TrayStateAction::StopRegularTunnel => state.stop_regular_tunnel().await,
            TrayStateAction::StopQuickShare => state.stop_quick_share().await,
            // Keep the operation observed by the menu, even if a newer operation
            // starts before this task runs. AppState rejects stale IDs.
            TrayStateAction::CancelOperation(operation_id) => state.cancel_operation(&operation_id),
        };
        match result {
            Ok(snapshot) => refresh_from_snapshot(&app, &snapshot),
            Err(error) => {
                eprintln!("CodexLink tray action failed: {}", error.code);
                let snapshot = state.get_state();
                refresh_from_snapshot(&app, &snapshot);
                let _ = desktop_shell::navigate(&app, NavigationTarget::Activity);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ChatGptActivitySnapshot, DesktopOperationKind, DesktopOperationSnapshot, Exposure,
        RegularTunnelState, RegularTunnelStatus, RunnerTopology, RuntimeTopology,
    };

    fn local_snapshot() -> DesktopStateSnapshot {
        let mut snapshot = DesktopStateSnapshot::default();
        snapshot.license.status = crate::models::LicenseStatus::Activated;
        snapshot.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: crate::models::Enrollment::ManagedPairing,
        });
        snapshot.openai_tunnel_configured = true;
        snapshot.regular_tunnel_available = true;
        snapshot
    }

    #[test]
    fn tray_text_follows_supported_ui_locale_and_defaults_to_chinese() {
        assert_eq!(tray_texts("zh-CN").open, "打开 CodexLink");
        assert_eq!(tray_texts("en-US").quit, "Quit CodexLink");
        assert_eq!(tray_texts("unsupported").open, "打开 CodexLink");
    }

    #[test]
    fn unactivated_tray_never_offers_start_actions_but_keeps_cleanup_actions() {
        let mut snapshot = local_snapshot();
        snapshot.license.status = crate::models::LicenseStatus::NotActivated;
        snapshot.readiness.server = ServerReadiness::Stopped;
        snapshot.readiness.runner = RunnerReadiness::Stopped;
        let stopped = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_eq!(stopped.runtime_action, None);
        assert_eq!(stopped.connection_action, None);

        snapshot.readiness.server = ServerReadiness::Ready;
        snapshot.readiness.runner = RunnerReadiness::Ready;
        snapshot.readiness.runtime_ready = true;
        snapshot.regular_tunnel = Some(RegularTunnelState {
            provider: "openai".into(),
            status: RegularTunnelStatus::Ready,
            clipboard_state: "copied".into(),
            clipboard_contains: "tunnel_id".into(),
            ready_for_chatgpt: true,
        });
        let running = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_eq!(running.runtime_action, Some(RuntimeAction::Stop));
        assert_eq!(
            running.connection_action,
            Some(ConnectionAction::Disconnect)
        );
    }

    #[test]
    fn stopped_runtime_projects_resume_action() {
        let mut snapshot = local_snapshot();
        snapshot.readiness.server = ServerReadiness::Stopped;
        snapshot.readiness.runner = RunnerReadiness::Stopped;
        let projection = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_eq!(projection.runtime_status, RuntimeStatus::Stopped);
        assert_eq!(projection.runtime_action, Some(RuntimeAction::Resume));
    }

    #[test]
    fn ready_runtime_projects_stop_and_connect_actions() {
        let mut snapshot = local_snapshot();
        snapshot.readiness.server = ServerReadiness::Ready;
        snapshot.readiness.runner = RunnerReadiness::Ready;
        snapshot.readiness.runtime_ready = true;
        let projection = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_eq!(projection.runtime_status, RuntimeStatus::Ready);
        assert_eq!(projection.runtime_action, Some(RuntimeAction::Stop));
        assert_eq!(projection.connection_status, ConnectionStatus::NotConnected);
        assert_eq!(
            projection.connection_action,
            Some(ConnectionAction::Connect)
        );
    }

    #[test]
    fn observed_chatgpt_project_use_is_distinct_from_desktop_tunnel_state() {
        let mut snapshot = local_snapshot();
        snapshot.readiness.server = ServerReadiness::Ready;
        snapshot.readiness.runner = RunnerReadiness::Ready;
        snapshot.readiness.runtime_ready = true;
        snapshot.chatgpt_activity = Some(ChatGptActivitySnapshot {
            observed: true,
            last_meaningful_activity_at_ms: Some(1234),
        });
        let projection = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_eq!(projection.connection_status, ConnectionStatus::ObservedUse);
        assert_eq!(
            projection.connection_action,
            Some(ConnectionAction::Connect),
            "observed use must not pretend Desktop owns a tunnel"
        );
    }

    #[test]
    fn observed_chatgpt_use_wins_over_waiting_tunnel_without_claiming_tunnel_ownership() {
        let mut snapshot = local_snapshot();
        snapshot.readiness.server = ServerReadiness::Ready;
        snapshot.readiness.runner = RunnerReadiness::Ready;
        snapshot.readiness.runtime_ready = true;
        snapshot.regular_tunnel = Some(RegularTunnelState {
            provider: "openai".to_string(),
            status: RegularTunnelStatus::Ready,
            clipboard_state: "copied".to_string(),
            clipboard_contains: "tunnel_id".to_string(),
            ready_for_chatgpt: true,
        });
        snapshot.chatgpt_activity = Some(ChatGptActivitySnapshot {
            observed: true,
            last_meaningful_activity_at_ms: Some(1234),
        });
        let projection = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_eq!(projection.connection_status, ConnectionStatus::ObservedUse);
        assert_eq!(
            projection.connection_action,
            Some(ConnectionAction::Disconnect)
        );
    }

    #[test]
    fn unconfigured_tunnel_does_not_offer_connect_action() {
        let mut snapshot = local_snapshot();
        snapshot.readiness.server = ServerReadiness::Ready;
        snapshot.readiness.runner = RunnerReadiness::Ready;
        snapshot.readiness.runtime_ready = true;
        snapshot.openai_tunnel_configured = false;
        let projection = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_eq!(projection.connection_status, ConnectionStatus::NotConnected);
        assert_eq!(projection.connection_action, None);
    }

    #[test]
    fn degraded_runtime_projects_needs_attention() {
        let mut snapshot = local_snapshot();
        snapshot.readiness.server = ServerReadiness::Ready;
        snapshot.readiness.runner = RunnerReadiness::Connecting;
        let projection = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_eq!(projection.runtime_status, RuntimeStatus::NeedsAttention);
    }

    #[test]
    fn locally_ready_tunnel_waits_for_chatgpt_and_offers_stop_action() {
        let mut snapshot = local_snapshot();
        snapshot.regular_tunnel = Some(RegularTunnelState {
            provider: "openai".into(),
            status: RegularTunnelStatus::Ready,
            clipboard_state: "copied".into(),
            clipboard_contains: "tunnel_id".into(),
            ready_for_chatgpt: true,
        });
        snapshot.readiness.ready_for_chatgpt = false;
        let projection = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_eq!(
            projection.connection_status,
            ConnectionStatus::WaitingForChatGpt
        );
        assert_eq!(
            projection.connection_action,
            Some(ConnectionAction::Disconnect)
        );
    }

    #[test]
    fn active_quick_share_and_cancellable_operation_project_actions() {
        let mut snapshot = local_snapshot();
        snapshot.quick_share = Some(crate::models::QuickShareState {
            provider: "cloudflare".into(),
            project: "/tmp/project".into(),
            mcp_url: None,
            clipboard_state: "copied".into(),
            clipboard_contains: "bearer_credential".into(),
            ready_for_chatgpt: true,
        });
        snapshot.current_operation = Some(DesktopOperationSnapshot {
            id: "operation-a".into(),
            kind: DesktopOperationKind::RuntimeResume,
            phase: DesktopOperationPhase::Running,
            started_at_ms: 1,
            cancellable: true,
        });
        let projection = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert!(projection.stop_quick_share);
        assert_eq!(
            projection.cancel_operation_id.as_deref(),
            Some("operation-a")
        );
        assert!(projection.cancel_operation_enabled);
        assert!(projection.operation_busy);
    }

    #[test]
    fn cancel_menu_preserves_observed_operation_identity() {
        let mut snapshot = local_snapshot();
        snapshot.current_operation = Some(DesktopOperationSnapshot {
            id: "operation-a".into(),
            kind: DesktopOperationKind::RuntimeResume,
            phase: DesktopOperationPhase::Running,
            started_at_ms: 1,
            cancellable: true,
        });
        let first = TrayProjection::from_snapshot(&snapshot, Some(false));
        let menu_id = format!(
            "{CANCEL_OPERATION_PREFIX}{}",
            first.cancel_operation_id.as_ref().unwrap()
        );
        snapshot.current_operation.as_mut().unwrap().id = "operation-b".into();
        let second = TrayProjection::from_snapshot(&snapshot, Some(false));
        assert_ne!(
            first, second,
            "a new operation must invalidate the menu cache"
        );
        assert_eq!(
            cancel_action_from_menu_id(&menu_id),
            Some(TrayStateAction::CancelOperation("operation-a".into()))
        );
        assert_eq!(cancel_action_from_menu_id(CANCEL_OPERATION_PREFIX), None);
        assert_eq!(cancel_action_from_menu_id(QUIT_ID), None);
    }

    #[test]
    fn autostart_projection_tracks_authoritative_os_observation() {
        let snapshot = local_snapshot();
        assert_eq!(
            TrayProjection::from_snapshot(&snapshot, Some(false)).launch_at_login,
            Some(false)
        );
        assert_eq!(
            TrayProjection::from_snapshot(&snapshot, Some(true)).launch_at_login,
            Some(true)
        );
        assert_eq!(
            TrayProjection::from_snapshot(&snapshot, None).launch_at_login,
            None
        );
    }
}
