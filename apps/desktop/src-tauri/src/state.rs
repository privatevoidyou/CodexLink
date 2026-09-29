use crate::activity::{ActivityEventKind, ActivityLevel, ActivityLog};
use crate::deadline::Deadline;
use crate::error::{DesktopError, DesktopResult};
use crate::license::{LicenseManager, RemoteLicenseState};
use crate::models::{
    aggregate_readiness, ChatGptActivitySnapshot, DesktopOperationKind, DesktopStateSnapshot,
    Enrollment, Experience, Exposure, ExposureReadiness, LicenseSnapshot, ProjectReadiness,
    ProjectSelection, QuickShareState, ReadinessNextActionKind, ReadinessSummaryKind,
    RegularConnectionPreference, RegularTunnelState, RegularTunnelStatus, RunnerReadiness,
    RunnerTopology, RuntimeTopology, ServerReadiness, ServerTopology, StoredDesktopConfig,
    StoredRuntime, TunnelProxyConfig, TunnelProxyMode, TunnelProxySnapshot,
};
use crate::operation::{
    cancelled_error, CancellationContext, CancellationSignal, OperationAdmission,
    OperationController,
};
use crate::process::{
    MachineEventReceiver, OwnedProcessGeneration, ProcessKind, ProcessPhase, ProcessSupervisor,
};
use crate::tunnel_config::{
    TunnelConfig, TunnelConfigRequest, LEGACY_TUNNEL_CONFIG_FILE_NAME, TUNNEL_KEY_FILE_NAME,
};
use crate::webcodex::{
    inspect_project_path, ProjectRuntimeIdentity, QuickShareReadyEvent, RegularTunnelReadyEvent,
    RunnerConnectionState, RunnerRuntimeIdentity, WebCodexAdapter,
};
use serde_json::Value;
#[cfg(unix)]
use std::fs::File;
use std::fs::OpenOptions;
use std::future::Future;
use std::io::{self, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Mutex;

const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(20);
const RUNNER_READY_TIMEOUT: Duration = Duration::from_secs(30);
const PROJECT_READY_TIMEOUT: Duration = Duration::from_secs(20);
const QUICK_SHARE_READY_TIMEOUT: Duration = Duration::from_secs(90);
const REGULAR_TUNNEL_READY_TIMEOUT: Duration = Duration::from_secs(90);
const TUNNEL_NETWORK_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const OPENAI_CONTROL_PLANE_PROBE_URL: &str = "https://api.openai.com/v1/models";
const POLL_INTERVAL: Duration = Duration::from_millis(300);
const READINESS_CLEANUP_SLACK: Duration = Duration::from_secs(2);
const SHUTDOWN_OPERATION_WAIT: Duration = Duration::from_secs(5);
const DESKTOP_STATE_MAX_BYTES: u64 = 256 * 1024;
const DESKTOP_SERVER_ENV_MAX_BYTES: u64 = 256 * 1024;
const DESKTOP_MCP_MODEL_SURFACE: &str = "adaptive-runtime-v1";
const DESKTOP_MCP_COMPACT_SCHEMAS: &str = "true";
static NEXT_STATE_TEMP_ID: AtomicU64 = AtomicU64::new(1);

type SharedSupervisor = Arc<Mutex<ProcessSupervisor>>;

#[derive(Debug, Clone)]
struct ChatGptActivityProbe {
    identity: ProjectRuntimeIdentity,
    webcodex: PathBuf,
}

pub struct AppState {
    core: Mutex<Option<DesktopCore>>,
    published: Arc<RwLock<DesktopStateSnapshot>>,
    supervisor: SharedSupervisor,
    activity: ActivityLog,
    operations: OperationController,
    shutdown_signal: CancellationSignal,
    shutdown_started: AtomicBool,
    license: Mutex<LicenseManager>,
    license_snapshot: Arc<RwLock<LicenseSnapshot>>,
    license_check: tokio::sync::OnceCell<bool>,
}

impl AppState {
    pub fn new(data_dir: PathBuf, resource_dir: PathBuf) -> DesktopResult<Self> {
        let license = LicenseManager::load(&data_dir);
        let license_snapshot = Arc::new(RwLock::new(license.snapshot()));
        let core = DesktopCore::new(data_dir, resource_dir)?;
        let published = Arc::clone(&core.published);
        let supervisor = Arc::clone(&core.supervisor);
        let activity = core.activity.clone();
        Ok(Self {
            core: Mutex::new(Some(core)),
            published,
            supervisor,
            operations: OperationController::new(activity.clone()),
            activity,
            shutdown_signal: CancellationSignal::new(),
            shutdown_started: AtomicBool::new(false),
            license: Mutex::new(license),
            license_snapshot,
            license_check: tokio::sync::OnceCell::new(),
        })
    }

    pub fn get_state(&self) -> DesktopStateSnapshot {
        let mut snapshot = self
            .published
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Ok(mut supervisor) = self.supervisor.try_lock() {
            let managed_local_runtime = snapshot.runtime_autostart
                && snapshot.topology.as_ref().is_some_and(|topology| {
                    topology.experience == Experience::Full
                        && matches!(topology.server, ServerTopology::Local)
                });
            if managed_local_runtime {
                let server_stopped =
                    supervisor
                        .snapshot(ProcessKind::LocalServer)
                        .is_some_and(|process| {
                            matches!(process.phase, ProcessPhase::Exited | ProcessPhase::Failed)
                        });
                let runner_stopped =
                    supervisor
                        .snapshot(ProcessKind::LocalRunner)
                        .is_some_and(|process| {
                            matches!(process.phase, ProcessPhase::Exited | ProcessPhase::Failed)
                        });
                if server_stopped || runner_stopped {
                    snapshot.readiness = aggregate_readiness(
                        if server_stopped {
                            ServerReadiness::Stopped
                        } else {
                            snapshot.readiness.server.clone()
                        },
                        if runner_stopped {
                            RunnerReadiness::Stopped
                        } else {
                            snapshot.readiness.runner.clone()
                        },
                        snapshot.readiness.exposure.clone(),
                        snapshot.readiness.project.clone(),
                    );
                }
            }

            if snapshot.regular_tunnel.is_some() {
                let active =
                    supervisor
                        .snapshot(ProcessKind::RegularTunnel)
                        .is_some_and(|process| {
                            matches!(
                                process.phase,
                                ProcessPhase::Starting | ProcessPhase::Running
                            )
                        });
                if let Some(exposure) =
                    regular_tunnel_exposure(&mut snapshot.regular_tunnel, active)
                {
                    snapshot.readiness = aggregate_readiness(
                        snapshot.readiness.server.clone(),
                        snapshot.readiness.runner.clone(),
                        exposure.clone(),
                        snapshot.readiness.project.clone(),
                    );
                    apply_regular_tunnel_next_action(&mut snapshot, &exposure);
                }
            }
        }
        snapshot.current_operation = self.operations.current();
        snapshot.activity_sequence = self.activity.latest_sequence();
        snapshot.license = self
            .license_snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if snapshot.openai_tunnel_config.source == crate::models::TunnelConfigSource::Environment {
            snapshot.openai_tunnel_config = crate::tunnel_config::environment_snapshot();
            snapshot.openai_tunnel_configured = snapshot.openai_tunnel_config.is_configured();
        }
        snapshot.regular_tunnel_available = true;
        snapshot.powershell_runtime = crate::platform::powershell_runtime_snapshot();
        snapshot
    }

    pub async fn activate_license(
        &self,
        activation_code: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        if self.shutdown_signal.is_cancelled() {
            return Err(cancelled_error());
        }
        let snapshot = {
            let mut license = self.license.lock().await;
            license.activate(activation_code).await?
        };
        self.license_snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone_from(&snapshot);
        Ok(self.get_state())
    }

    /// Asks the license service once per launch whether a refund revoked the
    /// saved license; concurrent callers share that one check. Returns whether
    /// it revoked the license. Any failure keeps the license working.
    pub async fn check_license_status(&self) -> bool {
        *self
            .license_check
            .get_or_init(|| self.run_license_status_check())
            .await
    }

    async fn run_license_status_check(&self) -> bool {
        let Some(binding) = self.license.lock().await.status_binding() else {
            return false;
        };
        if crate::license::fetch_license_status(&binding).await != Some(RemoteLicenseState::Revoked)
        {
            return false;
        }
        let snapshot = self.license.lock().await.revoke(&binding);
        let revoked = !snapshot.is_active();
        self.license_snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone_from(&snapshot);
        revoked
    }

    pub fn activity(&self) -> Vec<crate::activity::ActivityEntry> {
        self.activity.snapshot()
    }

    pub async fn inspect_project(&self, path: &str) -> DesktopResult<ProjectSelection> {
        inspect_project_path(path).await
    }

    pub async fn refresh_runtime_status(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RuntimeRefresh, true)
            .await?;
        let result = core.refresh_runtime_status(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn observe_chatgpt_activity(&self) -> DesktopResult<DesktopStateSnapshot> {
        if self.shutdown_signal.is_cancelled() {
            return Err(cancelled_error());
        }
        if self.operations.current().is_some() {
            return Ok(self.get_state());
        }
        let cancellation =
            CancellationContext::new(CancellationSignal::new(), self.shutdown_signal.clone());
        let probe = {
            let slot = self.core.lock().await;
            let Some(core) = slot.as_ref() else {
                return Ok(self.get_state());
            };
            let Some(probe) = core.chatgpt_activity_probe() else {
                return Ok(self.get_state());
            };
            probe
        };
        let observation = WebCodexAdapter::chatgpt_activity_with_binary(
            &probe.webcodex,
            &probe.identity,
            &cancellation,
        )
        .await;
        cancellation.check()?;
        let Ok(last_meaningful_activity_at_ms) = observation else {
            return Ok(self.get_state());
        };
        if self.operations.current().is_some() {
            return Ok(self.get_state());
        }
        let mut slot = self.core.lock().await;
        let Some(core) = slot.as_mut() else {
            return Ok(self.get_state());
        };
        core.apply_chatgpt_activity_observation(&probe.identity, last_meaningful_activity_at_ms)
    }

    pub async fn resume_saved_runtime(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RuntimeResume, true)
            .await?;
        let result = core.resume_saved_runtime(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn update_tunnel_config(
        &self,
        request: TunnelConfigRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::TunnelConfigUpdate, false)
            .await?;
        let result = async {
            cancellation.check()?;
            let key_path = core.data_dir.join("secrets").join(TUNNEL_KEY_FILE_NAME);
            let mut tunnel_config = core.tunnel_config.clone();
            let (tunnel_config, tunnel_id) = tokio::task::spawn_blocking(move || {
                let tunnel_id = tunnel_config.update(&key_path, request)?;
                Ok::<_, DesktopError>((tunnel_config, tunnel_id))
            })
            .await
            .map_err(|_| {
                DesktopError::new(
                    "tunnel_config_save_failed",
                    "Could not complete the configuration save",
                    "Check the saved configuration before retrying.",
                )
            })??;
            core.tunnel_config = tunnel_config;
            core.config.openai_tunnel_id = tunnel_id;
            core.save_config().await?;
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn update_tunnel_proxy(
        &self,
        mode: TunnelProxyMode,
        custom_url: Option<&str>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::TunnelProxyUpdate, false)
            .await?;
        let result = core
            .update_tunnel_proxy(mode, custom_url, &cancellation)
            .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn configure_local_setup(
        &self,
        project_path: Option<&str>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::LocalSetup, true)
            .await?;
        let result = core
            .configure_local_setup(project_path, &cancellation)
            .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn activate_local_project(
        &self,
        project_path: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::LocalProjectActivate, true)
            .await?;
        let result = core
            .activate_local_project(project_path, &cancellation)
            .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn remove_local_project(
        &self,
        project_path: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::LocalProjectRemove, true)
            .await?;
        let result = core.remove_local_project(project_path, &cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn configure_remote_setup(
        &self,
        server_url: &str,
        pairing_code: &str,
        project_path: Option<&str>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RemoteSetup, true)
            .await?;
        let result = core
            .configure_remote_setup(server_url, pairing_code, project_path, &cancellation)
            .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn start_quick_share(
        &self,
        project_path: &str,
        provider: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::QuickShareStart, true)
            .await?;
        let result = core
            .start_quick_share(project_path, provider, &cancellation)
            .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn stop_quick_share(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::QuickShareStop, false)
            .await?;
        let result = core.stop_quick_share(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn start_regular_tunnel(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RegularTunnelStart, true)
            .await?;
        let result = core.start_regular_tunnel(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn stop_regular_tunnel(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RegularTunnelStop, false)
            .await?;
        let result = core.stop_regular_tunnel(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn stop_local_runtime(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::LocalRuntimeStop, false)
            .await?;
        let result = core.stop_local_runtime(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub fn cancel_operation(&self, operation_id: &str) -> DesktopResult<DesktopStateSnapshot> {
        self.operations.cancel(operation_id)?;
        Ok(self.get_state())
    }

    pub async fn shutdown(&self) {
        if self.shutdown_started.swap(true, Ordering::SeqCst) {
            return;
        }
        self.shutdown_signal.cancel();
        self.operations.cancel_active_for_shutdown();
        self.supervisor.lock().await.stop_all().await;
        let _ = self
            .operations
            .wait_until_idle(tokio::time::Instant::now() + SHUTDOWN_OPERATION_WAIT)
            .await;
    }

    async fn begin_operation(
        &self,
        kind: DesktopOperationKind,
        cancellable: bool,
    ) -> DesktopResult<(
        OperationAdmission,
        CancellationContext,
        DesktopCore,
        ProcessBaseline,
    )> {
        if self.shutdown_signal.is_cancelled() {
            return Err(cancelled_error());
        }
        if operation_requires_license(kind) {
            // Nothing licensed starts before this launch's status check settles.
            self.check_license_status().await;
            if !self
                .license_snapshot
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_active()
            {
                return Err(crate::license::license_required_error());
            }
        }
        let operation = self.operations.admit(kind, cancellable)?;
        let cancellation =
            CancellationContext::new(operation.cancellation.clone(), self.shutdown_signal.clone());
        let baseline = self.capture_process_baseline().await;
        let core = {
            let mut slot = self.core.lock().await;
            slot.take()
        };
        let Some(core) = core else {
            let error = DesktopError::new(
                "desktop_operation_busy",
                "Desktop mutation state is already in use",
                "Wait for the current operation to finish.",
            );
            let result: DesktopResult<()> = Err(error.clone());
            self.operations.finish(&operation.id, &result);
            return Err(error);
        };
        Ok((operation, cancellation, core, baseline))
    }

    async fn finish_operation(
        &self,
        operation: OperationAdmission,
        cancellation: CancellationContext,
        mut core: DesktopCore,
        baseline: ProcessBaseline,
        mut result: DesktopResult<DesktopStateSnapshot>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        if result.is_ok() && cancellation.is_cancelled() {
            result = Err(cancelled_error());
        }
        if result.is_err() {
            let cancelled = result
                .as_ref()
                .err()
                .is_some_and(|error| error.code == "desktop_operation_cancelled");
            let cleanup = self.cleanup_new_owned_processes(&baseline).await;
            core.reconcile_after_operation_failure(operation.kind, &baseline, cleanup, cancelled);
            core.publish_snapshot();
        }
        {
            let mut slot = self.core.lock().await;
            *slot = Some(core);
        }
        self.operations.finish(&operation.id, &result);
        match result {
            Ok(_) => Ok(self.get_state()),
            Err(error) => Err(error),
        }
    }

    async fn capture_process_baseline(&self) -> ProcessBaseline {
        let snapshot = self
            .published
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let mut supervisor = self.supervisor.lock().await;
        ProcessBaseline {
            local_server: process_is_active(supervisor.snapshot(ProcessKind::LocalServer)),
            local_runner: supervisor.active_owned_generation(ProcessKind::LocalRunner),
            quick_share: process_is_active(supervisor.snapshot(ProcessKind::QuickShare)),
            regular_tunnel: process_is_active(supervisor.snapshot(ProcessKind::RegularTunnel)),
            snapshot,
        }
    }

    async fn cleanup_new_owned_processes(&self, baseline: &ProcessBaseline) -> ProcessCleanup {
        let mut supervisor = self.supervisor.lock().await;
        let mut cleanup = ProcessCleanup::default();
        for (kind, existed) in [
            (ProcessKind::QuickShare, baseline.quick_share),
            (ProcessKind::RegularTunnel, baseline.regular_tunnel),
        ] {
            if !existed && supervisor.snapshot(kind).is_some() {
                supervisor.stop(kind).await;
                cleanup.mark_stopped(kind);
            }
        }

        let current_runner = supervisor.owned_generation(ProcessKind::LocalRunner);
        let current_runner_active = process_is_active(supervisor.snapshot(ProcessKind::LocalRunner));
        let runner_generation_changed = baseline.local_runner != current_runner
            || (baseline.local_runner.is_some() && !current_runner_active);
        if runner_generation_changed {
            if current_runner.is_some() {
                // The slot still belongs to the Desktop, and the supervisor lock
                // keeps this comparison and stop scoped to that exact generation.
                supervisor.stop(ProcessKind::LocalRunner).await;
            }
            cleanup.mark_stopped(ProcessKind::LocalRunner);
        }

        if !baseline.local_server && supervisor.snapshot(ProcessKind::LocalServer).is_some() {
            supervisor.stop(ProcessKind::LocalServer).await;
            cleanup.mark_stopped(ProcessKind::LocalServer);
        }
        cleanup
    }

    #[cfg(test)]
    fn set_test_license_active(&self) {
        self.license_snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .status = crate::models::LicenseStatus::Activated;
    }

    #[cfg(test)]
    async fn hold_test_operation(
        &self,
        started: tokio::sync::oneshot::Sender<String>,
        cleanup_release: tokio::sync::oneshot::Receiver<()>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, core, baseline) = self
            .begin_operation(DesktopOperationKind::LocalSetup, true)
            .await?;
        let _ = started.send(operation.id.clone());
        cancellation.cancelled().await;
        let _ = cleanup_release.await;
        let result: DesktopResult<DesktopStateSnapshot> = Err(cancelled_error());
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    #[cfg(test)]
    async fn run_test_one_shot_operation(
        &self,
        executable: PathBuf,
        args: Vec<String>,
        payload: Vec<u8>,
        timeout: Duration,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RuntimeRefresh, true)
            .await?;
        let result = crate::webcodex::run_test_bounded(
            &executable,
            &args,
            Some(&payload),
            &cancellation,
            timeout,
        )
        .await
        .map(|_| core.publish_snapshot());
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}

fn operation_requires_license(kind: DesktopOperationKind) -> bool {
    matches!(
        kind,
        DesktopOperationKind::LocalSetup
            | DesktopOperationKind::LocalProjectActivate
            | DesktopOperationKind::LocalProjectRemove
            | DesktopOperationKind::RemoteSetup
            | DesktopOperationKind::QuickShareStart
            | DesktopOperationKind::RegularTunnelStart
            | DesktopOperationKind::RuntimeResume
    )
}

#[derive(Clone)]
struct ProcessBaseline {
    local_server: bool,
    local_runner: Option<OwnedProcessGeneration>,
    quick_share: bool,
    regular_tunnel: bool,
    snapshot: DesktopStateSnapshot,
}

#[derive(Clone, Copy, Default)]
struct ProcessCleanup {
    local_server: bool,
    local_runner: bool,
    quick_share: bool,
    regular_tunnel: bool,
}

impl ProcessCleanup {
    fn mark_stopped(&mut self, kind: ProcessKind) {
        match kind {
            ProcessKind::LocalServer => self.local_server = true,
            ProcessKind::LocalRunner => self.local_runner = true,
            ProcessKind::QuickShare => self.quick_share = true,
            ProcessKind::RegularTunnel => self.regular_tunnel = true,
        }
    }
}

fn project_not_loaded_error() -> DesktopError {
    readiness_timeout_error(
        "project_not_loaded",
        "The selected project did not become ready on the current Runner",
        "Retry project activation after checking Runner and project diagnostics.",
    )
}

fn process_is_active(snapshot: Option<crate::process::ProcessSnapshot>) -> bool {
    snapshot.is_some_and(|process| {
        matches!(
            process.phase,
            ProcessPhase::Starting | ProcessPhase::Running | ProcessPhase::Stopping
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OwnedRunnerPreparation {
    target_already_active: bool,
    replaced: bool,
}

fn runner_process_target_key(identity: &RunnerRuntimeIdentity) -> String {
    let runner_config = identity
        .runner_config
        .canonicalize()
        .unwrap_or_else(|_| identity.runner_config.clone());
    let mut runner_config = runner_config.to_string_lossy().into_owned();
    if cfg!(windows) {
        runner_config = runner_config.replace('/', "\\").to_ascii_lowercase();
    }
    format!(
        "{}\n{}",
        identity.server_url.trim_end_matches('/').to_ascii_lowercase(),
        runner_config
    )
}

fn can_refresh_legacy_runner(snapshot: Option<crate::process::ProcessSnapshot>) -> bool {
    snapshot.is_some_and(|process| {
        process.owned_by_desktop
            && matches!(
                process.phase,
                ProcessPhase::Starting | ProcessPhase::Running | ProcessPhase::Stopping
            )
    })
}

pub struct DesktopCore {
    data_dir: PathBuf,
    default_project_dir: PathBuf,
    config_path: PathBuf,
    config: StoredDesktopConfig,
    tunnel_config: TunnelConfig,
    resolved_tunnel_proxy: Option<EffectiveTunnelProxy>,
    snapshot: DesktopStateSnapshot,
    adapter: WebCodexAdapter,
    supervisor: SharedSupervisor,
    activity: ActivityLog,
    published: Arc<RwLock<DesktopStateSnapshot>>,
}

impl DesktopCore {
    fn new(data_dir: PathBuf, resource_dir: PathBuf) -> DesktopResult<Self> {
        let activity = ActivityLog::default();
        let default_project_dir = default_management_project_dir(&data_dir, &resource_dir);
        let config_path = data_dir.join("desktop-state.json");
        let mut config = load_config(&config_path, &activity)?;
        migrate_legacy_authorized_projects(&mut config, &default_project_dir, &resource_dir);
        let secrets_dir = data_dir.join("secrets");
        let tunnel_config = TunnelConfig::load(
            config.openai_tunnel_id.clone(),
            &secrets_dir.join(TUNNEL_KEY_FILE_NAME),
            &secrets_dir.join(LEGACY_TUNNEL_CONFIG_FILE_NAME),
        );
        let mut snapshot = DesktopStateSnapshot::default();
        snapshot.topology = config.topology.clone();
        snapshot.project = project_snapshot(&config);
        snapshot.projects = projects_snapshot(&config);
        if config.topology.is_some() && config.runtime_autostart == Some(false) {
            snapshot.readiness = aggregate_readiness(
                ServerReadiness::Stopped,
                RunnerReadiness::Stopped,
                ExposureReadiness::Disabled,
                if runtime_identity_project(&config).is_some() {
                    ProjectReadiness::Configured
                } else {
                    ProjectReadiness::None
                },
            );
        }
        apply_openai_tunnel_configuration(&mut snapshot, &tunnel_config);
        snapshot.regular_tunnel_available = true;
        snapshot.powershell_runtime = crate::platform::powershell_runtime_snapshot();
        apply_config_projection(&mut snapshot, &config);
        let published = Arc::new(RwLock::new(snapshot.clone()));
        let supervisor = Arc::new(Mutex::new(ProcessSupervisor::new(activity.clone())));
        Ok(Self {
            data_dir,
            default_project_dir,
            config_path,
            config,
            tunnel_config,
            snapshot,
            resolved_tunnel_proxy: None,
            adapter: WebCodexAdapter::new(Some(resource_dir.join(if cfg!(windows) {
                "codexlink-core"
            } else {
                "webcodex-runtime"
            }))),
            supervisor,
            activity,
            published,
        })
    }

    pub async fn get_state(&mut self) -> DesktopResult<DesktopStateSnapshot> {
        apply_openai_tunnel_configuration(&mut self.snapshot, &self.tunnel_config);
        self.snapshot.regular_tunnel_available = true;
        apply_config_projection(&mut self.snapshot, &self.config);
        self.snapshot.project = project_snapshot(&self.config);
        self.snapshot.projects = projects_snapshot(&self.config);
        if let Some(proxy) = self.resolved_tunnel_proxy.as_ref() {
            apply_resolved_tunnel_proxy_projection(&mut self.snapshot, &self.config, proxy);
        }
        if self.snapshot.regular_tunnel.is_some() {
            let active = self
                .process_snapshot(ProcessKind::RegularTunnel)
                .await
                .is_some_and(|process| {
                    matches!(
                        process.phase,
                        ProcessPhase::Starting | ProcessPhase::Running
                    )
                });
            if let Some(exposure) =
                regular_tunnel_exposure(&mut self.snapshot.regular_tunnel, active)
            {
                self.snapshot.readiness = aggregate_readiness(
                    self.snapshot.readiness.server.clone(),
                    self.snapshot.readiness.runner.clone(),
                    exposure.clone(),
                    self.snapshot.readiness.project.clone(),
                );
                apply_regular_tunnel_next_action(&mut self.snapshot, &exposure);
                self.snapshot.topology = effective_topology(
                    self.config.topology.as_ref(),
                    self.snapshot.regular_tunnel.is_some(),
                );
            }
        }
        Ok(self.publish_snapshot())
    }

    fn chatgpt_activity_probe(&self) -> Option<ChatGptActivityProbe> {
        if !self.snapshot.readiness.runtime_ready
            || self
                .snapshot
                .chatgpt_activity
                .as_ref()
                .is_some_and(|activity| activity.observed)
        {
            return None;
        }
        let identity = customer_project_identity_from_config(&self.config)?;
        let webcodex = self.adapter.binaries().ok()?.webcodex.clone();
        Some(ChatGptActivityProbe { identity, webcodex })
    }

    fn apply_chatgpt_activity_observation(
        &mut self,
        expected_identity: &ProjectRuntimeIdentity,
        last_meaningful_activity_at_ms: Option<i64>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        if !self.snapshot.readiness.runtime_ready
            || customer_project_identity_from_config(&self.config).as_ref() != Some(expected_identity)
            || self
                .snapshot
                .chatgpt_activity
                .as_ref()
                .is_some_and(|activity| activity.observed)
        {
            return Ok(self.publish_snapshot());
        }
        self.snapshot.chatgpt_activity = Some(ChatGptActivitySnapshot {
            observed: last_meaningful_activity_at_ms.is_some(),
            last_meaningful_activity_at_ms,
        });
        Ok(self.publish_snapshot())
    }

    fn stage_project_scope(&mut self, project: ProjectSelection) {
        self.snapshot.project = Some(project);
        // ChatGPT activity is evidence for one exact runtime Project. Never carry
        // an observation from the previously displayed Project into a new setup
        // or Quick Share scope; the new Project must earn its own observation.
        self.snapshot.chatgpt_activity = None;
    }

    pub async fn refresh_runtime_status(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        if self.snapshot.quick_share.is_some() {
            self.snapshot.chatgpt_activity = None;
            let active = self
                .process_snapshot(ProcessKind::QuickShare)
                .await
                .is_some_and(|process| {
                    matches!(
                        process.phase,
                        ProcessPhase::Starting | ProcessPhase::Running
                    )
                });
            if !active {
                self.snapshot.readiness = aggregate_readiness(
                    ServerReadiness::Stopped,
                    RunnerReadiness::Stopped,
                    ExposureReadiness::Error,
                    ProjectReadiness::Configured,
                );
                self.snapshot.readiness.summary_kind = ReadinessSummaryKind::QuickShareStopped;
                self.snapshot.readiness.next_action_kind =
                    Some(ReadinessNextActionKind::RestartQuickShare);
                self.snapshot.readiness.summary = "Quick Share stopped".to_string();
                self.snapshot.readiness.next_action = Some("Start Quick Share again.".to_string());
            }
            return self.get_state().await;
        }

        let Some(identity) = runner_identity_from_config(&self.config) else {
            self.snapshot.chatgpt_activity = None;
            self.snapshot.topology = self.config.topology.clone();
            self.snapshot.project = project_snapshot(&self.config);
            return self.get_state().await;
        };
        let project_identity = customer_project_identity_from_config(&self.config);
        let remote_topology = self.config.topology.as_ref().is_some_and(|topology| {
            topology.experience == Experience::Full
                && matches!(topology.server, ServerTopology::Remote { .. })
        });
        self.adapter.ensure_binaries(cancellation).await.ok();
        cancellation.check()?;
        if let Ok(binaries) = self.adapter.binaries() {
            self.snapshot.binaries = Some(binaries.info());
        }
        let server = match self
            .adapter
            .server_status(
                Some(&identity.server_url),
                self.config
                    .runtime
                    .as_ref()
                    .and_then(|runtime| runtime.server_env_file.as_deref()),
                Some(&identity.user_token_file),
                cancellation,
            )
            .await
        {
            Ok(status) if status.http_reachable => {
                if remote_topology && !identity.server_url.starts_with("https://") {
                    ServerReadiness::Degraded
                } else {
                    ServerReadiness::Ready
                }
            }
            Ok(_) => ServerReadiness::Offline,
            Err(error) if error.code == "binary_version_mismatch" => return Err(error),
            Err(_) => ServerReadiness::Unknown,
        };
        cancellation.check()?;
        // A saved local user stop remains stopped across refresh. Remote Server
        // liveness is always observed independently because it is externally owned.
        if !remote_topology
            && !runtime_autostart(&self.config)
            && !matches!(server, ServerReadiness::Ready | ServerReadiness::Degraded)
        {
            self.snapshot.chatgpt_activity = None;
            self.snapshot.readiness = aggregate_readiness(
                ServerReadiness::Stopped,
                RunnerReadiness::Stopped,
                ExposureReadiness::Disabled,
                if project_identity.is_some() {
                    ProjectReadiness::Configured
                } else {
                    ProjectReadiness::None
                },
            );
            return self.get_state().await;
        }
        let runner = match self
            .adapter
            .observe_runner_identity(
                &identity,
                stored_runner_client_id(&self.config).as_deref(),
                cancellation,
            )
            .await
        {
            Ok(observation) => match observation.state {
                RunnerConnectionState::Online => RunnerReadiness::Ready,
                RunnerConnectionState::RunnerOffline => RunnerReadiness::Error,
                RunnerConnectionState::ServerUnavailable | RunnerConnectionState::Unknown => {
                    RunnerReadiness::Unknown
                }
                RunnerConnectionState::CredentialInvalid => RunnerReadiness::Error,
            },
            Err(error) if error.code == "runner_identity_mismatch" => return Err(error),
            Err(_) => RunnerReadiness::Unknown,
        };
        cancellation.check()?;
        let project = if let Some(project_identity) = project_identity.as_ref() {
            match self.adapter.project_ready(project_identity, cancellation).await {
                Ok(true) => ProjectReadiness::Ready,
                Ok(false) => ProjectReadiness::ReloadRequired,
                Err(_) => ProjectReadiness::Unknown,
            }
        } else {
            ProjectReadiness::None
        };
        cancellation.check()?;
        self.snapshot.chatgpt_activity = if server == ServerReadiness::Ready
            && project == ProjectReadiness::Ready
        {
            if let Some(project_identity) = project_identity.as_ref() {
                match self.adapter.chatgpt_activity(project_identity, cancellation).await {
                    Ok(last_meaningful_activity_at_ms) => Some(ChatGptActivitySnapshot {
                        observed: last_meaningful_activity_at_ms.is_some(),
                        last_meaningful_activity_at_ms,
                    }),
                    Err(_) => None,
                }
            } else {
                None
            }
        } else {
            None
        };
        cancellation.check()?;

        let (exposure, regular_tunnel_expected) = if remote_topology {
            let exposure = match server {
                ServerReadiness::Ready => ExposureReadiness::RemoteReady,
                ServerReadiness::Degraded => ExposureReadiness::Degraded,
                ServerReadiness::Offline | ServerReadiness::Error => ExposureReadiness::Error,
                ServerReadiness::Stopped => ExposureReadiness::Disabled,
                ServerReadiness::Starting => ExposureReadiness::Starting,
                ServerReadiness::Unknown => ExposureReadiness::Unknown,
            };
            (exposure, false)
        } else {
            let tunnel_active = self
                .process_snapshot(ProcessKind::RegularTunnel)
                .await
                .is_some_and(|process| {
                    matches!(
                        process.phase,
                        ProcessPhase::Starting | ProcessPhase::Running
                    )
                });
            let expected = self.snapshot.regular_tunnel.is_some();
            let exposure = regular_tunnel_exposure(&mut self.snapshot.regular_tunnel, tunnel_active)
                .unwrap_or_else(|| exposure_readiness(self.config.topology.as_ref()));
            (exposure, expected)
        };
        self.snapshot.readiness = aggregate_readiness(server, runner, exposure.clone(), project);
        if regular_tunnel_expected && exposure == ExposureReadiness::Error {
            self.snapshot.readiness.next_action_kind =
                Some(ReadinessNextActionKind::RestartSecureTunnel);
            self.snapshot.readiness.next_action = Some("Restart the secure tunnel.".to_string());
        } else if regular_tunnel_expected && exposure == ExposureReadiness::Degraded {
            self.snapshot.readiness.next_action_kind =
                Some(ReadinessNextActionKind::RestoreClipboardHandoff);
            self.snapshot.readiness.next_action = Some(
                "Restore clipboard access, then restart the secure tunnel handoff.".to_string(),
            );
        }
        self.snapshot.topology = if remote_topology {
            self.config.topology.clone()
        } else {
            effective_topology(
                self.config.topology.as_ref(),
                self.snapshot.regular_tunnel.is_some(),
            )
        };
        self.snapshot.project = project_snapshot(&self.config);
        self.get_state().await
    }

    fn publish_snapshot(&mut self) -> DesktopStateSnapshot {
        self.snapshot.current_operation = None;
        self.snapshot.activity_sequence = self.activity.latest_sequence();
        apply_openai_tunnel_configuration(&mut self.snapshot, &self.tunnel_config);
        self.snapshot.regular_tunnel_available = true;
        self.snapshot.powershell_runtime = crate::platform::powershell_runtime_snapshot();
        apply_config_projection(&mut self.snapshot, &self.config);
        self.snapshot.project = project_snapshot(&self.config);
        self.snapshot.projects = projects_snapshot(&self.config);
        let snapshot = self.snapshot.clone();
        *self
            .published
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot.clone();
        snapshot
    }

    fn reconcile_after_operation_failure(
        &mut self,
        kind: DesktopOperationKind,
        baseline: &ProcessBaseline,
        cleanup: ProcessCleanup,
        cancelled: bool,
    ) {
        let observed_binaries = self.snapshot.binaries.clone();
        match kind {
            DesktopOperationKind::RuntimeResume => {
                // Resume is a desired-state replay of an already committed setup.
                // If any step fails or is cancelled, newly owned processes have
                // already been reclaimed above; restore the last published view
                // rather than leaving a synthetic "starting" state behind.
                self.snapshot = baseline.snapshot.clone();
            }
            DesktopOperationKind::QuickShareStart => {
                // Quick Share is intentionally ephemeral. Any failed start has
                // already stopped (or will have cleanup stop) the newly owned
                // foreground process, so return the public topology to the
                // last committed runtime instead of leaving "starting" behind.
                self.snapshot = baseline.snapshot.clone();
            }
            DesktopOperationKind::RegularTunnelStart if cancelled => {
                // User cancellation is not a tunnel failure. Restore the last
                // observed full-runtime state after the exact owned tunnel is
                // reclaimed rather than publishing a synthetic tunnel error.
                self.snapshot = baseline.snapshot.clone();
            }
            DesktopOperationKind::RuntimeRefresh if cancelled => {
                // A cancelled observation must not partially overwrite the
                // last published control-plane state.
                self.snapshot = baseline.snapshot.clone();
            }
            DesktopOperationKind::LocalSetup => {
                let server = if cleanup.local_server {
                    ServerReadiness::Stopped
                } else if self.snapshot.readiness.server == ServerReadiness::Starting {
                    baseline.snapshot.readiness.server.clone()
                } else {
                    self.snapshot.readiness.server.clone()
                };
                let runner = if cleanup.local_runner {
                    RunnerReadiness::Stopped
                } else if self.snapshot.readiness.runner == RunnerReadiness::Connecting {
                    baseline.snapshot.readiness.runner.clone()
                } else {
                    self.snapshot.readiness.runner.clone()
                };
                let project = if cleanup.local_server || cleanup.local_runner {
                    self.snapshot
                        .project
                        .as_ref()
                        .map(|_| ProjectReadiness::Configured)
                        .unwrap_or(ProjectReadiness::None)
                } else {
                    self.snapshot.readiness.project.clone()
                };
                self.snapshot.readiness = aggregate_readiness(
                    server,
                    runner,
                    self.snapshot.readiness.exposure.clone(),
                    project,
                );
            }
            DesktopOperationKind::RemoteSetup => {
                let runner = if cleanup.local_runner {
                    RunnerReadiness::Stopped
                } else if self.snapshot.readiness.runner == RunnerReadiness::Connecting {
                    baseline.snapshot.readiness.runner.clone()
                } else {
                    self.snapshot.readiness.runner.clone()
                };
                let project = if cleanup.local_runner {
                    self.snapshot
                        .project
                        .as_ref()
                        .map(|_| ProjectReadiness::Configured)
                        .unwrap_or(ProjectReadiness::None)
                } else {
                    self.snapshot.readiness.project.clone()
                };
                self.snapshot.readiness = aggregate_readiness(
                    self.snapshot.readiness.server.clone(),
                    runner,
                    self.snapshot.readiness.exposure.clone(),
                    project,
                );
            }
            _ => {}
        }
        if observed_binaries.is_some() {
            self.snapshot.binaries = observed_binaries;
        }
    }

    pub async fn resume_saved_runtime(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let Some(topology) = self.config.topology.clone() else {
            return self.configure_local_setup(None, cancellation).await;
        };
        if topology.experience != Experience::Full {
            return self.get_state().await;
        }
        match topology.server {
            ServerTopology::Local => self.configure_local_setup(None, cancellation).await,
            ServerTopology::Remote { url } => {
                let project_path = customer_authorized_anchor_path(&self.config);
                self.configure_remote_setup(&url, "", project_path.as_deref(), cancellation)
                    .await
            }
        }
    }

    pub async fn update_tunnel_proxy(
        &mut self,
        mode: TunnelProxyMode,
        custom_url: Option<&str>,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let custom_url = match mode {
            TunnelProxyMode::Custom => Some(validate_tunnel_proxy_url(custom_url.unwrap_or(""))?),
            TunnelProxyMode::Auto | TunnelProxyMode::Direct => None,
        };
        self.config.tunnel_proxy = TunnelProxyConfig { mode, custom_url };
        self.resolved_tunnel_proxy = None;
        self.save_config().await?;
        self.get_state().await
    }

    pub async fn activate_local_project(
        &mut self,
        project_path: &str,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let full_topology = self.config.topology.as_ref().is_some_and(|topology| {
            topology.experience == Experience::Full
        });
        let remote_topology = self.config.topology.as_ref().is_some_and(|topology| {
            topology.experience == Experience::Full
                && matches!(topology.server, ServerTopology::Remote { .. })
        });
        if !full_topology || self.snapshot.quick_share.is_some() {
            return Err(DesktopError::new(
                "unsupported_topology",
                "Projects can only be activated on a connected Full Runtime",
                "Use the full runtime setup flow before activating another project.",
            ));
        }
        if !self.snapshot.readiness.runtime_ready {
            return Err(DesktopError::new(
                "runtime_not_ready",
                "The Full Runtime is not ready for an in-place project activation",
                "Restore the runtime, then choose the project again.",
            ));
        }

        let project = self.adapter.inspect_project(project_path).await?;
        cancellation.check()?;
        let identity = runner_identity_from_config(&self.config).ok_or_else(|| {
            DesktopError::new(
                "runtime_not_ready",
                "The saved Runner identity is incomplete",
                "Restore or reconfigure the runtime before activating another project.",
            )
        })?;
        let runner_client_id = stored_runner_client_id(&self.config).ok_or_else(|| {
            DesktopError::new(
                "runner_offline",
                "The saved Runner client identity is unavailable",
                "Restore or reconfigure the runtime before activating another project.",
            )
        })?;
        let runner = self
            .adapter
            .observe_runner_identity(&identity, Some(&runner_client_id), cancellation)
            .await?;
        cancellation.check()?;
        if !runner.online() {
            return Err(DesktopError::new(
                "runner_offline",
                "The current Runner is not online",
                "Restore the runtime, then choose the project again.",
            ));
        }

        let activated = self
            .adapter
            .activate_project_for_runner(&identity, &runner_client_id, &project, cancellation)
            .await?;
        self.wait_for_project(&activated, cancellation).await?;
        cancellation.check()?;

        let previous_config = self.config.clone();
        let previous_snapshot = self.snapshot.clone();
        let mut committed_project = project;
        committed_project.runtime_project_id = Some(activated.runtime_project_id.clone());
        upsert_authorized_project(&mut self.config, committed_project.clone());
        if remote_topology {
            self.config.project = Some(committed_project.clone());
            if let Some(runtime) = self.config.runtime.as_mut() {
                runtime.project_id = Some(activated.project_id.clone());
                runtime.runtime_project_id = Some(activated.runtime_project_id.clone());
            }
        }
        if let Err(error) = self.save_config().await {
            self.config = previous_config;
            self.snapshot = previous_snapshot;
            self.publish_snapshot();
            return Err(error);
        }

        self.activity.push(
            ActivityEventKind::ProjectActivated,
            "desktop",
            ActivityLevel::Info,
            std::path::Path::new(&committed_project.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
        );
        self.snapshot.chatgpt_activity = None;
        self.snapshot.project = project_snapshot(&self.config);
        self.snapshot.readiness = aggregate_readiness(
            self.snapshot.readiness.server.clone(),
            self.snapshot.readiness.runner.clone(),
            self.snapshot.readiness.exposure.clone(),
            ProjectReadiness::Ready,
        );
        self.get_state().await
    }

    pub async fn remove_local_project(
        &mut self,
        project_path: &str,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let remote_topology = self.config.topology.as_ref().is_some_and(|topology| {
            topology.experience == Experience::Full
                && matches!(topology.server, ServerTopology::Remote { .. })
        });
        let project = self
            .config
            .projects
            .iter()
            .find(|project| project_paths_equal(&project.path, project_path))
            .cloned()
            .ok_or_else(|| {
                DesktopError::new(
                    "project_not_registered",
                    "This local path is not currently authorized for CodexLink",
                    "Refresh the Projects page and retry.",
                )
            })?;
        let identity = runner_identity_from_config(&self.config).ok_or_else(|| {
            DesktopError::new(
                "runtime_not_ready",
                "The saved Runner identity is incomplete",
                "Restore the runtime before removing project access.",
            )
        })?;

        self.adapter
            .remove_project_for_runner(&identity, &project, cancellation)
            .await?;
        cancellation.check()?;

        let previous_config = self.config.clone();
        remove_authorized_project(&mut self.config, &project.path);
        if remote_topology
            && self
                .config
                .project
                .as_ref()
                .is_some_and(|active| project_paths_equal(&active.path, &project.path))
        {
            self.config.project = self.config.projects.first().cloned();
            if let Some(runtime) = self.config.runtime.as_mut() {
                runtime.runtime_project_id = self
                    .config
                    .project
                    .as_ref()
                    .and_then(|next| next.runtime_project_id.clone());
                runtime.project_id = runtime
                    .runtime_project_id
                    .as_deref()
                    .and_then(|runtime_id| runtime_id.rsplit(':').next())
                    .map(str::to_string);
            }
        }
        if let Err(error) = self.save_config().await {
            self.config = previous_config;
            return Err(error);
        }
        self.activity.push(
            ActivityEventKind::ProjectActivated,
            "desktop",
            ActivityLevel::Info,
            format!("Removed project access: {}", project.path),
        );
        self.snapshot.chatgpt_activity = None;
        self.snapshot.project = project_snapshot(&self.config);
        self.snapshot.readiness = aggregate_readiness(
            self.snapshot.readiness.server.clone(),
            self.snapshot.readiness.runner.clone(),
            self.snapshot.readiness.exposure.clone(),
            if remote_topology {
                self.config
                    .project
                    .as_ref()
                    .map(|_| ProjectReadiness::Ready)
                    .unwrap_or(ProjectReadiness::None)
            } else {
                self.snapshot.readiness.project.clone()
            },
        );
        self.get_state().await
    }

    pub async fn configure_local_setup(
        &mut self,
        project_path: Option<&str>,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let requested_user_project = project_path
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string);
        tokio::fs::create_dir_all(&self.default_project_dir)
            .await
            .map_err(|error| {
                DesktopError::new(
                    "default_project_unavailable",
                    "Desktop could not prepare its internal runtime anchor",
                    "Check the current-user app-data permissions and retry.",
                )
                .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
            })?;
        let mut project = self
            .adapter
            .inspect_project(&self.default_project_dir.to_string_lossy())
            .await?;
        project.allowed_root = project.path.clone();
        cancellation.check()?;
        let binaries = self.adapter.ensure_binaries(cancellation).await?.clone();
        self.snapshot.binaries = Some(binaries.info());
        self.activity.push(
            ActivityEventKind::LocalSetupPreparing,
            "desktop",
            ActivityLevel::Info,
            "Preparing CodexLink on this computer",
        );
        self.snapshot.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        self.snapshot.project = project_snapshot(&self.config);
        self.snapshot.projects = projects_snapshot(&self.config);
        self.snapshot.chatgpt_activity = None;
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Stopped,
            ExposureReadiness::Disabled,
            if self.config.projects.is_empty() && requested_user_project.is_none() {
                ProjectReadiness::None
            } else {
                ProjectReadiness::Configured
            },
        );
        self.publish_snapshot();

        let local_dir = self.data_dir.join("runtime").join("local");
        let env_file = local_dir.join("webcodex.env");
        let data_dir = local_dir.join("data");
        tokio::fs::create_dir_all(&local_dir).await.map_err(|_| {
            DesktopError::new(
                "desktop_state_unavailable",
                "Desktop could not create its local runtime directory",
                "Check local app-data permissions and retry.",
            )
        })?;
        cancellation.check()?;

        let server_url = if env_file.is_file() {
            ensure_desktop_server_defaults(&env_file)?;
            self.adapter
                .server_status(None, Some(&env_file), None, cancellation)
                .await?
                .probe_url
        } else {
            let listen = reserve_loopback_address()?;
            let status = self
                .adapter
                .init_local_server(&listen, &data_dir, &env_file, cancellation)
                .await?;
            ensure_desktop_server_defaults(&env_file)?;
            status.probe_url
        };
        let reusable_identity = (!local_runtime_requires_zero_root_reenrollment(&self.config))
            .then(|| runner_identity_from_config(&self.config))
            .flatten()
            .filter(|identity| same_server(&identity.server_url, &server_url));
        let saved_runner_client_id = stored_runner_client_id(&self.config);
        cancellation.check()?;

        let server_deadline = Deadline::after(SERVER_READY_TIMEOUT);
        let running = self
            .adapter
            .server_status_until(
                Some(&server_url),
                Some(&env_file),
                None,
                cancellation,
                server_deadline,
            )
            .await
            .is_ok_and(|status| status.http_reachable);
        cancellation.check()?;
        let server_started = if !running {
            if server_deadline.is_elapsed() {
                return Err(readiness_timeout_error(
                    "server_unreachable",
                    "CodexLink Service did not become ready",
                    "Check the local Service diagnostics and retry.",
                ));
            }
            let command = self.adapter.local_server_command(&env_file)?;
            self.spawn_owned(ProcessKind::LocalServer, command, false, cancellation)
                .await?;
            true
        } else {
            false
        };
        self.wait_for_server(
            &server_url,
            Some(&env_file),
            None,
            cancellation,
            server_deadline,
            server_started,
        )
        .await?;
        self.snapshot.readiness.server = ServerReadiness::Ready;
        self.publish_snapshot();

        let reusable_observation = match reusable_identity.as_ref() {
            Some(identity) => self
                .adapter
                .observe_runner_identity(
                    identity,
                    saved_runner_client_id.as_deref(),
                    cancellation,
                )
                .await
                .ok(),
            None => None,
        };
        let (runner_identity, identity_replaced, runner_client_id) =
            match (reusable_identity, reusable_observation) {
                (Some(identity), Some(observation)) => (identity, false, observation.client_id),
                _ => {
                    // Product-managed Local enrollment starts with true zero customer
                    // authority. Existing user projects are replayed only after the
                    // exact Runner identity is online; the internal runtime anchor is
                    // never registered as a Project or allowed_root.
                    let connections_dir = local_enrollment_directory(&self.data_dir, &self.config);
                    let pairing_code = self
                        .adapter
                        .create_local_pairing(&server_url, &env_file, cancellation)
                        .await?;
                    let identity = self
                        .adapter
                        .login_runner_with_pairing(
                            &server_url,
                            &pairing_code,
                            &connections_dir,
                            cancellation,
                        )
                        .await?;
                    drop(pairing_code);
                    cancellation.check()?;
                    let observation = self
                        .adapter
                        .observe_runner_identity(&identity, None, cancellation)
                        .await?;
                    (identity, true, observation.client_id)
                }
            };
        let runner_deadline = Deadline::after(RUNNER_READY_TIMEOUT);
        let preparation = self
            .prepare_owned_runner_target(
                &runner_identity,
                identity_replaced,
                runner_deadline,
                cancellation,
            )
            .await?;
        let replacing_owned_runner = preparation.replaced;
        let activation: DesktopResult<bool> = async {
            let runner_ready = if identity_replaced || replacing_owned_runner {
                // A fresh enrollment may reuse the same path/client_id with new
                // credentials, while a topology switch may replace a different
                // Desktop-owned Runner target. Neither old observation proves this
                // exact generation is active.
                false
            } else {
                self.adapter
                    .runner_identity_ready_until(&runner_identity, cancellation, runner_deadline)
                    .await
                    .unwrap_or(false)
            };
            cancellation.check()?;
            let runner_started = if !runner_ready {
                if runner_deadline.is_elapsed() {
                    return Err(readiness_timeout_error(
                        "runner_offline",
                        "Runner did not become connected",
                        "Retry project setup to restart Desktop's Runner.",
                    ));
                }
                self.snapshot.readiness.runner = RunnerReadiness::Connecting;
                self.publish_snapshot();
                if preparation.target_already_active {
                    false
                } else {
                    self.spawn_owned_runner(&runner_identity, cancellation).await?;
                    true
                }
            } else {
                false
            };
            self.wait_for_runner_identity(
                &runner_identity,
                cancellation,
                runner_deadline,
                runner_started,
            )
            .await?;
            self.snapshot.readiness.runner = RunnerReadiness::Ready;
            self.publish_snapshot();
            cancellation.check()?;
            Ok(runner_started)
        }
        .await;
        let runner_started = match activation {
            Ok(runner_started) => runner_started,
            Err(error) => {
                if replacing_owned_runner {
                    self.stop_process_until(
                        ProcessKind::LocalRunner,
                        Deadline::after(READINESS_CLEANUP_SLACK),
                    )
                    .await;
                    self.snapshot.topology = self.config.topology.clone();
                    self.snapshot.project = project_snapshot(&self.config);
                    self.snapshot.readiness = aggregate_readiness(
                        ServerReadiness::Ready,
                        RunnerReadiness::Stopped,
                        exposure_readiness(self.config.topology.as_ref()),
                        self.config
                            .projects
                            .first()
                            .map(|_| ProjectReadiness::Configured)
                            .unwrap_or(ProjectReadiness::None),
                    );
                    self.publish_snapshot();
                }
                return Err(error);
            }
        };

        // Commit the internal runtime anchor only as Desktop recovery metadata.
        // It is deliberately not a Runner allowed_root and has no runtime Project
        // identity; only explicit customer projects in `projects[]` are activated.
        let previous_config = self.config.clone();
        let mut committed_anchor = project.clone();
        committed_anchor.runtime_project_id = None;
        self.config.topology = self.snapshot.topology.clone();
        self.config.runtime_anchor = Some(committed_anchor);
        self.config.project = None;
        self.config.runtime_autostart = Some(true);
        self.config.runtime = Some(StoredRuntime {
            server_url: runner_identity.server_url.clone(),
            server_env_file: Some(env_file.clone()),
            runner_config: Some(runner_identity.runner_config.clone()),
            user_token_file: Some(runner_identity.user_token_file.clone()),
            runner_client_id: Some(runner_client_id.clone()),
            project_id: None,
            runtime_project_id: None,
        });
        if let Err(error) = self.save_config().await {
            self.config = previous_config;
            self.snapshot.topology = self.config.topology.clone();
            self.snapshot.project = project_snapshot(&self.config);
            if runner_started || replacing_owned_runner {
                self.stop_process_until(
                    ProcessKind::LocalRunner,
                    Deadline::after(READINESS_CLEANUP_SLACK),
                )
                .await;
            }
            if server_started {
                self.stop_process_until(
                    ProcessKind::LocalServer,
                    Deadline::after(READINESS_CLEANUP_SLACK),
                )
                .await;
            }
            self.snapshot.readiness = aggregate_readiness(
                if server_started {
                    ServerReadiness::Stopped
                } else {
                    ServerReadiness::Ready
                },
                if runner_started || replacing_owned_runner {
                    RunnerReadiness::Stopped
                } else {
                    RunnerReadiness::Ready
                },
                ExposureReadiness::Disabled,
                self.config
                    .projects
                    .first()
                    .map(|_| ProjectReadiness::Configured)
                    .unwrap_or(ProjectReadiness::None),
            );
            self.publish_snapshot();
            return Err(error);
        }

        if identity_replaced || requested_user_project.is_some() {
            self.reconcile_authorized_projects(
                &runner_identity,
                &runner_client_id,
                requested_user_project.as_deref(),
                identity_replaced,
                cancellation,
            )
            .await?;
            self.save_config().await?;
        }

        self.snapshot.project = project_snapshot(&self.config);
        self.snapshot.projects = projects_snapshot(&self.config);
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            ExposureReadiness::LocalReady,
            if self.config.projects.is_empty() {
                ProjectReadiness::None
            } else {
                ProjectReadiness::Ready
            },
        );

        self.activity.push(
            ActivityEventKind::LocalRuntimeReady,
            "desktop",
            ActivityLevel::Info,
            "Local CodexLink runtime is ready",
        );
        self.get_state().await
    }

    async fn reconcile_authorized_projects(
        &mut self,
        runner_identity: &RunnerRuntimeIdentity,
        runner_client_id: &str,
        requested_user_project: Option<&str>,
        authority_rebuilt_from_anchor: bool,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        let mut paths = self
            .config
            .projects
            .iter()
            .map(|project| project.path.clone())
            .collect::<Vec<_>>();
        if let Some(requested) = requested_user_project {
            if !paths
                .iter()
                .any(|path| project_paths_equal(path, requested))
            {
                paths.push(requested.to_string());
            }
        }

        let mut reconciled = Vec::with_capacity(paths.len());
        for path in paths {
            cancellation.check()?;
            let project = match self.adapter.inspect_project(&path).await {
                Ok(project) => project,
                Err(error)
                    if requested_user_project
                        .is_some_and(|requested| project_paths_equal(requested, &path)) =>
                {
                    return Err(error);
                }
                Err(_) if authority_rebuilt_from_anchor => {
                    self.activity.push(
                        ActivityEventKind::ProjectActivated,
                        "desktop",
                        ActivityLevel::Warning,
                        format!("Dropped unavailable legacy project authorization: {path}"),
                    );
                    continue;
                }
                Err(_) => {
                    if let Some(existing) = self
                        .config
                        .projects
                        .iter()
                        .find(|existing| project_paths_equal(&existing.path, &path))
                        .cloned()
                    {
                        reconciled.push(existing);
                    }
                    continue;
                }
            };
            let activated = self
                .adapter
                .activate_project_for_runner(
                    runner_identity,
                    runner_client_id,
                    &project,
                    cancellation,
                )
                .await?;
            self.wait_for_project(&activated, cancellation).await?;
            let mut committed = project;
            committed.runtime_project_id = Some(activated.runtime_project_id);
            if !reconciled.iter().any(|existing: &ProjectSelection| {
                project_paths_equal(&existing.path, &committed.path)
            }) {
                reconciled.push(committed);
            }
        }
        self.config.projects = reconciled;
        Ok(())
    }

    pub async fn configure_remote_setup(
        &mut self,
        server_url: &str,
        pairing_code: &str,
        project_path: Option<&str>,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let server_url = crate::webcodex::validate_server_url(server_url)?;
        let project = match project_path.map(str::trim).filter(|path| !path.is_empty()) {
            Some(path) => Some(self.adapter.inspect_project(path).await?),
            None => None,
        };
        cancellation.check()?;
        let binaries = self.adapter.ensure_binaries(cancellation).await?.clone();
        let exposure = if server_url.starts_with("https://") {
            Exposure::ExistingHttps {
                url: server_url.clone(),
            }
        } else {
            Exposure::None
        };
        let topology = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Remote {
                url: server_url.clone(),
            },
            runner: RunnerTopology::Local,
            exposure,
            enrollment: Enrollment::ManagedPairing,
        };
        self.activity.push(
            ActivityEventKind::RemoteConnecting,
            "desktop",
            ActivityLevel::Info,
            "Validating the existing CodexLink Server before switching topology",
        );

        let connections_dir = self.data_dir.join("connections");
        let configured_identity = runner_identity_from_config(&self.config)
            .filter(|identity| same_server(&identity.server_url, &server_url));
        let configured_client_id = configured_identity
            .as_ref()
            .and_then(|_| stored_runner_client_id(&self.config));
        let (reusable_identity, expected_client_id) = if let Some(identity) = configured_identity {
            (Some(identity), configured_client_id)
        } else {
            match self
                .adapter
                .saved_runner_identity(&server_url, &connections_dir, cancellation)
                .await?
            {
                Some((identity, client_id)) => (Some(identity), Some(client_id)),
                None => (None, None),
            }
        };

        let reusable = match reusable_identity {
            Some(identity) => {
                let observation = self
                    .adapter
                    .observe_runner_identity(
                        &identity,
                        expected_client_id.as_deref(),
                        cancellation,
                    )
                    .await?;
                match observation.state {
                    RunnerConnectionState::CredentialInvalid => None,
                    RunnerConnectionState::ServerUnavailable | RunnerConnectionState::Unknown => {
                        return Err(DesktopError::new(
                            "remote_server_offline",
                            "The saved CodexLink enrollment is still present, but the Server is currently unreachable",
                            "Keep the saved enrollment and retry when the Server is reachable; do not create a new pairing code.",
                        ));
                    }
                    RunnerConnectionState::Online | RunnerConnectionState::RunnerOffline => {
                        Some((identity, observation.client_id))
                    }
                }
            }
            None => None,
        };

        let (mut identity, identity_replaced, runner_client_id) = if let Some(reusable) = reusable {
            (reusable.0, false, reusable.1)
        } else {
            if !pairing_code.starts_with("wc_pair_") {
                let code = if expected_client_id.is_some() {
                    "remote_credential_invalid"
                } else {
                    "pairing_code_required"
                };
                return Err(DesktopError::new(
                    code,
                    if code == "remote_credential_invalid" {
                        "The saved CodexLink enrollment was rejected by the Server"
                    } else {
                        "This Server has no saved CodexLink enrollment on this computer"
                    },
                    "Provide a fresh wc_pair_… code only when enrolling this Server or replacing a revoked credential.",
                ));
            }
            let identity = self
                .adapter
                .login_runner_with_pairing(
                    &server_url,
                    pairing_code,
                    &connections_dir,
                    cancellation,
                )
                .await?;
            let observation = self
                .adapter
                .observe_runner_identity(&identity, None, cancellation)
                .await?;
            match observation.state {
                RunnerConnectionState::CredentialInvalid => {
                    return Err(DesktopError::new(
                        "remote_credential_invalid",
                        "The newly enrolled credential was rejected by the Server",
                        "Generate a fresh pairing code and enroll again.",
                    ));
                }
                RunnerConnectionState::ServerUnavailable | RunnerConnectionState::Unknown => {
                    return Err(DesktopError::new(
                        "remote_server_offline",
                        "The enrollment was saved, but the Server became unreachable before topology validation completed",
                        "Keep the saved enrollment and retry later without generating another pairing code.",
                    ));
                }
                RunnerConnectionState::Online | RunnerConnectionState::RunnerOffline => {}
            }
            (identity, true, observation.client_id)
        };

        let server_deadline = Deadline::after(SERVER_READY_TIMEOUT);
        let server_status = match self
            .adapter
            .server_status_until(
                Some(&server_url),
                None,
                Some(&identity.user_token_file),
                cancellation,
                server_deadline,
            )
            .await
        {
            Ok(status) => status,
            Err(error) => {
                cancellation.check()?;
                if server_deadline.is_elapsed() {
                    return Err(readiness_timeout_error(
                        "server_unreachable",
                        "The existing CodexLink Server did not respond before the readiness deadline",
                        "Check the Server URL and network path, then retry.",
                    ));
                }
                return Err(error);
            }
        };
        cancellation.check()?;
        if !server_status.http_reachable {
            return Err(DesktopError::new(
                "server_unreachable",
                "The existing CodexLink Server is not reachable",
                "Check the Server URL and network path, then retry.",
            ));
        }
        let runner_deadline = Deadline::after(RUNNER_READY_TIMEOUT);
        let preparation = self
            .prepare_owned_runner_target(
                &identity,
                identity_replaced,
                runner_deadline,
                cancellation,
            )
            .await?;
        let replacing_owned_runner = preparation.replaced;
        let runner_ready = if identity_replaced || replacing_owned_runner {
            false
        } else {
            self.adapter
                .runner_identity_ready_until(&identity, cancellation, runner_deadline)
                .await
                .unwrap_or(false)
        };
        cancellation.check()?;
        let mut runner_started = if !runner_ready {
            if runner_deadline.is_elapsed() {
                return Err(readiness_timeout_error(
                    "runner_offline",
                    "Runner did not become connected",
                    "Check Server reachability and Runner diagnostics, then retry.",
                ));
            }
            if preparation.target_already_active {
                false
            } else {
                self.spawn_owned_runner(&identity, cancellation).await?;
                true
            }
        } else {
            false
        };
        self.wait_for_runner_identity(&identity, cancellation, runner_deadline, runner_started)
            .await?;

        let mut activated_identity: Option<ProjectRuntimeIdentity> = None;
        if let Some(project) = project.as_ref() {
            let activated = match self
                .adapter
                .activate_project_for_runner(&identity, &runner_client_id, project, cancellation)
                .await
            {
                Ok(identity) => identity,
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        "project_activation_capability_unavailable"
                            | "project_activation_restart_required"
                    ) =>
                {
                    if !can_refresh_legacy_runner(
                        self.process_snapshot(ProcessKind::LocalRunner).await,
                    ) {
                        return Err(DesktopError::new(
                            "project_activation_legacy_runner",
                            "This Runner needs to be refreshed before the new project can be activated",
                            "Refresh the Runner, then select this project again.",
                        ));
                    }
                    let legacy_identity = self
                        .adapter
                        .legacy_register_project_for_runner(
                            &identity,
                            &runner_client_id,
                            project,
                            cancellation,
                        )
                        .await?;
                    let legacy_deadline = Deadline::after(RUNNER_READY_TIMEOUT);
                    self.stop_process_until(ProcessKind::LocalRunner, legacy_deadline)
                        .await;
                    if legacy_deadline.is_elapsed() {
                        return Err(readiness_timeout_error(
                            "runner_offline",
                            "Desktop could not refresh its legacy Runner",
                            "Retry project setup after checking Runner diagnostics.",
                        ));
                    }
                    identity = legacy_identity.runner_identity();
                    self.spawn_owned_runner(&identity, cancellation).await?;
                    self.wait_for_runner_identity(&identity, cancellation, legacy_deadline, true)
                        .await?;
                    runner_started = true;
                    legacy_identity
                }
                Err(error) => return Err(error),
            };
            self.wait_for_project(&activated, cancellation).await?;
            activated_identity = Some(activated);
        }
        cancellation.check()?;

        let previous_config = self.config.clone();
        self.config.topology = Some(topology.clone());
        self.config.runtime_anchor = None;
        self.config.project = None;
        self.config.projects.clear();
        self.config.runtime_autostart = Some(true);
        self.config.preferred_connection = Some(RegularConnectionPreference::NoChatGpt);
        let mut project_readiness = ProjectReadiness::None;
        let (project_id, runtime_project_id) = if let (Some(project), Some(activated)) =
            (project.as_ref(), activated_identity.as_ref())
        {
            let mut committed = project.clone();
            committed.runtime_project_id = Some(activated.runtime_project_id.clone());
            self.config.project = Some(committed.clone());
            self.config.projects.push(committed);
            project_readiness = ProjectReadiness::Ready;
            (
                Some(activated.project_id.clone()),
                Some(activated.runtime_project_id.clone()),
            )
        } else {
            (None, None)
        };
        self.config.runtime = Some(StoredRuntime {
            server_url: identity.server_url.clone(),
            server_env_file: None,
            runner_config: Some(identity.runner_config.clone()),
            user_token_file: Some(identity.user_token_file.clone()),
            runner_client_id: Some(runner_client_id),
            project_id,
            runtime_project_id,
        });
        if let Err(error) = self.save_config().await {
            self.config = previous_config;
            if runner_started || replacing_owned_runner {
                self.stop_process_until(
                    ProcessKind::LocalRunner,
                    Deadline::after(READINESS_CLEANUP_SLACK),
                )
                .await;
            }
            return Err(error);
        }

        self.snapshot.binaries = Some(binaries.info());
        self.snapshot.topology = Some(topology);
        self.snapshot.project = project_snapshot(&self.config);
        self.snapshot.projects = projects_snapshot(&self.config);
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            if server_url.starts_with("https://") {
                ExposureReadiness::RemoteReady
            } else {
                ExposureReadiness::Degraded
            },
            project_readiness,
        );
        self.activity.push(
            ActivityEventKind::RemoteConnected,
            "desktop",
            ActivityLevel::Info,
            "This computer is connected to the existing CodexLink Server",
        );
        self.get_state().await
    }

    pub async fn start_quick_share(
        &mut self,
        project_path: &str,
        provider: &str,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let project = self.adapter.inspect_project(project_path).await?;
        cancellation.check()?;
        let binaries = self.adapter.ensure_binaries(cancellation).await?.clone();
        self.snapshot.binaries = Some(binaries.info());
        if self
            .process_snapshot(ProcessKind::QuickShare)
            .await
            .is_some_and(|process| {
                matches!(
                    process.phase,
                    ProcessPhase::Starting | ProcessPhase::Running
                )
            })
        {
            return Err(DesktopError::new(
                "quick_share_already_running",
                "Quick Share is already running",
                "Stop the current share before starting another one.",
            ));
        }
        let tunnel_proxy = if provider == "openai" {
            let selected =
                resolve_tunnel_proxy_for_start(&self.config.tunnel_proxy, cancellation).await?;
            self.resolved_tunnel_proxy = Some(selected.clone());
            selected
        } else {
            effective_tunnel_proxy(&self.config.tunnel_proxy)?
        };
        let deadline = Deadline::after(QUICK_SHARE_READY_TIMEOUT);
        let mut command = self.adapter.quick_share_command(
            Path::new(&project.path),
            provider,
            tunnel_proxy.url.as_deref(),
        )?;
        if provider == "openai" {
            self.tunnel_config.apply_to_command(&mut command)?;
        }
        if deadline.is_elapsed() {
            return Err(readiness_timeout_error(
                "quick_share_not_ready",
                "Quick Share did not reach verified readiness",
                "Check Activity and Tunnel prerequisites, then retry.",
            ));
        }
        let mut events = self
            .spawn_owned(ProcessKind::QuickShare, command, true, cancellation)
            .await?
            .expect("machine stdout requested");
        self.snapshot.topology = Some(RuntimeTopology {
            experience: Experience::QuickShare,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: match provider {
                "cloudflare" => Exposure::Cloudflare,
                "openai" => Exposure::OpenAiTunnel,
                _ => Exposure::None,
            },
            enrollment: Enrollment::ExistingProfile {
                profile: "temporary_share".to_string(),
            },
        });
        self.stage_project_scope(project.clone());
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Connecting,
            ExposureReadiness::Starting,
            ProjectReadiness::Configured,
        );
        self.activity.push(
            ActivityEventKind::QuickShareStarting,
            "quick_share",
            ActivityLevel::Info,
            "Starting the temporary Quick Share runtime",
        );
        self.publish_snapshot();
        let event_wait = async {
            while let Some(value) = events.recv().await {
                match value.get("event").and_then(Value::as_str) {
                    Some("ready") => return Ok(Some(value)),
                    Some("machine_event_overflow") => return Err(value),
                    _ => {}
                }
            }
            Ok(None)
        };
        let event_result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                self.stop_process_until(
                    ProcessKind::QuickShare,
                    Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
                ).await;
                return Err(cancelled_error());
            }
            result = tokio::time::timeout_at(deadline.instant(), event_wait) => {
                result
            }
        };
        let event_value = match event_result {
            Ok(Ok(Some(value))) => value,
            Ok(Err(overflow)) => {
                self.stop_process_until(
                    ProcessKind::QuickShare,
                    Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
                )
                .await;
                return Err(machine_event_overflow_error(&overflow));
            }
            Ok(Ok(None)) | Err(_) => {
                let logs = self.process_logs(ProcessKind::QuickShare).await;
                self.stop_process_until(
                    ProcessKind::QuickShare,
                    Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
                )
                .await;
                return Err(DesktopError::new(
                    "quick_share_not_ready",
                    "Quick Share did not reach verified readiness",
                    "Check Activity and Tunnel prerequisites, then retry.",
                )
                .with_details(serde_json::json!({
                    "category": "readiness_timeout",
                    "diagnostic_lines": logs,
                })));
            }
        };
        let event: QuickShareReadyEvent = match serde_json::from_value(event_value) {
            Ok(event) => event,
            Err(_) => {
                self.stop_process(ProcessKind::QuickShare).await;
                return Err(DesktopError::new(
                    "webcodex_contract_invalid",
                    "Quick Share returned an invalid readiness event",
                    "Verify that Desktop and CodexLink binaries come from the same source baseline.",
                ));
            }
        };
        if event.event != "ready"
            || event.schema_version != 1
            || event.experience != "quick_share"
            || event.project.trim().is_empty()
            || event.exposure.kind.trim().is_empty()
        {
            self.stop_process(ProcessKind::QuickShare).await;
            return Err(DesktopError::new(
                "webcodex_contract_invalid",
                "Quick Share readiness identity is incomplete",
                "Update Desktop and CodexLink together.",
            ));
        }
        cancellation.check()?;
        let clipboard_required = event.connection.clipboard_contains != "none";
        let handoff_available = !clipboard_required || event.connection.clipboard_state == "copied";
        let ready_for_chatgpt = event.ready_for_chatgpt && handoff_available;
        self.snapshot.quick_share = Some(QuickShareState {
            provider: provider.to_string(),
            project: project.path.clone(),
            mcp_url: event.connection.mcp_url,
            clipboard_state: event.connection.clipboard_state,
            clipboard_contains: event.connection.clipboard_contains,
            ready_for_chatgpt,
        });
        let exposure_readiness = match event.exposure.state.as_str() {
            "remote_ready" if handoff_available => ExposureReadiness::RemoteReady,
            "remote_ready" => ExposureReadiness::Degraded,
            "local_ready" => ExposureReadiness::LocalReady,
            _ => ExposureReadiness::Unknown,
        };
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            exposure_readiness,
            ProjectReadiness::Ready,
        );
        if !handoff_available {
            self.snapshot.readiness.next_action_kind =
                Some(ReadinessNextActionKind::RestoreClipboardHandoff);
            self.snapshot.readiness.next_action =
                Some("Clipboard handoff is unavailable; restart Quick Share after clipboard access is restored.".to_string());
        }
        self.activity.push(
            ActivityEventKind::QuickShareReady,
            "quick_share",
            ActivityLevel::Info,
            "Quick Share reached verified readiness",
        );
        self.get_state().await
    }

    pub async fn stop_quick_share(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.stop_process(ProcessKind::QuickShare).await;
        self.snapshot.quick_share = None;
        self.snapshot.topology = self.config.topology.clone();
        self.snapshot.project = project_snapshot(&self.config);
        self.activity.push(
            ActivityEventKind::QuickShareStopped,
            "quick_share",
            ActivityLevel::Info,
            "Quick Share stopped",
        );
        if self.config.runtime.is_some() {
            self.refresh_runtime_status(cancellation).await
        } else {
            self.snapshot.readiness = DesktopStateSnapshot::default().readiness;
            self.get_state().await
        }
    }

    pub async fn start_regular_tunnel(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        if self.snapshot.quick_share.is_some() {
            return Err(DesktopError::new(
                "unsupported_topology",
                "Regular ChatGPT Connection is unavailable while Quick Share is active",
                "Stop Quick Share and start the Local Full Runtime first.",
            ));
        }
        let topology = self.config.topology.clone().ok_or_else(|| {
            DesktopError::new(
                "runtime_not_ready",
                "Local Full Runtime has not been configured",
                "Set up CodexLink on this computer before starting the secure tunnel.",
            )
        })?;
        if topology.experience != Experience::Full
            || !matches!(topology.server, ServerTopology::Local)
        {
            return Err(DesktopError::new(
                "unsupported_topology",
                "Regular OpenAI Secure Tunnel is only started for a local CodexLink Server",
                "Manage external exposure on the remote Server instead.",
            ));
        }
        if !self.tunnel_config.snapshot().is_configured() {
            return Err(DesktopError::new(
                "tunnel_unavailable",
                "OpenAI Secure Tunnel is not configured",
                "Save the Tunnel ID and API key in Desktop settings, then retry.",
            ));
        }
        if self
            .process_snapshot(ProcessKind::RegularTunnel)
            .await
            .is_some_and(|process| {
                matches!(
                    process.phase,
                    ProcessPhase::Starting | ProcessPhase::Running
                )
            })
        {
            return Err(DesktopError::new(
                "regular_tunnel_already_running",
                "OpenAI Secure Tunnel is already running",
                "Stop the current secure tunnel before starting another one.",
            ));
        }

        let current = self.refresh_runtime_status(cancellation).await?;
        if !current.readiness.runtime_ready {
            return Err(DesktopError::new(
                "runtime_not_ready",
                "Local CodexLink runtime is not ready",
                "Restore the Server and Runner readiness before starting the secure tunnel.",
            ));
        }
        let runtime = self.config.runtime.clone().ok_or_else(|| {
            DesktopError::new(
                "runtime_not_ready",
                "Local runtime identity is incomplete",
                "Run Local Setup again.",
            )
        })?;
        let env_file = runtime
            .server_env_file
            .filter(|path| path.is_file())
            .ok_or_else(|| {
                DesktopError::new(
                    "server_unavailable",
                    "Local Server configuration is unavailable",
                    "Run Local Setup again.",
                )
            })?;
        let tunnel_proxy =
            match resolve_tunnel_proxy_for_start(&self.config.tunnel_proxy, cancellation).await {
                Ok(proxy) => proxy,
                Err(error) => {
                    self.resolved_tunnel_proxy = None;
                    return Err(error);
                }
            };
        self.resolved_tunnel_proxy = Some(tunnel_proxy.clone());
        let deadline = Deadline::after(REGULAR_TUNNEL_READY_TIMEOUT);
        let mut command = self
            .adapter
            .regular_tunnel_command(&env_file, tunnel_proxy.url.as_deref())?;
        self.tunnel_config.apply_to_command(&mut command)?;
        if deadline.is_elapsed() {
            return Err(readiness_timeout_error(
                "tunnel_unavailable",
                "OpenAI Secure Tunnel did not reach verified readiness",
                "Check Activity and the canonical Tunnel prerequisites, then retry.",
            ));
        }
        let mut events = self
            .spawn_owned(ProcessKind::RegularTunnel, command, true, cancellation)
            .await?
            .expect("regular tunnel machine stdout requested");
        self.snapshot.regular_tunnel = Some(RegularTunnelState {
            provider: "openai".to_string(),
            status: RegularTunnelStatus::Starting,
            clipboard_state: "pending".to_string(),
            clipboard_contains: "tunnel_id".to_string(),
            ready_for_chatgpt: false,
        });
        self.snapshot.topology = effective_topology(self.config.topology.as_ref(), true);
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            ExposureReadiness::Starting,
            current.readiness.project.clone(),
        );
        self.activity.push(
            ActivityEventKind::RegularTunnelStarting,
            "regular_tunnel",
            ActivityLevel::Info,
            "Starting the regular OpenAI Secure Tunnel",
        );
        self.publish_snapshot();
        let event_wait = async {
            while let Some(value) = events.recv().await {
                match value.get("event").and_then(Value::as_str) {
                    Some("ready") => return Ok(Some(value)),
                    Some("machine_event_overflow") => return Err(value),
                    _ => {}
                }
            }
            Ok(None)
        };
        let event_result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                self.stop_process_until(
                    ProcessKind::RegularTunnel,
                    Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
                ).await;
                return Err(cancelled_error());
            }
            result = tokio::time::timeout_at(deadline.instant(), event_wait) => {
                result
            }
        };
        let event_value = match event_result {
            Ok(Ok(Some(value))) => value,
            Ok(Err(overflow)) => {
                self.stop_process_until(
                    ProcessKind::RegularTunnel,
                    Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
                )
                .await;
                self.snapshot.regular_tunnel = None;
                return Err(machine_event_overflow_error(&overflow));
            }
            Ok(Ok(None)) | Err(_) => {
                let logs = self.process_logs(ProcessKind::RegularTunnel).await;
                self.stop_process_until(
                    ProcessKind::RegularTunnel,
                    Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
                )
                .await;
                self.snapshot.regular_tunnel = Some(RegularTunnelState {
                    provider: "openai".to_string(),
                    status: RegularTunnelStatus::Error,
                    clipboard_state: "unavailable".to_string(),
                    clipboard_contains: "tunnel_id".to_string(),
                    ready_for_chatgpt: false,
                });
                self.snapshot.readiness = aggregate_readiness(
                    ServerReadiness::Ready,
                    RunnerReadiness::Ready,
                    ExposureReadiness::Error,
                    current.readiness.project.clone(),
                );
                apply_regular_tunnel_next_action(&mut self.snapshot, &ExposureReadiness::Error);
                return Err(DesktopError::new(
                    "tunnel_unavailable",
                    "OpenAI Secure Tunnel did not reach verified readiness",
                    "Check Activity and the canonical Tunnel prerequisites, then retry.",
                )
                .with_details(serde_json::json!({
                    "category": "readiness_timeout",
                    "diagnostic_lines": logs,
                })));
            }
        };
        let event: RegularTunnelReadyEvent = match serde_json::from_value(event_value) {
            Ok(event) => event,
            Err(_) => {
                self.stop_process(ProcessKind::RegularTunnel).await;
                self.snapshot.regular_tunnel = None;
                return Err(DesktopError::new(
                    "webcodex_contract_invalid",
                    "Regular Tunnel returned an invalid readiness event",
                    "Verify that Desktop and CodexLink binaries come from the same source baseline.",
                ));
            }
        };
        if event.event != "ready"
            || event.schema_version != 1
            || event.provider != "openai"
            || event.connection.kind != "openai_tunnel"
            || event.connection.clipboard_contains != "tunnel_id"
        {
            self.stop_process(ProcessKind::RegularTunnel).await;
            self.snapshot.regular_tunnel = None;
            return Err(DesktopError::new(
                "webcodex_contract_invalid",
                "Regular Tunnel readiness identity is incomplete",
                "Update Desktop and CodexLink together.",
            ));
        }
        cancellation.check()?;
        // The schema-v1 ready event is emitted only after the daemon is ready.
        // Its ready_for_chatgpt flag additionally requires CLI clipboard success;
        // Desktop can also hand off the same non-secret ID through its copy field.
        let id_available = self.tunnel_config.snapshot().effective_tunnel_id.is_some();
        let handoff_available = event.connection.clipboard_state == "copied" || id_available;
        self.snapshot.regular_tunnel = Some(RegularTunnelState {
            provider: event.provider,
            status: RegularTunnelStatus::Ready,
            clipboard_state: event.connection.clipboard_state,
            clipboard_contains: event.connection.clipboard_contains,
            ready_for_chatgpt: (event.ready_for_chatgpt || id_available) && handoff_available,
        });
        self.config.preferred_connection = Some(RegularConnectionPreference::OpenAiTunnel);
        self.save_config().await?;
        // The daemon and clipboard handoff prove only Desktop-managed Tunnel
        // readiness. Actual ChatGPT use is observed independently from canonical
        // Window activity and projected through `chatgpt_activity` on refresh.
        let exposure = if handoff_available {
            ExposureReadiness::LocalReady
        } else {
            ExposureReadiness::Degraded
        };
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            exposure.clone(),
            current.readiness.project.clone(),
        );
        apply_regular_tunnel_next_action(&mut self.snapshot, &exposure);
        self.snapshot.topology = effective_topology(self.config.topology.as_ref(), true);
        self.snapshot.project = project_snapshot(&self.config);
        self.activity.push(
            ActivityEventKind::RegularTunnelReady,
            "regular_tunnel",
            ActivityLevel::Info,
            "Regular OpenAI Secure Tunnel reached local handoff readiness",
        );
        self.get_state().await
    }

    pub async fn stop_regular_tunnel(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.stop_process(ProcessKind::RegularTunnel).await;
        self.snapshot.regular_tunnel = None;
        self.snapshot.topology = self.config.topology.clone();
        self.config.preferred_connection = Some(RegularConnectionPreference::NoChatGpt);
        self.save_config().await?;
        self.activity.push(
            ActivityEventKind::RegularTunnelStopped,
            "regular_tunnel",
            ActivityLevel::Info,
            "Regular OpenAI Secure Tunnel stopped",
        );
        if self.config.runtime.is_some() {
            self.refresh_runtime_status(cancellation).await
        } else {
            self.get_state().await
        }
    }

    pub async fn stop_local_runtime(
        &mut self,
        _cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.stop_process(ProcessKind::RegularTunnel).await;
        self.snapshot.regular_tunnel = None;
        self.stop_process(ProcessKind::LocalRunner).await;
        self.config.runtime_autostart = Some(false);
        self.save_config().await?;
        self.stop_process(ProcessKind::LocalServer).await;
        self.snapshot.topology = self.config.topology.clone();
        let exposure = exposure_readiness(self.config.topology.as_ref());
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Stopped,
            RunnerReadiness::Stopped,
            exposure,
            self.config
                .project
                .as_ref()
                .map(|_| ProjectReadiness::Configured)
                .unwrap_or(ProjectReadiness::None),
        );
        self.activity.push(
            ActivityEventKind::RuntimeStopped,
            "desktop",
            ActivityLevel::Info,
            "Desktop-managed local runtime stopped",
        );
        self.get_state().await
    }

    async fn process_snapshot(&self, kind: ProcessKind) -> Option<crate::process::ProcessSnapshot> {
        self.supervisor.lock().await.snapshot(kind)
    }

    async fn process_logs(&self, kind: ProcessKind) -> Vec<String> {
        self.supervisor.lock().await.logs(kind)
    }

    async fn prepare_owned_runner_target(
        &self,
        identity: &RunnerRuntimeIdentity,
        identity_replaced: bool,
        deadline: Deadline,
        cancellation: &CancellationContext,
    ) -> DesktopResult<OwnedRunnerPreparation> {
        let target_key = runner_process_target_key(identity);
        let target_match = self
            .supervisor
            .lock()
            .await
            .active_target_matches(ProcessKind::LocalRunner, &target_key);
        let replaced = target_match.is_some_and(|matches| identity_replaced || !matches);
        if replaced {
            self.stop_process_until(ProcessKind::LocalRunner, deadline).await;
            if deadline.is_elapsed() {
                return Err(readiness_timeout_error(
                    "runner_offline",
                    "Desktop could not stop its previous Runner before switching runtime target",
                    "Retry setup. Desktop will only replace the exact Runner generation it owns.",
                ));
            }
            cancellation.check()?;
        }
        Ok(OwnedRunnerPreparation {
            target_already_active: target_match == Some(true) && !replaced,
            replaced,
        })
    }

    async fn spawn_owned_runner(
        &self,
        identity: &RunnerRuntimeIdentity,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        cancellation.check()?;
        let command = self.adapter.local_runner_command(&identity.runner_config)?;
        let target_key = runner_process_target_key(identity);
        let mut supervisor = self.supervisor.lock().await;
        cancellation.check()?;
        supervisor
            .spawn_owned_with_target(
                ProcessKind::LocalRunner,
                command,
                false,
                Some(target_key),
            )
            .await?;
        Ok(())
    }

    async fn spawn_owned(
        &self,
        kind: ProcessKind,
        command: std::process::Command,
        machine_stdout: bool,
        cancellation: &CancellationContext,
    ) -> DesktopResult<Option<MachineEventReceiver>> {
        cancellation.check()?;
        let mut supervisor = self.supervisor.lock().await;
        cancellation.check()?;
        supervisor.spawn_owned(kind, command, machine_stdout).await
    }

    async fn stop_process(&self, kind: ProcessKind) {
        self.supervisor.lock().await.stop(kind).await;
    }

    async fn stop_process_until(&self, kind: ProcessKind, deadline: Deadline) {
        self.supervisor
            .lock()
            .await
            .stop_until(kind, deadline)
            .await;
    }

    async fn wait_for_server(
        &mut self,
        server_url: &str,
        env_file: Option<&Path>,
        token_file: Option<&Path>,
        cancellation: &CancellationContext,
        deadline: Deadline,
        cleanup_owned_process: bool,
    ) -> DesktopResult<()> {
        loop {
            cancellation.check()?;
            if deadline.is_elapsed() {
                self.cleanup_readiness_process(
                    ProcessKind::LocalServer,
                    deadline,
                    cleanup_owned_process,
                )
                .await;
                return Err(readiness_timeout_error(
                    "server_unreachable",
                    "CodexLink Service did not become ready",
                    "Check the local Service diagnostics and retry.",
                ));
            }
            if let Some(process) = self.process_snapshot(ProcessKind::LocalServer).await {
                if matches!(process.phase, ProcessPhase::Exited | ProcessPhase::Failed) {
                    self.cleanup_readiness_process(
                        ProcessKind::LocalServer,
                        deadline,
                        cleanup_owned_process,
                    )
                    .await;
                    return Err(DesktopError::new(
                        "server_start_failed",
                        "The Desktop-owned CodexLink Server exited during startup",
                        "Open Activity for safe diagnostics and retry.",
                    ));
                }
            }
            if self
                .adapter
                .server_status_until(
                    Some(server_url),
                    env_file,
                    token_file,
                    cancellation,
                    deadline,
                )
                .await
                .is_ok_and(|status| status.http_reachable)
            {
                return Ok(());
            }
            cancellation.check()?;
            if deadline.is_elapsed() {
                self.cleanup_readiness_process(
                    ProcessKind::LocalServer,
                    deadline,
                    cleanup_owned_process,
                )
                .await;
                return Err(readiness_timeout_error(
                    "server_unreachable",
                    "CodexLink Service did not become ready",
                    "Check the local Service diagnostics and retry.",
                ));
            }
            sleep_or_cancel_until(POLL_INTERVAL, cancellation, deadline).await?;
        }
    }

    async fn wait_for_runner_identity(
        &mut self,
        identity: &RunnerRuntimeIdentity,
        cancellation: &CancellationContext,
        deadline: Deadline,
        cleanup_owned_process: bool,
    ) -> DesktopResult<()> {
        loop {
            cancellation.check()?;
            if deadline.is_elapsed() {
                self.cleanup_readiness_process(
                    ProcessKind::LocalRunner,
                    deadline,
                    cleanup_owned_process,
                )
                .await;
                return Err(readiness_timeout_error(
                    "runner_offline",
                    "Runner did not become connected",
                    "Check Server reachability and Runner diagnostics, then retry.",
                ));
            }
            if let Some(process) = self.process_snapshot(ProcessKind::LocalRunner).await {
                if matches!(process.phase, ProcessPhase::Exited | ProcessPhase::Failed) {
                    self.cleanup_readiness_process(
                        ProcessKind::LocalRunner,
                        deadline,
                        cleanup_owned_process,
                    )
                    .await;
                    return Err(DesktopError::new(
                        "runner_offline",
                        "The Desktop-owned Runner exited while connecting",
                        "Open Activity for safe diagnostics and retry.",
                    ));
                }
            }
            if self
                .adapter
                .runner_identity_ready_until(identity, cancellation, deadline)
                .await
                .unwrap_or(false)
            {
                return Ok(());
            }
            sleep_or_cancel_until(POLL_INTERVAL, cancellation, deadline).await?;
        }
    }

    async fn wait_for_runner(
        &mut self,
        identity: &ProjectRuntimeIdentity,
        cancellation: &CancellationContext,
        deadline: Deadline,
        cleanup_owned_process: bool,
    ) -> DesktopResult<()> {
        let runner = identity.runner_identity();
        self.wait_for_runner_identity(&runner, cancellation, deadline, cleanup_owned_process)
            .await
    }

    async fn wait_for_project(
        &mut self,
        identity: &ProjectRuntimeIdentity,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        let deadline = Deadline::after(PROJECT_READY_TIMEOUT);
        loop {
            cancellation.check()?;
            if deadline.is_elapsed() {
                return Err(project_not_loaded_error());
            }
            if self
                .adapter
                .project_ready_until(identity, cancellation, deadline)
                .await
                .unwrap_or(false)
            {
                return Ok(());
            }
            cancellation.check()?;
            if deadline.is_elapsed() {
                return Err(project_not_loaded_error());
            }
            sleep_or_cancel_until(POLL_INTERVAL, cancellation, deadline).await?;
        }
    }

    async fn cleanup_readiness_process(
        &self,
        kind: ProcessKind,
        deadline: Deadline,
        cleanup_owned_process: bool,
    ) {
        if cleanup_owned_process {
            self.stop_process_until(
                kind,
                Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
            )
            .await;
        }
    }

    async fn save_config(&self) -> DesktopResult<()> {
        tokio::fs::create_dir_all(&self.data_dir)
            .await
            .map_err(|_| {
                DesktopError::new(
                    "desktop_state_unavailable",
                    "Desktop cannot create its app-data directory",
                    "Check local filesystem permissions and retry.",
                )
            })?;
        let encoded = serde_json::to_vec_pretty(&self.config).map_err(|_| {
            DesktopError::new(
                "desktop_state_invalid",
                "Desktop could not encode its non-secret runtime state",
                "Retry the setup operation.",
            )
        })?;
        let config_path = self.config_path.clone();
        tokio::task::spawn_blocking(move || save_config_atomically(&config_path, &encoded))
            .await
            .map_err(|_| desktop_state_unavailable("Desktop state persistence worker stopped"))??;
        Ok(())
    }
}

async fn sleep_or_cancel_until(
    duration: Duration,
    cancellation: &CancellationContext,
    deadline: Deadline,
) -> DesktopResult<()> {
    cancellation.check()?;
    if deadline.is_elapsed() {
        return Ok(());
    }
    let wake_at = std::cmp::min(deadline.instant(), tokio::time::Instant::now() + duration);
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(cancelled_error()),
        _ = tokio::time::sleep_until(wake_at) => Ok(()),
    }
}

fn readiness_timeout_error(
    code: &'static str,
    message: &'static str,
    action: &'static str,
) -> DesktopError {
    DesktopError::new(code, message, action)
        .with_details(serde_json::json!({ "category": "readiness_timeout" }))
}

// These are Desktop packaging defaults, not process-environment overrides.
// The Server loads its env file only into keys that are absent from the process
// environment, so the effective precedence remains built-in defaults < this
// config file < explicit environment variables.
fn ensure_desktop_server_defaults(path: &Path) -> DesktopResult<()> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        desktop_state_unavailable("Desktop could not inspect its local Server configuration")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    if !metadata.is_file() || metadata.len() > DESKTOP_SERVER_ENV_MAX_BYTES {
        return Err(desktop_state_unavailable(
            "Desktop local Server configuration is not a bounded regular file",
        ));
    }
    let content = std::fs::read_to_string(path).map_err(|error| {
        desktop_state_unavailable("Desktop could not read its local Server configuration")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    let has_key = |key: &str| {
        content.lines().any(|line| {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line).trim();
            line.split_once('=')
                .is_some_and(|(candidate, _)| candidate.trim() == key)
        })
    };
    let mut additions = Vec::new();
    if !has_key("WEBCODEX_MCP_MODEL_SURFACE") {
        additions.push(format!(
            "WEBCODEX_MCP_MODEL_SURFACE={DESKTOP_MCP_MODEL_SURFACE}"
        ));
    }
    if !has_key("WEBCODEX_MCP_COMPACT_SCHEMAS") {
        additions.push(format!(
            "WEBCODEX_MCP_COMPACT_SCHEMAS={DESKTOP_MCP_COMPACT_SCHEMAS}"
        ));
    }
    if additions.is_empty() {
        return Ok(());
    }
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| {
            desktop_state_unavailable("Desktop could not update its local Server configuration")
                .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
        })?;
    if !content.is_empty() && !content.ends_with('\n') {
        file.write_all(b"\n").map_err(|error| {
            desktop_state_unavailable("Desktop could not update its local Server configuration")
                .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
        })?;
    }
    for addition in additions {
        writeln!(file, "{addition}").map_err(|error| {
            desktop_state_unavailable("Desktop could not update its local Server configuration")
                .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
        })?;
    }
    file.flush().map_err(|error| {
        desktop_state_unavailable("Desktop could not update its local Server configuration")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    Ok(())
}

fn machine_event_overflow_error(event: &Value) -> DesktopError {
    DesktopError::new(
        "machine_event_overflow",
        "Desktop could not retain every critical machine-readiness event",
        "Retry the operation and inspect Activity if the child keeps emitting excessive machine events.",
    )
    .with_details(serde_json::json!({
        "category": "machine_event_overflow",
        "dropped_critical": event
            .get("dropped_critical")
            .and_then(Value::as_u64)
            .unwrap_or(1),
    }))
}

fn local_enrollment_directory(data_dir: &Path, config: &StoredDesktopConfig) -> PathBuf {
    // Two reusable slots bound local credential storage while keeping login's
    // --overwrite away from the currently committed recovery identity. Resolve
    // native path aliases (notably /var on macOS) before comparing paths.
    let root = data_dir
        .canonicalize()
        .unwrap_or_else(|_| data_dir.to_path_buf())
        .join("connections");
    let first = root.join("a");
    let saved_runner = config
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.runner_config.as_ref());
    if saved_runner.is_some_and(|path| {
        path.canonicalize()
            .unwrap_or_else(|_| path.clone())
            .starts_with(&first)
    }) {
        root.join("b")
    } else {
        first
    }
}

fn project_snapshot(config: &StoredDesktopConfig) -> Option<ProjectSelection> {
    let remote_full = config.topology.as_ref().is_some_and(|topology| {
        topology.experience == Experience::Full
            && matches!(topology.server, ServerTopology::Remote { .. })
    });
    let mut project = if remote_full {
        config.project.clone().or_else(|| config.projects.first().cloned())
    } else {
        config.projects.first().cloned()
    }?;
    // Keep the customer-visible path selection, but never advertise a stale
    // runtime Project identity when the underlying Runner enrollment files are
    // missing or otherwise unusable.
    if runner_identity_from_config(config).is_none() {
        project.runtime_project_id = None;
    }
    Some(project)
}

fn runtime_identity_project(config: &StoredDesktopConfig) -> Option<&ProjectSelection> {
    if config.topology.as_ref().is_some_and(|topology| {
        topology.experience == Experience::Full && matches!(topology.server, ServerTopology::Local)
    }) {
        config.runtime_anchor.as_ref()
    } else {
        config.project.as_ref()
    }
}

fn project_paths_equal(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.trim_end_matches(['\\', '/'])
            .eq_ignore_ascii_case(right.trim_end_matches(['\\', '/']))
    } else {
        left.trim_end_matches('/').eq(right.trim_end_matches('/'))
    }
}

fn projects_snapshot(config: &StoredDesktopConfig) -> Vec<ProjectSelection> {
    config.projects.clone()
}

fn upsert_authorized_project(config: &mut StoredDesktopConfig, project: ProjectSelection) {
    if let Some(existing) = config
        .projects
        .iter_mut()
        .find(|existing| project_paths_equal(&existing.path, &project.path))
    {
        *existing = project;
    } else {
        config.projects.push(project);
    }
}

fn remove_authorized_project(
    config: &mut StoredDesktopConfig,
    project_path: &str,
) -> Option<ProjectSelection> {
    let index = config
        .projects
        .iter()
        .position(|project| project_paths_equal(&project.path, project_path))?;
    Some(config.projects.remove(index))
}

fn migrate_legacy_authorized_projects(
    config: &mut StoredDesktopConfig,
    default_project_dir: &Path,
    resource_dir: &Path,
) {
    let local_full = config.topology.as_ref().is_some_and(|topology| {
        topology.experience == Experience::Full && matches!(topology.server, ServerTopology::Local)
    });
    if !local_full {
        return;
    }

    if config.projects.is_empty() {
        if let Some(project) = config.project.clone() {
            let path = project.path.trim();
            let looks_internal = path.is_empty()
                || project_paths_equal(path, default_project_dir.to_string_lossy().as_ref())
                || project_paths_equal(path, resource_dir.to_string_lossy().as_ref());
            if !looks_internal {
                config.projects.push(project);
            }
        }
    }

    // `project` was overloaded by older Desktop builds as both the Runner's
    // recovery identity and the customer-visible authorization. A local runtime
    // now owns a dedicated app-data anchor, so force one clean re-enrollment and
    // stop serializing the legacy field for the local topology.
    if config.runtime_anchor.is_none() {
        config.project = None;
    }
}

fn customer_authorized_anchor_path(config: &StoredDesktopConfig) -> Option<String> {
    config
        .projects
        .first()
        .map(|project| project.path.clone())
        .or_else(|| config.project.as_ref().map(|project| project.path.clone()))
}

fn stored_runner_client_id(config: &StoredDesktopConfig) -> Option<String> {
    let runtime = config.runtime.as_ref()?;
    if let Some(client_id) = runtime
        .runner_client_id
        .as_deref()
        .map(str::trim)
        .filter(|client_id| !client_id.is_empty())
    {
        return Some(client_id.to_string());
    }
    // Pre-migration Desktop state did not persist client_id separately. Recover
    // it from the exact runtime Project identity instead of accepting whatever
    // client_id happens to be present in runner.toml during the first upgrade.
    let project_id = runtime.project_id.as_deref()?.trim();
    let runtime_project_id = runtime.runtime_project_id.as_deref()?.trim();
    if project_id.is_empty() || runtime_project_id.is_empty() {
        return None;
    }
    let suffix = format!(":{project_id}");
    runtime_project_id
        .strip_prefix("agent:")?
        .strip_suffix(&suffix)
        .map(str::trim)
        .filter(|client_id| !client_id.is_empty())
        .map(str::to_string)
}

fn local_runtime_requires_zero_root_reenrollment(config: &StoredDesktopConfig) -> bool {
    let local_full = config.topology.as_ref().is_some_and(|topology| {
        topology.experience == Experience::Full && matches!(topology.server, ServerTopology::Local)
    });
    if !local_full {
        return false;
    }
    // Pre-zero-root Local builds stored the internal runtime anchor as a real
    // runtime Project and persisted its ids in StoredRuntime. Force exactly one
    // managed re-enrollment so that stale anchor authority is not reused.
    config.runtime_anchor.is_none()
        || config
            .runtime_anchor
            .as_ref()
            .and_then(|anchor| anchor.runtime_project_id.as_deref())
            .is_some_and(|value| !value.trim().is_empty())
        || config.runtime.as_ref().is_some_and(|runtime| {
            runtime
                .project_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                || runtime
                    .runtime_project_id
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
        })
}

fn runner_identity_from_config(config: &StoredDesktopConfig) -> Option<RunnerRuntimeIdentity> {
    let runtime = config.runtime.as_ref()?;
    let runner_config = runtime.runner_config.clone()?;
    let user_token_file = runtime.user_token_file.clone()?;
    if runtime.server_url.trim().is_empty()
        || !runner_config.is_file()
        || !user_token_file.is_file()
    {
        return None;
    }
    Some(RunnerRuntimeIdentity {
        runner_config,
        user_token_file,
        server_url: runtime.server_url.clone(),
    })
}

fn customer_project_identity_from_config(
    config: &StoredDesktopConfig,
) -> Option<ProjectRuntimeIdentity> {
    let runner = runner_identity_from_config(config)?;
    let project = project_snapshot(config)?;
    let runtime_project_id = project.runtime_project_id.clone()?.trim().to_string();
    if runtime_project_id.is_empty() {
        return None;
    }
    let project_id = runtime_project_id
        .rsplit(':')
        .next()
        .unwrap_or(runtime_project_id.as_str())
        .to_string();
    Some(ProjectRuntimeIdentity {
        project_id,
        runtime_project_id,
        project_path: project.path,
        runner_config: runner.runner_config,
        user_token_file: runner.user_token_file,
        server_url: runner.server_url,
    })
}

#[cfg(test)]
fn identity_from_config(config: &StoredDesktopConfig) -> Option<ProjectRuntimeIdentity> {
    let runtime = config.runtime.as_ref()?;
    let project = runtime_identity_project(config)?;
    let runner_config = runtime.runner_config.clone()?;
    let user_token_file = runtime.user_token_file.clone()?;
    let project_id = runtime.project_id.clone()?.trim().to_string();
    let runtime_project_id = runtime.runtime_project_id.clone()?.trim().to_string();
    if project_id.is_empty()
        || runtime_project_id.is_empty()
        || !runner_config.is_file()
        || !user_token_file.is_file()
    {
        return None;
    }
    Some(ProjectRuntimeIdentity {
        project_id,
        runtime_project_id,
        project_path: project.path.clone(),
        runner_config,
        user_token_file,
        server_url: runtime.server_url.clone(),
    })
}

#[derive(Debug)]
enum StoredConfigFile {
    Missing,
    Valid {
        config: StoredDesktopConfig,
        bytes: Vec<u8>,
    },
    Corrupt,
}

fn load_config(path: &Path, activity: &ActivityLog) -> DesktopResult<StoredDesktopConfig> {
    let backup_path = desktop_state_backup_path(path);
    match read_stored_config(path)? {
        StoredConfigFile::Valid { config, .. } => Ok(config),
        StoredConfigFile::Missing => match read_stored_config(&backup_path)? {
            StoredConfigFile::Missing => Ok(StoredDesktopConfig::default()),
            StoredConfigFile::Valid { config, bytes } => {
                recover_config_from_backup(path, &bytes, activity)?;
                Ok(config)
            }
            StoredConfigFile::Corrupt => Err(desktop_state_corrupt()),
        },
        StoredConfigFile::Corrupt => match read_stored_config(&backup_path)? {
            StoredConfigFile::Valid { config, bytes } => {
                recover_config_from_backup(path, &bytes, activity)?;
                Ok(config)
            }
            StoredConfigFile::Missing | StoredConfigFile::Corrupt => Err(desktop_state_corrupt()),
        },
    }
}

fn recover_config_from_backup(
    primary_path: &Path,
    bytes: &[u8],
    activity: &ActivityLog,
) -> DesktopResult<()> {
    write_atomic_file(primary_path, bytes).map_err(|error| {
        desktop_state_unavailable("Desktop could not restore the previous known-good state")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    activity.push(
        ActivityEventKind::StateRecovered,
        "desktop_state",
        ActivityLevel::Warning,
        "Recovered Desktop state from the previous known-good snapshot",
    );
    Ok(())
}

fn read_stored_config(path: &Path) -> DesktopResult<StoredConfigFile> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(StoredConfigFile::Missing)
        }
        Err(error) => {
            return Err(
                desktop_state_unavailable("Desktop could not inspect its saved state")
                    .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) })),
            )
        }
    };
    if !metadata.is_file() || metadata.len() > DESKTOP_STATE_MAX_BYTES {
        return Ok(StoredConfigFile::Corrupt);
    }
    let bytes = std::fs::read(path).map_err(|error| {
        desktop_state_unavailable("Desktop could not read its saved state")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    match serde_json::from_slice::<StoredDesktopConfig>(&bytes) {
        Ok(config) => Ok(StoredConfigFile::Valid { config, bytes }),
        Err(_) => Ok(StoredConfigFile::Corrupt),
    }
}

fn save_config_atomically(path: &Path, encoded: &[u8]) -> DesktopResult<()> {
    if encoded.len() as u64 > DESKTOP_STATE_MAX_BYTES {
        return Err(DesktopError::new(
            "desktop_state_invalid",
            "Desktop state exceeded its bounded persistence size",
            "Retry after reducing the saved Desktop configuration.",
        ));
    }

    if let StoredConfigFile::Valid { bytes, .. } = read_stored_config(path)? {
        let backup = desktop_state_backup_path(path);
        write_atomic_file(&backup, &bytes).map_err(|error| {
            desktop_state_unavailable("Desktop could not preserve the previous known-good state")
                .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
        })?;
    }

    write_atomic_file(path, encoded).map_err(|error| {
        desktop_state_unavailable("Desktop could not persist its non-secret runtime state")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })
}

fn desktop_state_backup_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("desktop-state.json");
    path.with_file_name(format!("{file_name}.bak"))
}

fn state_temp_path(path: &Path) -> PathBuf {
    let id = NEXT_STATE_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("desktop-state.json");
    path.with_file_name(format!(".{file_name}.{}.{}.tmp", std::process::id(), id))
}

pub(crate) fn write_atomic_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_file_with_hook(path, bytes, |_| Ok(()))
}

fn write_atomic_file_with_hook<F>(path: &Path, bytes: &[u8], before_replace: F) -> io::Result<()>
where
    F: FnOnce(&Path) -> io::Result<()>,
{
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temp_path = state_temp_path(path);
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp_path)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        before_replace(&temp_path)?;
        atomic_replace(&temp_path, path)?;
        sync_state_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_state_directory(path: &Path) -> io::Result<()> {
    let directory = File::open(path)?;
    match directory.sync_all() {
        Ok(()) => Ok(()),
        Err(error)
            if error.kind() == io::ErrorKind::Unsupported
                || error.raw_os_error() == Some(libc::EINVAL) =>
        {
            // Some Unix filesystems (notably macOS variants) do not support
            // directory fsync. The file itself has already been synced and the
            // same-directory rename is atomic, so treat this specific platform
            // limitation as best-effort durability rather than a false save
            // failure after replacement has already succeeded.
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
fn sync_state_directory(_path: &Path) -> io::Result<()> {
    // Windows uses MOVEFILE_WRITE_THROUGH for the replacement. Opening a
    // directory for FlushFileBuffers would require broader sharing semantics
    // than the app-data policy needs here.
    Ok(())
}

fn desktop_state_corrupt() -> DesktopError {
    DesktopError::new(
        "desktop_state_corrupt",
        "Desktop saved state is corrupt and no valid recovery snapshot is available",
        "Restore or remove the Desktop state files explicitly, then restart CodexLink.",
    )
    .with_details(serde_json::json!({ "category": "state_corrupt" }))
}

fn desktop_state_unavailable(message: &'static str) -> DesktopError {
    DesktopError::new(
        "desktop_state_unavailable",
        message,
        "Check local app-data permissions and retry.",
    )
}

fn reserve_loopback_address() -> DesktopResult<String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| {
        DesktopError::new(
            "local_port_unavailable",
            "Desktop could not reserve a loopback port for CodexLink",
            "Check local networking and retry.",
        )
    })?;
    let address = listener.local_addr().map_err(|_| {
        DesktopError::new(
            "local_port_unavailable",
            "Desktop could not inspect the reserved loopback port",
            "Retry setup.",
        )
    })?;
    Ok(address.to_string())
}

fn regular_tunnel_exposure(
    state: &mut Option<RegularTunnelState>,
    process_active: bool,
) -> Option<ExposureReadiness> {
    let state = state.as_mut()?;
    if !process_active {
        state.status = RegularTunnelStatus::Error;
        state.ready_for_chatgpt = false;
        return Some(ExposureReadiness::Error);
    }
    Some(match state.status {
        RegularTunnelStatus::Starting => ExposureReadiness::Starting,
        RegularTunnelStatus::Ready if state.ready_for_chatgpt => ExposureReadiness::LocalReady,
        RegularTunnelStatus::Ready => ExposureReadiness::Degraded,
        RegularTunnelStatus::Error => ExposureReadiness::Error,
    })
}

fn apply_regular_tunnel_next_action(
    snapshot: &mut DesktopStateSnapshot,
    exposure: &ExposureReadiness,
) {
    if !snapshot.readiness.runtime_ready {
        return;
    }
    match exposure {
        ExposureReadiness::Error => {
            snapshot.readiness.next_action_kind =
                Some(ReadinessNextActionKind::RestartSecureTunnel);
            snapshot.readiness.next_action = Some("Restart the secure tunnel.".to_string());
        }
        ExposureReadiness::LocalReady => {
            snapshot.readiness.summary_kind = ReadinessSummaryKind::TunnelReadyWaitingForChatGpt;
            snapshot.readiness.summary =
                "OpenAI Secure Tunnel is ready; waiting for ChatGPT to connect".to_string();
            snapshot.readiness.next_action_kind = Some(ReadinessNextActionKind::CheckConnection);
            snapshot.readiness.next_action = Some(
                "Connect the Tunnel in ChatGPT, then verify it with one real project read."
                    .to_string(),
            );
        }
        ExposureReadiness::Degraded => {
            snapshot.readiness.next_action_kind =
                Some(ReadinessNextActionKind::RestoreClipboardHandoff);
            snapshot.readiness.next_action = Some(
                "Restore clipboard access, then restart the secure tunnel handoff.".to_string(),
            );
        }
        _ => {}
    }
}

fn effective_topology(
    configured: Option<&RuntimeTopology>,
    regular_tunnel_selected: bool,
) -> Option<RuntimeTopology> {
    let mut topology = configured.cloned()?;
    if regular_tunnel_selected
        && topology.experience == Experience::Full
        && matches!(topology.server, ServerTopology::Local)
    {
        topology.exposure = Exposure::OpenAiTunnel;
    }
    Some(topology)
}

fn exposure_readiness(topology: Option<&RuntimeTopology>) -> ExposureReadiness {
    match topology.map(|topology| &topology.exposure) {
        Some(Exposure::None) => ExposureReadiness::LocalReady,
        // An HTTPS origin is a configured route, not evidence that the MCP
        // endpoint plus ChatGPT authentication/handoff is externally usable.
        Some(Exposure::ExistingHttps { .. }) => ExposureReadiness::Unknown,
        Some(Exposure::Cloudflare | Exposure::OpenAiTunnel) => ExposureReadiness::Unknown,
        None => ExposureReadiness::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EffectiveTunnelProxy {
    url: Option<String>,
    source: &'static str,
    detected_url: Option<String>,
}

fn validate_tunnel_proxy_url(value: &str) -> DesktopResult<String> {
    crate::platform::normalize_proxy_server(value).ok_or_else(|| {
        DesktopError::new(
            "tunnel_proxy_invalid",
            "The Tunnel proxy must be an HTTP or HTTPS proxy URL without embedded credentials",
            "Use a value such as http://127.0.0.1:7890, or choose automatic proxy detection.",
        )
    })
}

fn environment_tunnel_proxies() -> Vec<String> {
    let mut proxies = Vec::new();
    for name in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        let Some(url) = std::env::var(name)
            .ok()
            .and_then(|value| crate::platform::normalize_proxy_server(&value))
        else {
            continue;
        };
        if !proxies.iter().any(|existing| existing == &url) {
            proxies.push(url);
        }
    }
    proxies
}

fn push_tunnel_proxy_candidate(
    candidates: &mut Vec<EffectiveTunnelProxy>,
    candidate: EffectiveTunnelProxy,
) {
    if candidates
        .iter()
        .any(|existing| existing.url == candidate.url)
    {
        return;
    }
    candidates.push(candidate);
}

fn auto_tunnel_proxy_candidates_from(
    environment: Vec<String>,
    system: Option<crate::platform::SystemProxyCandidate>,
) -> Vec<EffectiveTunnelProxy> {
    let detected_url = system.as_ref().map(|candidate| candidate.url.clone());
    let mut candidates = Vec::new();
    for url in environment {
        push_tunnel_proxy_candidate(
            &mut candidates,
            EffectiveTunnelProxy {
                url: Some(url),
                source: "environment",
                detected_url: detected_url.clone(),
            },
        );
    }
    if let Some(candidate) = system {
        if candidate.enabled || crate::platform::proxy_is_loopback(&candidate.url) {
            push_tunnel_proxy_candidate(
                &mut candidates,
                EffectiveTunnelProxy {
                    url: Some(candidate.url),
                    source: if candidate.enabled {
                        "windows_system"
                    } else {
                        "windows_loopback_candidate"
                    },
                    detected_url: detected_url.clone(),
                },
            );
        }
    }
    push_tunnel_proxy_candidate(
        &mut candidates,
        EffectiveTunnelProxy {
            url: None,
            source: "direct",
            detected_url,
        },
    );
    candidates
}

fn tunnel_proxy_candidates(config: &TunnelProxyConfig) -> DesktopResult<Vec<EffectiveTunnelProxy>> {
    let system = crate::platform::system_http_proxy_candidate();
    let detected_url = system.as_ref().map(|candidate| candidate.url.clone());
    match config.mode {
        TunnelProxyMode::Direct => Ok(vec![EffectiveTunnelProxy {
            url: None,
            source: "direct",
            detected_url,
        }]),
        TunnelProxyMode::Custom => Ok(vec![EffectiveTunnelProxy {
            url: Some(validate_tunnel_proxy_url(
                config.custom_url.as_deref().unwrap_or(""),
            )?),
            source: "custom",
            detected_url,
        }]),
        TunnelProxyMode::Auto => Ok(auto_tunnel_proxy_candidates_from(
            environment_tunnel_proxies(),
            system,
        )),
    }
}

fn effective_tunnel_proxy(config: &TunnelProxyConfig) -> DesktopResult<EffectiveTunnelProxy> {
    tunnel_proxy_candidates(config)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            DesktopError::new(
                "tunnel_proxy_unavailable",
                "No Tunnel network route is available",
                "Open Tunnel network settings and retry.",
            )
        })
}

async fn first_reachable_tunnel_proxy<F, Fut>(
    candidates: Vec<EffectiveTunnelProxy>,
    mut probe: F,
) -> Option<EffectiveTunnelProxy>
where
    F: FnMut(EffectiveTunnelProxy) -> Fut,
    Fut: Future<Output = bool>,
{
    for candidate in candidates {
        if probe(candidate.clone()).await {
            return Some(candidate);
        }
    }
    None
}

async fn tunnel_proxy_candidate_reaches_control_plane(candidate: EffectiveTunnelProxy) -> bool {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(TUNNEL_NETWORK_PROBE_TIMEOUT)
        .timeout(TUNNEL_NETWORK_PROBE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());
    if let Some(url) = candidate.url.as_deref() {
        let Ok(proxy) = reqwest::Proxy::all(url) else {
            return false;
        };
        builder = builder.proxy(proxy);
    } else {
        builder = builder.no_proxy();
    }
    let Ok(client) = builder.build() else {
        return false;
    };
    matches!(
        tokio::time::timeout(
            TUNNEL_NETWORK_PROBE_TIMEOUT,
            client.get(OPENAI_CONTROL_PLANE_PROBE_URL).send(),
        )
        .await,
        Ok(Ok(response)) if response.status().as_u16() < 500
    )
}

async fn resolve_tunnel_proxy_for_start(
    config: &TunnelProxyConfig,
    cancellation: &CancellationContext,
) -> DesktopResult<EffectiveTunnelProxy> {
    let candidates = tunnel_proxy_candidates(config)?;
    if config.mode != TunnelProxyMode::Auto {
        return candidates.into_iter().next().ok_or_else(|| {
            DesktopError::new(
                "tunnel_proxy_unavailable",
                "No Tunnel network route is available",
                "Open Tunnel network settings and retry.",
            )
        });
    }
    cancellation.check()?;
    let attempted_routes = candidates
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "source": candidate.source,
                "url": candidate.url.as_deref().unwrap_or("direct"),
            })
        })
        .collect::<Vec<_>>();
    let selected = first_reachable_tunnel_proxy(candidates, |candidate| async move {
        tunnel_proxy_candidate_reaches_control_plane(candidate).await
    })
    .await;
    cancellation.check()?;
    selected.ok_or_else(|| {
        DesktopError::new(
            "tunnel_proxy_unavailable",
            "Automatic Tunnel networking could not reach the OpenAI control plane",
            "Open Tunnel network settings to choose a custom proxy or direct connection, then retry.",
        )
        .with_details(serde_json::json!({
            "category": "control_plane_unreachable",
            "probe_host": "api.openai.com",
            "attempted_routes": attempted_routes,
        }))
    })
}

fn apply_resolved_tunnel_proxy_projection(
    snapshot: &mut DesktopStateSnapshot,
    config: &StoredDesktopConfig,
    proxy: &EffectiveTunnelProxy,
) {
    snapshot.tunnel_proxy = TunnelProxySnapshot {
        mode: config.tunnel_proxy.mode,
        custom_url: config.tunnel_proxy.custom_url.clone(),
        effective_source: proxy.source.to_string(),
        effective_url: proxy.url.clone(),
        detected_url: proxy.detected_url.clone(),
    };
}

fn runtime_autostart(config: &StoredDesktopConfig) -> bool {
    config.runtime_autostart.unwrap_or_else(|| {
        config.runtime.is_some()
            && config
                .topology
                .as_ref()
                .is_some_and(|topology| topology.experience == Experience::Full)
    })
}

fn default_management_project_dir(data_dir: &Path, resource_dir: &Path) -> PathBuf {
    let _ = resource_dir;
    // The runtime anchor is product-internal state, not customer-authorized
    // source access. Keep it stable under per-user app data so install/upgrade
    // locations can never leak into the Projects authorization surface.
    data_dir.join("runtime-anchor")
}

fn preferred_connection(
    config: &StoredDesktopConfig,
    tunnel_configured: bool,
) -> RegularConnectionPreference {
    if let Some(explicit) = config.preferred_connection {
        return explicit;
    }
    let local_full_runtime = config.runtime.is_some()
        && config.topology.as_ref().is_some_and(|topology| {
            topology.experience == Experience::Full
                && matches!(topology.server, ServerTopology::Local)
        });
    if tunnel_configured && local_full_runtime {
        RegularConnectionPreference::OpenAiTunnel
    } else {
        RegularConnectionPreference::NoChatGpt
    }
}

fn apply_config_projection(snapshot: &mut DesktopStateSnapshot, config: &StoredDesktopConfig) {
    snapshot.runtime_autostart = runtime_autostart(config);
    snapshot.preferred_connection = preferred_connection(config, snapshot.openai_tunnel_configured);
    snapshot.project = project_snapshot(config);
    snapshot.projects = projects_snapshot(config);
    snapshot.tunnel_proxy = if config.tunnel_proxy.mode == TunnelProxyMode::Auto {
        TunnelProxySnapshot {
            mode: TunnelProxyMode::Auto,
            custom_url: None,
            effective_source: "auto_pending".to_string(),
            effective_url: None,
            detected_url: crate::platform::system_http_proxy_candidate()
                .map(|candidate| candidate.url),
        }
    } else {
        match effective_tunnel_proxy(&config.tunnel_proxy) {
            Ok(proxy) => TunnelProxySnapshot {
                mode: config.tunnel_proxy.mode,
                custom_url: config.tunnel_proxy.custom_url.clone(),
                effective_source: proxy.source.to_string(),
                effective_url: proxy.url,
                detected_url: proxy.detected_url,
            },
            Err(_) => TunnelProxySnapshot {
                mode: config.tunnel_proxy.mode,
                custom_url: config.tunnel_proxy.custom_url.clone(),
                effective_source: "invalid_custom".to_string(),
                effective_url: None,
                detected_url: crate::platform::system_http_proxy_candidate()
                    .map(|candidate| candidate.url),
            },
        }
    };
}

fn apply_openai_tunnel_configuration(snapshot: &mut DesktopStateSnapshot, config: &TunnelConfig) {
    let configuration = config.snapshot();
    snapshot.openai_tunnel_configured = configuration.is_configured();
    snapshot.openai_tunnel_config = configuration;
}

fn same_server(left: &str, right: &str) -> bool {
    left.trim_end_matches('/')
        .eq_ignore_ascii_case(right.trim_end_matches('/'))
}

#[cfg(test)]
fn same_project(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_management_project_uses_safe_platform_location() {
        let data = PathBuf::from(r"C:\Users\test\AppData\Local\CodexLink");
        let resources = PathBuf::from(r"D:\Apps\CodexLink Desktop");
        let project = default_management_project_dir(&data, &resources);
        assert_eq!(project, data.join("runtime-anchor"));
    }

    #[test]
    fn local_runtime_anchor_is_metadata_only_and_customer_authority_stays_in_projects() {
        let mut config = StoredDesktopConfig::default();
        config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        config.runtime_anchor = Some(ProjectSelection {
            path: r"C:\Users\test\AppData\Local\CodexLink\runtime-anchor".to_string(),
            allowed_root: r"C:\Users\test\AppData\Local\CodexLink\runtime-anchor".to_string(),
            is_git_repository: false,
            runtime_project_id: None,
        });
        config.project = None;
        config.projects = vec![ProjectSelection {
            path: r"D:\customer".to_string(),
            allowed_root: r"D:\customer".to_string(),
            is_git_repository: true,
            runtime_project_id: Some("agent:desktop:customer".to_string()),
        }];

        let anchor = runtime_identity_project(&config).expect("runtime anchor metadata");
        assert_eq!(
            anchor.path,
            r"C:\Users\test\AppData\Local\CodexLink\runtime-anchor"
        );
        assert!(anchor.runtime_project_id.is_none());
        assert_eq!(projects_snapshot(&config)[0].path, r"D:\customer");
        assert!(!local_runtime_requires_zero_root_reenrollment(&config));

        config
            .runtime_anchor
            .as_mut()
            .expect("runtime anchor")
            .runtime_project_id = Some("agent:desktop:legacy-internal".to_string());
        assert!(local_runtime_requires_zero_root_reenrollment(&config));
    }

    #[test]
    fn desktop_server_defaults_append_missing_values_and_preserve_explicit_config() {
        let dir = unique_state_dir("server-defaults-explicit");
        std::fs::create_dir_all(&dir).unwrap();
        let env_file = dir.join("webcodex.env");
        std::fs::write(
            &env_file,
            "WEBCODEX_TOKEN=secret\nWEBCODEX_MCP_COMPACT_SCHEMAS=false\n",
        )
        .unwrap();

        ensure_desktop_server_defaults(&env_file).unwrap();
        let once = std::fs::read_to_string(&env_file).unwrap();
        assert!(once.contains("WEBCODEX_TOKEN=secret\n"));
        assert!(once.contains("WEBCODEX_MCP_COMPACT_SCHEMAS=false\n"));
        assert!(once.contains("WEBCODEX_MCP_MODEL_SURFACE=adaptive-runtime-v1\n"));
        assert_eq!(once.matches("WEBCODEX_MCP_COMPACT_SCHEMAS=").count(), 1);

        ensure_desktop_server_defaults(&env_file).unwrap();
        assert_eq!(std::fs::read_to_string(&env_file).unwrap(), once);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn desktop_server_defaults_add_both_values_to_fresh_server_env() {
        let dir = unique_state_dir("server-defaults-fresh");
        std::fs::create_dir_all(&dir).unwrap();
        let env_file = dir.join("webcodex.env");
        std::fs::write(&env_file, "WEBCODEX_ADDR=127.0.0.1:12345").unwrap();

        ensure_desktop_server_defaults(&env_file).unwrap();
        let content = std::fs::read_to_string(&env_file).unwrap();
        assert!(content.starts_with("WEBCODEX_ADDR=127.0.0.1:12345\n"));
        assert!(content.contains("WEBCODEX_MCP_MODEL_SURFACE=adaptive-runtime-v1\n"));
        assert!(content.contains("WEBCODEX_MCP_COMPACT_SCHEMAS=true\n"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn unique_state_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "webcodex-desktop-state-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn test_stored_config(label: &str) -> StoredDesktopConfig {
        StoredDesktopConfig {
            topology: None,
            runtime_anchor: None,
            project: Some(ProjectSelection {
                path: format!("/{label}"),
                allowed_root: "/".to_string(),
                is_git_repository: false,
                runtime_project_id: None,
            }),
            projects: vec![ProjectSelection {
                path: format!("/{label}"),
                allowed_root: "/".to_string(),
                is_git_repository: false,
                runtime_project_id: None,
            }],
            runtime_autostart: None,
            preferred_connection: None,
            openai_tunnel_id: None,
            tunnel_proxy: TunnelProxyConfig::default(),
            runtime: None,
        }
    }

    #[test]
    fn chatgpt_activity_observation_is_fenced_to_the_captured_project_identity() {
        let data_dir = unique_state_dir("chatgpt-activity-fence");
        let resource_dir = data_dir.join("resources");
        std::fs::create_dir_all(&resource_dir).unwrap();
        let runner_config = data_dir.join("runner.toml");
        let user_token_file = data_dir.join("user-token");
        std::fs::write(&runner_config, "fixture").unwrap();
        std::fs::write(&user_token_file, "fixture").unwrap();

        let mut core = DesktopCore::new(data_dir.clone(), resource_dir).unwrap();
        core.snapshot.readiness.runtime_ready = true;
        core.config.projects = vec![ProjectSelection {
            path: data_dir.join("project-b").to_string_lossy().to_string(),
            allowed_root: data_dir.to_string_lossy().to_string(),
            is_git_repository: false,
            runtime_project_id: Some("agent:desktop:project-b".to_string()),
        }];
        core.config.project = None;
        core.config.runtime = Some(StoredRuntime {
            server_url: "http://127.0.0.1:8080".to_string(),
            server_env_file: None,
            runner_config: Some(runner_config),
            user_token_file: Some(user_token_file),
            runner_client_id: Some("desktop".to_string()),
            project_id: None,
            runtime_project_id: None,
        });

        let current_identity = customer_project_identity_from_config(&core.config)
            .expect("current customer project identity");
        let mut stale_identity = current_identity.clone();
        stale_identity.project_id = "project-a".to_string();
        stale_identity.runtime_project_id = "agent:desktop:project-a".to_string();

        let stale = core
            .apply_chatgpt_activity_observation(&stale_identity, Some(1234))
            .unwrap();
        assert!(
            stale.chatgpt_activity.is_none(),
            "an observation captured for the old Project must not cross a Project switch"
        );

        let current = core
            .apply_chatgpt_activity_observation(&current_identity, Some(5678))
            .unwrap();
        assert_eq!(
            current
                .chatgpt_activity
                .as_ref()
                .and_then(|activity| activity.last_meaningful_activity_at_ms),
            Some(5678)
        );
        assert!(current.chatgpt_activity.unwrap().observed);
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn staging_a_project_scope_clears_prior_chatgpt_evidence() {
        let data_dir = unique_state_dir("chatgpt-project-scope-reset");
        let resource_dir = data_dir.join("resources");
        std::fs::create_dir_all(&resource_dir).unwrap();
        let mut core = DesktopCore::new(data_dir.clone(), resource_dir).unwrap();
        core.snapshot.chatgpt_activity = Some(ChatGptActivitySnapshot {
            observed: true,
            last_meaningful_activity_at_ms: Some(1234),
        });

        let project = ProjectSelection {
            path: data_dir.join("project-b").to_string_lossy().to_string(),
            allowed_root: data_dir.to_string_lossy().to_string(),
            is_git_repository: false,
            runtime_project_id: None,
        };
        core.stage_project_scope(project.clone());

        assert_eq!(core.snapshot.project, Some(project));
        assert!(core.snapshot.chatgpt_activity.is_none());
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn legacy_project_refresh_never_claims_external_or_inactive_runner_ownership() {
        assert!(!can_refresh_legacy_runner(None));
        assert!(!can_refresh_legacy_runner(Some(
            crate::process::ProcessSnapshot {
                kind: ProcessKind::LocalRunner,
                phase: ProcessPhase::Running,
                pid: Some(42),
                exit_code: None,
                owned_by_desktop: false,
            }
        )));
        assert!(!can_refresh_legacy_runner(Some(
            crate::process::ProcessSnapshot {
                kind: ProcessKind::LocalRunner,
                phase: ProcessPhase::Exited,
                pid: Some(43),
                exit_code: Some(0),
                owned_by_desktop: true,
            }
        )));
        assert!(can_refresh_legacy_runner(Some(
            crate::process::ProcessSnapshot {
                kind: ProcessKind::LocalRunner,
                phase: ProcessPhase::Running,
                pid: Some(44),
                exit_code: None,
                owned_by_desktop: true,
            }
        )));
    }

    #[test]
    fn legacy_runtime_project_identity_recovers_runner_client_id_without_project_coupling() {
        let mut config = test_stored_config("legacy");
        config.runtime = Some(StoredRuntime {
            server_url: "https://example.test".to_string(),
            server_env_file: None,
            runner_config: None,
            user_token_file: None,
            runner_client_id: None,
            project_id: Some("repo".to_string()),
            runtime_project_id: Some("agent:desktop:runner:repo".to_string()),
        });
        assert_eq!(
            stored_runner_client_id(&config).as_deref(),
            Some("desktop:runner")
        );
        config.runtime.as_mut().unwrap().runner_client_id = Some("explicit-runner".to_string());
        assert_eq!(
            stored_runner_client_id(&config).as_deref(),
            Some("explicit-runner")
        );
    }

    #[test]
    fn local_enrollment_preserves_saved_files_and_reuses_only_the_inactive_slot() {
        let dir = unique_state_dir("enrollment-slots");
        std::fs::create_dir_all(&dir).unwrap();
        let mut config = test_stored_config("previous");
        let first = local_enrollment_directory(&dir, &config);
        assert!(first.ends_with(std::path::Path::new("connections").join("a")));
        assert!(!first.to_string_lossy().contains("local-connections"));
        let saved_runner = first.join("server").join("runner.toml");
        std::fs::create_dir_all(saved_runner.parent().unwrap()).unwrap();
        std::fs::write(&saved_runner, "previous project fixture").unwrap();
        config.runtime = Some(StoredRuntime {
            server_url: "http://127.0.0.1:7890".into(),
            server_env_file: None,
            runner_config: Some(saved_runner.clone()),
            user_token_file: None,
            runner_client_id: None,
            project_id: None,
            runtime_project_id: None,
        });

        let candidate = local_enrollment_directory(&dir, &config);
        assert_ne!(candidate, first);
        let candidate_runner = candidate.join("server").join("runner.toml");
        std::fs::create_dir_all(candidate_runner.parent().unwrap()).unwrap();
        std::fs::write(&candidate_runner, "replacement project fixture").unwrap();
        assert_eq!(
            std::fs::read_to_string(saved_runner).unwrap(),
            "previous project fixture"
        );
        // A failed activation retries the candidate, never the saved connection.
        assert_eq!(local_enrollment_directory(&dir, &config), candidate);
        config.runtime.as_mut().unwrap().runner_config = Some(candidate_runner);
        assert_eq!(local_enrollment_directory(&dir, &config), first);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn atomic_save_interruption_keeps_prior_valid_state() {
        let dir = unique_state_dir("interrupted-save");
        std::fs::create_dir_all(&dir).expect("create state fixture dir");
        let path = dir.join("desktop-state.json");
        let previous = test_stored_config("previous");
        let replacement = test_stored_config("replacement");
        let previous_bytes = serde_json::to_vec_pretty(&previous).unwrap();
        let replacement_bytes = serde_json::to_vec_pretty(&replacement).unwrap();
        write_atomic_file(&path, &previous_bytes).expect("write previous state");

        let interrupted = write_atomic_file_with_hook(&path, &replacement_bytes, |_| {
            Err(io::Error::other("injected interruption before replace"))
        });
        assert!(interrupted.is_err());
        match read_stored_config(&path).expect("read state after interruption") {
            StoredConfigFile::Valid { config, .. } => assert_eq!(config, previous),
            other => panic!("previous state was not preserved: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn atomic_save_preserves_previous_known_good_backup() {
        let dir = unique_state_dir("known-good-backup");
        std::fs::create_dir_all(&dir).expect("create state fixture dir");
        let path = dir.join("desktop-state.json");
        let previous = test_stored_config("previous");
        let replacement = test_stored_config("replacement");
        save_config_atomically(&path, &serde_json::to_vec_pretty(&previous).unwrap())
            .expect("initial atomic save");
        save_config_atomically(&path, &serde_json::to_vec_pretty(&replacement).unwrap())
            .expect("replacement atomic save");

        match read_stored_config(&path).expect("read primary") {
            StoredConfigFile::Valid { config, .. } => assert_eq!(config, replacement),
            other => panic!("replacement state was not valid: {other:?}"),
        }
        match read_stored_config(&desktop_state_backup_path(&path)).expect("read backup") {
            StoredConfigFile::Valid { config, .. } => assert_eq!(config, previous),
            other => panic!("previous snapshot was not valid: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_primary_with_valid_backup_recovers_explicitly() {
        let dir = unique_state_dir("recover-backup");
        std::fs::create_dir_all(&dir).expect("create state fixture dir");
        let path = dir.join("desktop-state.json");
        let expected = test_stored_config("recovered");
        write_atomic_file(
            &desktop_state_backup_path(&path),
            &serde_json::to_vec_pretty(&expected).unwrap(),
        )
        .expect("write valid backup");
        std::fs::write(&path, b"{corrupt-primary").expect("write corrupt primary");
        let activity = ActivityLog::default();

        let recovered = load_config(&path, &activity).expect("recover from backup");
        assert_eq!(recovered, expected);
        assert!(matches!(
            read_stored_config(&path).expect("read restored primary"),
            StoredConfigFile::Valid { .. }
        ));
        assert!(activity.snapshot().iter().any(|entry| {
            entry.event_kind == ActivityEventKind::StateRecovered && entry.source == "desktop_state"
        }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_primary_and_backup_returns_explicit_error() {
        let dir = unique_state_dir("both-corrupt");
        std::fs::create_dir_all(&dir).expect("create state fixture dir");
        let path = dir.join("desktop-state.json");
        std::fs::write(&path, b"{corrupt-primary").expect("write corrupt primary");
        std::fs::write(desktop_state_backup_path(&path), b"{corrupt-backup")
            .expect("write corrupt backup");

        let error = load_config(&path, &ActivityLog::default())
            .expect_err("both corrupt copies must fail closed");
        assert_eq!(error.code, "desktop_state_corrupt");
        assert_eq!(
            error
                .details
                .as_ref()
                .and_then(|details| details.get("category"))
                .and_then(Value::as_str),
            Some("state_corrupt")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stored_runtime_contains_paths_not_credentials() {
        let runtime = StoredRuntime {
            server_url: "https://example.com".to_string(),
            server_env_file: Some(PathBuf::from("webcodex.env")),
            runner_config: Some(PathBuf::from("runner.toml")),
            user_token_file: Some(PathBuf::from("user-token")),
            runner_client_id: Some("desktop-runner".to_string()),
            project_id: Some("project".to_string()),
            runtime_project_id: Some("agent:desktop:project".to_string()),
        };
        let json = serde_json::to_string(&runtime).unwrap();
        assert!(!json.contains("wc_pat_"));
        assert!(!json.contains("wc_agent_"));
        assert!(!json.contains("CONTROL_PLANE_API_KEY"));
    }

    #[tokio::test]
    async fn explicit_stop_survives_refresh_and_desktop_restart() {
        let data_dir = unique_state_dir("stopped-refresh");
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("resources")).unwrap();
        core.config = test_stored_config("stopped");
        core.config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        let cancellation = CancellationContext::never();
        core.stop_local_runtime(&cancellation).await.unwrap();
        let refreshed = core.refresh_runtime_status(&cancellation).await.unwrap();
        assert_eq!(
            refreshed.readiness.summary_kind,
            ReadinessSummaryKind::RuntimeStopped
        );
        assert!(!refreshed.runtime_autostart);
        let restarted = DesktopCore::new(data_dir.clone(), data_dir.join("resources")).unwrap();
        assert_eq!(
            restarted.snapshot.readiness.summary_kind,
            ReadinessSummaryKind::RuntimeStopped
        );
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn legacy_full_runtime_defaults_to_autostart_but_explicit_stop_is_preserved() {
        let mut config = test_stored_config("resume");
        config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        config.runtime = Some(StoredRuntime {
            server_url: "http://127.0.0.1:58208".to_string(),
            server_env_file: None,
            runner_config: None,
            user_token_file: None,
            runner_client_id: None,
            project_id: None,
            runtime_project_id: None,
        });
        assert!(runtime_autostart(&config));

        config.runtime_autostart = Some(false);
        assert!(!runtime_autostart(&config));
    }

    #[test]
    fn legacy_full_runtime_with_saved_tunnel_defaults_to_reconnect_but_explicit_stop_wins() {
        let mut config = test_stored_config("reconnect");
        config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        config.runtime = Some(StoredRuntime {
            server_url: "http://127.0.0.1:58208".to_string(),
            server_env_file: None,
            runner_config: None,
            user_token_file: None,
            runner_client_id: Some("desktop".to_string()),
            project_id: None,
            runtime_project_id: None,
        });

        assert_eq!(
            preferred_connection(&config, true),
            RegularConnectionPreference::OpenAiTunnel
        );
        assert_eq!(
            preferred_connection(&config, false),
            RegularConnectionPreference::NoChatGpt
        );

        config.preferred_connection = Some(RegularConnectionPreference::NoChatGpt);
        assert_eq!(
            preferred_connection(&config, true),
            RegularConnectionPreference::NoChatGpt,
            "an explicit user stop must override legacy auto-reconnect inference"
        );
    }

    #[test]
    fn legacy_user_project_migrates_to_authorized_paths_but_management_anchor_does_not() {
        let management = PathBuf::from("/management");
        let resources = PathBuf::from("/resources");
        let local_topology = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        };
        let mut user = test_stored_config("legacy-user");
        user.topology = Some(local_topology.clone());
        user.projects.clear();
        migrate_legacy_authorized_projects(&mut user, &management, &resources);
        assert_eq!(user.projects.len(), 1);
        assert_eq!(user.projects[0].path, "/legacy-user");
        assert!(user.project.is_none());

        let mut internal = test_stored_config("internal");
        internal.topology = Some(local_topology);
        internal.projects.clear();
        internal.project.as_mut().unwrap().path = resources.to_string_lossy().to_string();
        migrate_legacy_authorized_projects(&mut internal, &management, &resources);
        assert!(internal.projects.is_empty());
        assert!(internal.project.is_none());
    }

    #[test]
    fn remote_resume_prefers_authorized_paths_and_keeps_legacy_fallback() {
        let mut config = test_stored_config("visible");
        assert_eq!(
            customer_authorized_anchor_path(&config).as_deref(),
            Some("/visible")
        );

        config.projects.clear();
        assert_eq!(
            customer_authorized_anchor_path(&config).as_deref(),
            Some("/visible")
        );
    }

    #[test]
    fn remote_zero_root_runner_identity_survives_without_project_authority() {
        let dir = unique_state_dir("remote-zero-root-identity");
        std::fs::create_dir_all(&dir).unwrap();
        let runner_config = dir.join("runner.toml");
        let user_token_file = dir.join("webcodex-user-token");
        std::fs::write(&runner_config, "fixture").unwrap();
        std::fs::write(&user_token_file, "fixture").unwrap();

        let mut config = test_stored_config("legacy");
        config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Remote {
                url: "https://server.example.test".to_string(),
            },
            runner: RunnerTopology::Local,
            exposure: Exposure::ExistingHttps {
                url: "https://server.example.test".to_string(),
            },
            enrollment: Enrollment::ManagedPairing,
        });
        config.runtime_anchor = None;
        config.project = None;
        config.projects.clear();
        config.runtime = Some(StoredRuntime {
            server_url: "https://server.example.test".to_string(),
            server_env_file: None,
            runner_config: Some(runner_config),
            user_token_file: Some(user_token_file),
            runner_client_id: Some("desktop-zero-root".to_string()),
            project_id: None,
            runtime_project_id: None,
        });

        assert!(runner_identity_from_config(&config).is_some());
        assert!(identity_from_config(&config).is_none());
        assert!(customer_project_identity_from_config(&config).is_none());
        assert!(project_snapshot(&config).is_none());
        assert_eq!(customer_authorized_anchor_path(&config), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remote_project_authority_can_be_removed_without_losing_runner_enrollment() {
        let dir = unique_state_dir("remote-project-lifecycle");
        std::fs::create_dir_all(&dir).unwrap();
        let runner_config = dir.join("runner.toml");
        let user_token_file = dir.join("webcodex-user-token");
        std::fs::write(&runner_config, "fixture").unwrap();
        std::fs::write(&user_token_file, "fixture").unwrap();
        let project = ProjectSelection {
            path: dir.join("project").to_string_lossy().to_string(),
            allowed_root: dir.join("project").to_string_lossy().to_string(),
            is_git_repository: false,
            runtime_project_id: Some("agent:desktop-remote:project-a".to_string()),
        };

        let mut config = test_stored_config("legacy");
        config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Remote {
                url: "https://server.example.test".to_string(),
            },
            runner: RunnerTopology::Local,
            exposure: Exposure::ExistingHttps {
                url: "https://server.example.test".to_string(),
            },
            enrollment: Enrollment::ManagedPairing,
        });
        config.runtime_anchor = None;
        config.project = Some(project.clone());
        config.projects = vec![project];
        config.runtime = Some(StoredRuntime {
            server_url: "https://server.example.test".to_string(),
            server_env_file: None,
            runner_config: Some(runner_config),
            user_token_file: Some(user_token_file),
            runner_client_id: Some("desktop-remote".to_string()),
            project_id: Some("project-a".to_string()),
            runtime_project_id: Some("agent:desktop-remote:project-a".to_string()),
        });

        assert!(runner_identity_from_config(&config).is_some());
        assert!(customer_project_identity_from_config(&config).is_some());
        config.projects.clear();
        config.project = None;
        if let Some(runtime) = config.runtime.as_mut() {
            runtime.project_id = None;
            runtime.runtime_project_id = None;
        }
        assert!(runner_identity_from_config(&config).is_some());
        assert!(customer_project_identity_from_config(&config).is_none());
        assert!(project_snapshot(&config).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn remote_setup_validation_failure_keeps_active_topology_unchanged() {
        let data_dir = unique_state_dir("remote-setup-transaction");
        let resource_dir = data_dir.join("resources");
        std::fs::create_dir_all(&resource_dir).unwrap();
        let mut core = DesktopCore::new(data_dir.clone(), resource_dir).unwrap();
        let local = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        };
        core.config.topology = Some(local.clone());
        core.snapshot.topology = Some(local.clone());
        let error = core
            .configure_remote_setup(
                "not-a-server-url",
                "wc_pair_unused",
                None,
                &CancellationContext::never(),
            )
            .await
            .expect_err("invalid remote setup must fail before commit");
        assert_eq!(error.code, "server_url_invalid");
        assert_eq!(core.config.topology, Some(local.clone()));
        assert_eq!(core.snapshot.topology, Some(local));
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn authorized_project_helpers_keep_multiple_exact_paths_and_remove_only_one() {
        let mut config = test_stored_config("first");
        config.projects.clear();
        let first = ProjectSelection {
            path: "/first".to_string(),
            allowed_root: "/first".to_string(),
            is_git_repository: false,
            runtime_project_id: Some("agent:desktop:first".to_string()),
        };
        let second = ProjectSelection {
            path: "/second".to_string(),
            allowed_root: "/second".to_string(),
            is_git_repository: true,
            runtime_project_id: Some("agent:desktop:second".to_string()),
        };
        upsert_authorized_project(&mut config, first.clone());
        upsert_authorized_project(&mut config, second.clone());
        assert_eq!(config.projects, vec![first.clone(), second.clone()]);

        assert_eq!(
            remove_authorized_project(&mut config, &first.path),
            Some(first)
        );
        assert_eq!(config.projects, vec![second]);
    }

    #[test]
    fn explicit_tunnel_proxy_is_bounded_and_direct_mode_clears_routing() {
        let custom = TunnelProxyConfig {
            mode: TunnelProxyMode::Custom,
            custom_url: Some("http://127.0.0.1:7890".to_string()),
        };
        let effective = effective_tunnel_proxy(&custom).expect("valid custom proxy");
        assert_eq!(effective.url.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(effective.source, "custom");

        let direct = effective_tunnel_proxy(&TunnelProxyConfig {
            mode: TunnelProxyMode::Direct,
            custom_url: Some("http://ignored.example.test:8080".to_string()),
        })
        .expect("direct mode");
        assert_eq!(direct.url, None);
        assert_eq!(direct.source, "direct");

        assert!(validate_tunnel_proxy_url("http://user:secret@127.0.0.1:7890").is_err());
    }

    #[test]
    fn automatic_tunnel_proxy_candidates_preserve_real_ports_and_fallback_order() {
        let candidates = auto_tunnel_proxy_candidates_from(
            vec!["http://127.0.0.1:7897".to_string()],
            Some(crate::platform::SystemProxyCandidate {
                url: "http://127.0.0.1:12000".to_string(),
                enabled: true,
            }),
        );
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].url.as_deref(), Some("http://127.0.0.1:7897"));
        assert_eq!(candidates[0].source, "environment");
        assert_eq!(candidates[1].url.as_deref(), Some("http://127.0.0.1:12000"));
        assert_eq!(candidates[1].source, "windows_system");
        assert_eq!(candidates[2].url, None);
        assert_eq!(candidates[2].source, "direct");
    }

    #[tokio::test]
    async fn automatic_tunnel_proxy_falls_back_from_bad_inherited_route() {
        let candidates = auto_tunnel_proxy_candidates_from(
            vec!["http://127.0.0.1:7897".to_string()],
            Some(crate::platform::SystemProxyCandidate {
                url: "http://127.0.0.1:12000".to_string(),
                enabled: true,
            }),
        );
        let selected = first_reachable_tunnel_proxy(candidates, |candidate| async move {
            candidate.url.as_deref() == Some("http://127.0.0.1:12000")
        })
        .await
        .expect("system proxy should be selected after inherited proxy fails");
        assert_eq!(selected.url.as_deref(), Some("http://127.0.0.1:12000"));
        assert_eq!(selected.source, "windows_system");
    }

    #[test]
    fn automatic_tunnel_proxy_deduplicates_same_environment_and_system_route() {
        let candidates = auto_tunnel_proxy_candidates_from(
            vec![
                "http://127.0.0.1:10809".to_string(),
                "http://127.0.0.1:10809".to_string(),
            ],
            Some(crate::platform::SystemProxyCandidate {
                url: "http://127.0.0.1:10809".to_string(),
                enabled: true,
            }),
        );
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].url.as_deref(), Some("http://127.0.0.1:10809"));
        assert_eq!(candidates[1].url, None);
    }

    #[test]
    fn automatic_proxy_projection_does_not_claim_an_unverified_route() {
        let config = StoredDesktopConfig {
            tunnel_proxy: TunnelProxyConfig::default(),
            ..StoredDesktopConfig::default()
        };
        let mut snapshot = DesktopStateSnapshot::default();
        apply_config_projection(&mut snapshot, &config);
        assert_eq!(snapshot.tunnel_proxy.mode, TunnelProxyMode::Auto);
        assert_eq!(snapshot.tunnel_proxy.effective_source, "auto_pending");
        assert_eq!(snapshot.tunnel_proxy.effective_url, None);
    }

    #[test]
    fn failed_runtime_resume_restores_the_last_published_state() {
        let data_dir = unique_state_dir("resume-failure-reconcile");
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("resources"))
            .expect("create Desktop core");
        let mut baseline_snapshot = DesktopStateSnapshot::default();
        baseline_snapshot.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        baseline_snapshot.readiness = aggregate_readiness(
            ServerReadiness::Stopped,
            RunnerReadiness::Stopped,
            ExposureReadiness::LocalReady,
            ProjectReadiness::Configured,
        );
        let baseline = ProcessBaseline {
            local_server: false,
            local_runner: None,
            quick_share: false,
            regular_tunnel: false,
            snapshot: baseline_snapshot.clone(),
        };
        core.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Connecting,
            ExposureReadiness::LocalReady,
            ProjectReadiness::Configured,
        );

        core.reconcile_after_operation_failure(
            DesktopOperationKind::RuntimeResume,
            &baseline,
            ProcessCleanup {
                local_server: true,
                local_runner: true,
                ..ProcessCleanup::default()
            },
            false,
        );

        assert_eq!(core.snapshot.readiness, baseline_snapshot.readiness);
        assert_eq!(core.snapshot.topology, baseline_snapshot.topology);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn invalid_stored_identity_is_not_advertised_for_reuse() {
        let config = StoredDesktopConfig {
            topology: Some(RuntimeTopology {
                experience: Experience::Full,
                server: ServerTopology::Remote {
                    url: "https://example.test".to_string(),
                },
                runner: RunnerTopology::Local,
                exposure: Exposure::ExistingHttps {
                    url: "https://example.test".to_string(),
                },
                enrollment: Enrollment::ManagedPairing,
            }),
            runtime_anchor: None,
            project: Some(ProjectSelection {
                path: r"C:\repo".to_string(),
                allowed_root: r"C:\".to_string(),
                is_git_repository: true,
                runtime_project_id: Some("agent:desktop:repo".to_string()),
            }),
            projects: vec![ProjectSelection {
                path: r"C:\repo".to_string(),
                allowed_root: r"C:\".to_string(),
                is_git_repository: true,
                runtime_project_id: Some("agent:desktop:repo".to_string()),
            }],
            runtime_autostart: None,
            preferred_connection: None,
            openai_tunnel_id: None,
            tunnel_proxy: TunnelProxyConfig::default(),
            runtime: Some(StoredRuntime {
                server_url: "https://example.test".to_string(),
                server_env_file: None,
                runner_config: Some(PathBuf::from("missing-runner.toml")),
                user_token_file: Some(PathBuf::from("missing-user-token")),
                runner_client_id: Some("desktop".to_string()),
                project_id: Some("repo".to_string()),
                runtime_project_id: Some("agent:desktop:repo".to_string()),
            }),
        };
        assert_eq!(
            project_snapshot(&config).and_then(|project| project.runtime_project_id),
            None
        );
    }

    #[tokio::test]
    async fn unactivated_desktop_rejects_runtime_start_before_any_process_is_spawned() {
        let temp = tempfile::tempdir().expect("temp dir");
        let state = AppState::new(temp.path().to_path_buf(), temp.path().join("resources"))
            .expect("create unactivated Desktop state");

        assert_eq!(
            state.get_state().license.status,
            crate::models::LicenseStatus::NotActivated
        );
        let error = state
            .resume_saved_runtime()
            .await
            .expect_err("runtime resume must be gated by device activation");
        assert_eq!(error.code, "license_required");

        let mut supervisor = state.supervisor.lock().await;
        for kind in [
            ProcessKind::LocalServer,
            ProcessKind::LocalRunner,
            ProcessKind::QuickShare,
            ProcessKind::RegularTunnel,
        ] {
            assert!(
                supervisor.snapshot(kind).is_none(),
                "activation gate must run before any Desktop-owned process is created"
            );
        }
        assert!(!state
            .activity()
            .iter()
            .any(|entry| entry.event_kind == ActivityEventKind::ProcessStarted));
    }

    #[tokio::test]
    async fn control_plane_stays_observable_and_cancel_is_exact_while_mutation_is_stuck() {
        let data_dir = std::env::temp_dir().join(format!(
            "webcodex-desktop-control-plane-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let state = Arc::new(
            AppState::new(data_dir.clone(), data_dir.join("test-resources"))
                .expect("create Desktop test state"),
        );
        state.set_test_license_active();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let operation_state = Arc::clone(&state);
        let operation = tokio::spawn(async move {
            operation_state
                .hold_test_operation(started_tx, release_rx)
                .await
        });
        let first_id = tokio::time::timeout(Duration::from_secs(1), started_rx)
            .await
            .expect("stuck operation must start")
            .expect("operation id");

        let observed = tokio::time::timeout(Duration::from_millis(250), async {
            (state.get_state(), state.activity())
        })
        .await
        .expect("control-plane reads must not wait for the mutation core");
        assert_eq!(
            observed
                .0
                .current_operation
                .as_ref()
                .map(|operation| operation.id.as_str()),
            Some(first_id.as_str())
        );
        assert_eq!(
            observed
                .0
                .current_operation
                .as_ref()
                .map(|operation| operation.phase),
            Some(crate::models::DesktopOperationPhase::Running)
        );
        assert!(
            !observed.1.is_empty(),
            "Activity must stay independently readable"
        );

        let busy = tokio::time::timeout(Duration::from_millis(250), state.refresh_runtime_status())
            .await
            .expect("second mutation must fail fast")
            .expect_err("second mutation must not queue behind the stuck operation");
        assert_eq!(busy.code, "desktop_operation_busy");

        let cancelling = state
            .cancel_operation(&first_id)
            .expect("exact observed operation can be stopped");
        assert_eq!(
            cancelling
                .current_operation
                .as_ref()
                .map(|operation| operation.phase),
            Some(crate::models::DesktopOperationPhase::Cancelling)
        );
        let still_busy =
            tokio::time::timeout(Duration::from_millis(250), state.refresh_runtime_status())
                .await
                .expect("cancelling operation must retain the mutation slot")
                .expect_err("cleanup has not completed yet");
        assert_eq!(still_busy.code, "desktop_operation_busy");

        release_tx.send(()).expect("release first cleanup");
        let first_error = operation
            .await
            .expect("first operation task")
            .expect_err("first operation was cancelled");
        assert_eq!(first_error.code, "desktop_operation_cancelled");
        assert!(state.get_state().current_operation.is_none());

        let (second_started_tx, second_started_rx) = tokio::sync::oneshot::channel();
        let (second_release_tx, second_release_rx) = tokio::sync::oneshot::channel();
        let second_state = Arc::clone(&state);
        let second_operation = tokio::spawn(async move {
            second_state
                .hold_test_operation(second_started_tx, second_release_rx)
                .await
        });
        let second_id = tokio::time::timeout(Duration::from_secs(1), second_started_rx)
            .await
            .expect("second operation must start")
            .expect("second operation id");
        assert_ne!(first_id, second_id);

        let stale = state
            .cancel_operation(&first_id)
            .expect_err("late cancel for A must not target B");
        assert_eq!(stale.code, "desktop_operation_not_current");
        let second_snapshot = state.get_state();
        assert_eq!(
            second_snapshot
                .current_operation
                .as_ref()
                .map(|operation| operation.id.as_str()),
            Some(second_id.as_str())
        );
        assert_eq!(
            second_snapshot
                .current_operation
                .as_ref()
                .map(|operation| operation.phase),
            Some(crate::models::DesktopOperationPhase::Running)
        );

        state
            .cancel_operation(&second_id)
            .expect("exact second cancel");
        second_release_tx.send(()).expect("release second cleanup");
        let second_error = second_operation
            .await
            .expect("second operation task")
            .expect_err("second operation was cancelled");
        assert_eq!(second_error.code, "desktop_operation_cancelled");
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[cfg(target_os = "macos")]
    fn mac_process_exists(pid: u32) -> bool {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        let result = unsafe { libc::kill(pid, 0) };
        if result == 0 {
            return true;
        }
        !matches!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        )
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn shutdown_cancels_stuck_one_shot_and_reclaims_all_desktop_owned_trees() {
        let data_dir = std::env::temp_dir().join(format!(
            "webcodex-desktop-shutdown-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let long_marker = data_dir.join("long-lived-pids.txt");
        let one_shot_marker = data_dir.join("one-shot-pids.txt");
        std::fs::create_dir_all(&data_dir).expect("create shutdown fixture dir");
        let state = Arc::new(
            AppState::new(data_dir.clone(), data_dir.join("test-resources"))
                .expect("create Desktop shutdown test state"),
        );

        let mut long_command = std::process::Command::new("/bin/sh");
        long_command.args([
            "-c",
            "sleep 8 & descendant=$!; printf '%s %s\\n' \"$$\" \"$descendant\" > \"$1\"; wait \"$descendant\"",
            "webcodex-long-lived-shutdown",
            &long_marker.to_string_lossy(),
        ]);
        state
            .supervisor
            .lock()
            .await
            .spawn_owned(ProcessKind::LocalServer, long_command, false)
            .await
            .expect("start long-lived Desktop-owned fixture");

        let marker_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while !long_marker.is_file() {
            assert!(
                tokio::time::Instant::now() < marker_deadline,
                "long-lived fixture did not start"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let long_pids = std::fs::read_to_string(&long_marker)
            .expect("long-lived fixture pids")
            .split_whitespace()
            .map(|value| value.parse::<u32>().expect("long-lived fixture pid"))
            .collect::<Vec<_>>();

        let args = vec![
            "-c".to_string(),
            "sleep 8 & descendant=$!; printf '%s %s\\n' \"$$\" \"$descendant\" > \"$1\"; wait \"$descendant\"".to_string(),
            "webcodex-one-shot-shutdown".to_string(),
            one_shot_marker.to_string_lossy().to_string(),
        ];
        let operation_state = Arc::clone(&state);
        let operation = tokio::spawn(async move {
            operation_state
                .run_test_one_shot_operation(
                    PathBuf::from("/bin/sh"),
                    args,
                    vec![b'x'; 64 * 1024],
                    Duration::from_secs(8),
                )
                .await
        });
        let marker_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while !one_shot_marker.is_file() {
            assert!(
                tokio::time::Instant::now() < marker_deadline,
                "one-shot fixture did not start"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let one_shot_pids = std::fs::read_to_string(&one_shot_marker)
            .expect("one-shot fixture pids")
            .split_whitespace()
            .map(|value| value.parse::<u32>().expect("one-shot fixture pid"))
            .collect::<Vec<_>>();

        tokio::time::timeout(Duration::from_secs(7), state.shutdown())
            .await
            .expect("shutdown must remain bounded during a stuck one-shot operation");
        let operation_error = operation
            .await
            .expect("one-shot operation task")
            .expect_err("shutdown must cancel the one-shot operation");
        assert_eq!(operation_error.code, "desktop_operation_cancelled");
        for pid in long_pids.into_iter().chain(one_shot_pids) {
            assert!(
                !mac_process_exists(pid),
                "Desktop-owned PID {pid} survived application shutdown"
            );
        }
        assert!(state
            .supervisor
            .lock()
            .await
            .snapshot(ProcessKind::LocalServer)
            .is_none());
        tokio::time::timeout(Duration::from_millis(250), state.shutdown())
            .await
            .expect("repeated shutdown must be idempotent and fast");
        let _ = std::fs::remove_dir_all(data_dir);
    }

    /// Serializes tests that mutate process environment variables.
    ///
    /// Env mutation is process-global, so env-mutating tests in this test
    /// binary hold one shared lock for their whole body. Desktop is its own
    /// Cargo workspace, so this per-crate `TEST_ENV_LOCK` follows the
    /// convention documented in docs/TESTING.md.
    static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Restores one process environment variable when the guard is dropped,
    /// including through panic unwinding. Holding `TEST_ENV_LOCK` for the
    /// guard's lifetime also serializes env mutation across sibling tests;
    /// poisoning is tolerated because the RAII restore already fixed the
    /// environment before the lock is released.
    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
        _test_env_lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let test_env_lock = TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self {
                key,
                previous,
                _test_env_lock: test_env_lock,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn native_local_full_dogfood_reuses_enrollment_and_stops_owned_runtime() {
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        // The Server only accepts lowercase local pairing usernames. Pin a
        // mixed-case OS username so the compatibility path is exercised on
        // every machine, not only where the login name already fails. The
        // guard restores the previous value on every exit path, including
        // panics, so this override never leaks to sibling tests.
        let _username_guard = EnvVarGuard::set("USERNAME", "Alice Dogfood");
        // macOS exposes its temporary root through /var -> /private/var.
        // Resolve the fixture root, not the credential-store security checks.
        let temporary_root = std::env::temp_dir()
            .canonicalize()
            .expect("resolve the native temporary fixture root");
        let data_dir = temporary_root.join(format!(
            "webcodex-desktop-local-dogfood-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("test-resources"))
            .expect("create local dogfood state");
        let cancellation = CancellationContext::never();
        let setup = core
            .configure_local_setup(Some(&project), &cancellation)
            .await;
        let snapshot = match setup {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let stages: Vec<_> = core
                    .activity
                    .snapshot()
                    .into_iter()
                    .map(|entry| entry.event_kind)
                    .collect();
                let readiness = core.snapshot.readiness.clone();
                let initialized = data_dir.join("runtime/local/webcodex.env").is_file();
                core.supervisor.lock().await.stop_all().await;
                let _ = std::fs::remove_dir_all(&data_dir);
                panic!(
                    "local full setup failed: {error:?}; initialized={initialized}; readiness={readiness:?}; stages={stages:?}"
                );
            }
        };
        assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
        assert_eq!(snapshot.readiness.exposure, ExposureReadiness::LocalReady);
        assert!(snapshot.readiness.runtime_ready);
        assert!(!snapshot.readiness.ready_for_chatgpt);

        let first_runtime = core
            .config
            .runtime
            .as_ref()
            .expect("local setup stores runtime identity");
        let first_user_token_file = first_runtime
            .user_token_file
            .clone()
            .expect("local setup stores managed user token path");
        let first_user_token =
            std::fs::read(&first_user_token_file).expect("read managed user token before restart");
        let first_runner_config = first_runtime.runner_config.clone().unwrap();
        let first_runner_bytes = std::fs::read(&first_runner_config).unwrap();

        let stopped = core
            .stop_local_runtime(&cancellation)
            .await
            .expect("stop local runtime");
        assert_eq!(stopped.readiness.server, ServerReadiness::Stopped);
        assert_eq!(stopped.readiness.runner, RunnerReadiness::Stopped);
        assert!(core
            .process_snapshot(ProcessKind::LocalServer)
            .await
            .is_none());
        assert!(core
            .process_snapshot(ProcessKind::LocalRunner)
            .await
            .is_none());

        let restarted = core
            .configure_local_setup(Some(&project), &cancellation)
            .await
            .expect("restart local full setup without re-enrollment");
        assert_eq!(restarted.readiness.server, ServerReadiness::Ready);
        assert_eq!(restarted.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(restarted.readiness.project, ProjectReadiness::Ready);
        let second_user_token_file = core
            .config
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.user_token_file.clone())
            .expect("restarted local setup keeps managed user token path");
        assert_eq!(second_user_token_file, first_user_token_file);
        let second_user_token =
            std::fs::read(&second_user_token_file).expect("read managed user token after restart");
        assert!(
            first_user_token == second_user_token,
            "local restart must reuse enrollment instead of rotating the managed user token"
        );
        let project_a_identity =
            identity_from_config(&core.config).expect("project A runtime identity after restart");
        let first_runner_client_id =
            stored_runner_client_id(&core.config).expect("stored Runner client identity");
        let server_before_switch = core
            .process_snapshot(ProcessKind::LocalServer)
            .await
            .expect("Desktop owns local Server before project switch");
        let runner_before_switch = core
            .process_snapshot(ProcessKind::LocalRunner)
            .await
            .expect("Desktop owns local Runner before project switch");
        assert!(server_before_switch.owned_by_desktop);
        assert!(runner_before_switch.owned_by_desktop);
        // Switch projects while the Desktop-owned Server and Runner are still
        // running. A compatible connection hot-extends exact-root authority and
        // activates the new Project without replacing either owned process.
        let second_project = data_dir.join("second-project");
        std::fs::create_dir_all(&second_project).expect("create second project fixture");
        let expected_project = core
            .adapter
            .inspect_project(&second_project.to_string_lossy())
            .await
            .expect("inspect second project fixture");
        let switched = match core
            .activate_local_project(&second_project.to_string_lossy(), &cancellation)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let readiness = core.snapshot.readiness.clone();
                core.supervisor.lock().await.stop_all().await;
                let _ = std::fs::remove_dir_all(&data_dir);
                panic!("switch project setup failed: {error:?}; readiness={readiness:?}");
            }
        };
        assert_eq!(switched.readiness.server, ServerReadiness::Ready);
        assert_eq!(switched.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(switched.readiness.project, ProjectReadiness::Ready);
        assert!(same_project(
            &core.config.project.as_ref().expect("selected project").path,
            &expected_project.path,
        ));
        let server_after_switch = core
            .process_snapshot(ProcessKind::LocalServer)
            .await
            .expect("Desktop still owns local Server after project switch");
        let runner_after_switch = core
            .process_snapshot(ProcessKind::LocalRunner)
            .await
            .expect("Desktop still owns the same Runner after project switch");
        assert_eq!(
            server_after_switch.pid, server_before_switch.pid,
            "project switch must keep the existing Desktop-owned Server"
        );
        assert_eq!(
            runner_after_switch.pid, runner_before_switch.pid,
            "compatible project switch must preserve the Desktop-owned Runner PID"
        );
        assert!(server_after_switch.owned_by_desktop);
        assert!(runner_after_switch.owned_by_desktop);
        let switched_identity =
            identity_from_config(&core.config).expect("project B runtime identity after switch");
        assert_eq!(
            stored_runner_client_id(&core.config).as_deref(),
            Some(first_runner_client_id.as_str()),
            "project switch must preserve the Runner client identity"
        );
        assert_eq!(
            switched_identity.runner_config, first_runner_config,
            "project switch must hot-update the same Runner config"
        );
        assert_eq!(
            switched_identity.user_token_file, first_user_token_file,
            "project switch must preserve the managed user-token path"
        );
        assert_ne!(
            switched_identity.runtime_project_id, project_a_identity.runtime_project_id,
            "different canonical projects must keep distinct runtime Project identities"
        );
        let config_after_switch = std::fs::read(&first_runner_config).unwrap();
        assert_ne!(
            config_after_switch, first_runner_bytes,
            "switching to a new exact root must persist the policy candidate in the existing Runner config"
        );
        assert_eq!(
            std::fs::read(&first_user_token_file).unwrap(),
            first_user_token,
            "switching projects must not rotate the managed user token"
        );
        assert!(
            core.adapter
                .project_ready(&project_a_identity, &cancellation)
                .await
                .expect("project A inventory remains observable"),
            "activating Project B must not delete Project A registration"
        );

        let project_b_runtime_id = switched_identity.runtime_project_id.clone();
        let repeated = core
            .activate_local_project(&second_project.to_string_lossy(), &cancellation)
            .await
            .expect("reselecting Project B is idempotent");
        assert_eq!(repeated.readiness.project, ProjectReadiness::Ready);
        let runner_after_repeat = core
            .process_snapshot(ProcessKind::LocalRunner)
            .await
            .expect("Desktop still owns Runner after idempotent activation");
        assert_eq!(
            runner_after_repeat.pid, runner_before_switch.pid,
            "idempotent project activation must not restart the Runner"
        );
        let repeated_identity = identity_from_config(&core.config)
            .expect("Project B identity after repeated activation");
        assert_eq!(repeated_identity.runtime_project_id, project_b_runtime_id);
        assert_eq!(
            stored_runner_client_id(&core.config),
            Some(first_runner_client_id)
        );
        assert_eq!(
            std::fs::read(&first_runner_config).unwrap(),
            config_after_switch,
            "idempotent activation must not duplicate or rewrite allowed_roots"
        );
        assert_eq!(
            std::fs::read(&first_user_token_file).unwrap(),
            first_user_token,
            "idempotent activation must keep the same managed user token"
        );
        core.stop_local_runtime(&cancellation)
            .await
            .expect("stop restarted local runtime");
        drop(core);
        std::fs::remove_dir_all(&data_dir).expect("remove local dogfood app data");
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn windows_quick_share_dogfood_reaches_ready_and_stops_foreground_owner() {
        if !cfg!(windows) {
            return;
        }
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        let data_dir = std::env::temp_dir().join(format!(
            "webcodex-desktop-share-dogfood-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("test-resources"))
            .expect("create Quick Share dogfood state");
        let cancellation = CancellationContext::never();
        let started = core
            .start_quick_share(&project, "none", &cancellation)
            .await;
        let snapshot = match started {
            Ok(snapshot) => snapshot,
            Err(error) => {
                core.supervisor.lock().await.stop_all().await;
                let _ = std::fs::remove_dir_all(&data_dir);
                panic!("Quick Share setup failed: {error:?}");
            }
        };
        assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
        assert_eq!(snapshot.readiness.exposure, ExposureReadiness::LocalReady);
        assert!(snapshot.readiness.runtime_ready);
        assert!(!snapshot.readiness.ready_for_chatgpt);
        assert!(core
            .process_snapshot(ProcessKind::QuickShare)
            .await
            .is_some());

        core.stop_quick_share(&cancellation)
            .await
            .expect("stop Quick Share");
        assert!(core
            .process_snapshot(ProcessKind::QuickShare)
            .await
            .is_none());
        drop(core);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn saved_remote_switch_replaces_only_the_different_desktop_owned_runner_generation() {
        let data_dir = unique_state_dir("saved-remote-owned-runner-switch");
        let resource_dir = data_dir.join("resources");
        std::fs::create_dir_all(&resource_dir).unwrap();
        let local_runner_config = data_dir.join("local-runner.toml");
        let remote_runner_config = data_dir.join("remote-runner.toml");
        let local_user_token = data_dir.join("local-user-token");
        let remote_user_token = data_dir.join("remote-user-token");
        for path in [
            &local_runner_config,
            &remote_runner_config,
            &local_user_token,
            &remote_user_token,
        ] {
            std::fs::write(path, "fixture").unwrap();
        }
        let core = DesktopCore::new(data_dir.clone(), resource_dir).unwrap();
        let cancellation = CancellationContext::never();
        let local_identity = RunnerRuntimeIdentity {
            runner_config: local_runner_config,
            user_token_file: local_user_token,
            server_url: "http://127.0.0.1:3721".to_string(),
        };
        let remote_identity = RunnerRuntimeIdentity {
            runner_config: remote_runner_config,
            user_token_file: remote_user_token,
            server_url: "https://engine-test.example".to_string(),
        };
        let command = || {
            let mut command = std::process::Command::new("powershell.exe");
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$null = [Console]::In.ReadToEnd()",
            ]);
            command
        };

        core.supervisor
            .lock()
            .await
            .spawn_owned_with_target(
                ProcessKind::LocalRunner,
                command(),
                false,
                Some(runner_process_target_key(&local_identity)),
            )
            .await
            .expect("start Desktop-owned Local Runner fixture");

        let first_remote = core
            .prepare_owned_runner_target(
                &remote_identity,
                false,
                Deadline::after(Duration::from_secs(5)),
                &cancellation,
            )
            .await
            .expect("saved Remote target should replace Local Runner");
        assert!(first_remote.replaced);
        assert!(!first_remote.target_already_active);
        core.supervisor
            .lock()
            .await
            .spawn_owned_with_target(
                ProcessKind::LocalRunner,
                command(),
                false,
                Some(runner_process_target_key(&remote_identity)),
            )
            .await
            .expect("saved Remote Runner must not hit process_already_running");
        let same_remote = core
            .prepare_owned_runner_target(
                &remote_identity,
                false,
                Deadline::after(Duration::from_secs(5)),
                &cancellation,
            )
            .await
            .expect("same active Remote target should be reusable");
        assert!(!same_remote.replaced);
        assert!(same_remote.target_already_active);

        let back_local = core
            .prepare_owned_runner_target(
                &local_identity,
                false,
                Deadline::after(Duration::from_secs(5)),
                &cancellation,
            )
            .await
            .expect("Local target should replace Remote Runner");
        assert!(back_local.replaced);
        core.supervisor
            .lock()
            .await
            .spawn_owned_with_target(
                ProcessKind::LocalRunner,
                command(),
                false,
                Some(runner_process_target_key(&local_identity)),
            )
            .await
            .expect("Local Runner restart must not hit process_already_running");

        let second_remote = core
            .prepare_owned_runner_target(
                &remote_identity,
                false,
                Deadline::after(Duration::from_secs(5)),
                &cancellation,
            )
            .await
            .expect("same saved Remote target should replace Local Runner again");
        assert!(second_remote.replaced);
        core.supervisor
            .lock()
            .await
            .spawn_owned_with_target(
                ProcessKind::LocalRunner,
                command(),
                false,
                Some(runner_process_target_key(&remote_identity)),
            )
            .await
            .expect("second saved Remote switch must not hit process_already_running");
        assert_eq!(
            core.supervisor
                .lock()
                .await
                .active_target_matches(
                    ProcessKind::LocalRunner,
                    &runner_process_target_key(&remote_identity),
                ),
            Some(true)
        );

        core.supervisor.lock().await.stop_all().await;
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn failed_remote_setup_cleans_replaced_runner_generation_and_cannot_stay_green() {
        let data_dir = unique_state_dir("failed-remote-runner-rollback");
        let resource_dir = data_dir.join("resources");
        std::fs::create_dir_all(&resource_dir).unwrap();
        let local_runner_config = data_dir.join("local-runner.toml");
        let remote_runner_config = data_dir.join("remote-runner.toml");
        let local_user_token = data_dir.join("local-user-token");
        let remote_user_token = data_dir.join("remote-user-token");
        for path in [
            &local_runner_config,
            &remote_runner_config,
            &local_user_token,
            &remote_user_token,
        ] {
            std::fs::write(path, "fixture").unwrap();
        }

        let state = AppState::new(data_dir.clone(), resource_dir).expect("create Desktop state");
        state.set_test_license_active();
        let local_topology = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        };
        {
            let mut slot = state.core.lock().await;
            let core = slot.as_mut().expect("Desktop core available");
            core.config.topology = Some(local_topology.clone());
            core.config.runtime_autostart = Some(true);
            core.snapshot.topology = Some(local_topology.clone());
            core.snapshot.readiness = aggregate_readiness(
                ServerReadiness::Ready,
                RunnerReadiness::Ready,
                ExposureReadiness::LocalReady,
                ProjectReadiness::Ready,
            );
            core.publish_snapshot();
        }

        let local_identity = RunnerRuntimeIdentity {
            runner_config: local_runner_config,
            user_token_file: local_user_token,
            server_url: "http://127.0.0.1:3721".to_string(),
        };
        let remote_identity = RunnerRuntimeIdentity {
            runner_config: remote_runner_config,
            user_token_file: remote_user_token,
            server_url: "https://engine-test.example".to_string(),
        };
        let command = || {
            let mut command = std::process::Command::new("powershell.exe");
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$null = [Console]::In.ReadToEnd()",
            ]);
            command
        };
        state
            .supervisor
            .lock()
            .await
            .spawn_owned_with_target(
                ProcessKind::LocalRunner,
                command(),
                false,
                Some(runner_process_target_key(&local_identity)),
            )
            .await
            .expect("start baseline Local Runner");

        let (operation, cancellation, mut core, baseline) = state
            .begin_operation(DesktopOperationKind::RemoteSetup, true)
            .await
            .expect("begin Remote setup operation");
        assert!(baseline.local_runner.is_some());
        let preparation = core
            .prepare_owned_runner_target(
                &remote_identity,
                false,
                Deadline::after(Duration::from_secs(5)),
                &cancellation,
            )
            .await
            .expect("replace Local Runner with saved Remote target");
        assert!(preparation.replaced);
        core.supervisor
            .lock()
            .await
            .spawn_owned_with_target(
                ProcessKind::LocalRunner,
                command(),
                false,
                Some(runner_process_target_key(&remote_identity)),
            )
            .await
            .expect("start replacement Remote Runner");
        let remote_generation = core
            .supervisor
            .lock()
            .await
            .active_owned_generation(ProcessKind::LocalRunner);
        assert_ne!(baseline.local_runner, remote_generation);

        // Model a failure after the replacement Runner reached readiness but before
        // Remote topology/config commit. The rollback must not trust the old bool
        // baseline and leave this new generation running under the Local config.
        core.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            ExposureReadiness::LocalReady,
            ProjectReadiness::Ready,
        );
        assert!(core.snapshot.readiness.runtime_ready);
        let result: DesktopResult<DesktopStateSnapshot> = Err(DesktopError::new(
            "test_remote_precommit_failure",
            "Forced Remote setup failure before topology commit",
            "Test-only failure.",
        ));
        let error = state
            .finish_operation(operation, cancellation, core, baseline, result)
            .await
            .expect_err("forced Remote setup failure must propagate");
        assert_eq!(error.code, "test_remote_precommit_failure");

        assert!(state
            .supervisor
            .lock()
            .await
            .snapshot(ProcessKind::LocalRunner)
            .is_none());
        let snapshot = state.get_state();
        assert!(matches!(
            snapshot.topology.as_ref().map(|topology| &topology.server),
            Some(ServerTopology::Local)
        ));
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Stopped);
        assert!(!snapshot.readiness.runtime_ready);
        {
            let slot = state.core.lock().await;
            let core = slot.as_ref().expect("Desktop core restored after failure");
            assert!(matches!(
                core.config.topology.as_ref().map(|topology| &topology.server),
                Some(ServerTopology::Local)
            ));
        }

        state.shutdown().await;
        drop(state);
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn windows_remote_full_dogfood_reuses_saved_profile_across_local_switches() {
        if !cfg!(windows) {
            return;
        }
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        let suffix = std::process::id();
        let host_data =
            std::env::temp_dir().join(format!("webcodex-desktop-remote-host-dogfood-{suffix}"));
        let client_data =
            std::env::temp_dir().join(format!("webcodex-desktop-remote-client-dogfood-{suffix}"));
        let _ = std::fs::remove_dir_all(&host_data);
        let _ = std::fs::remove_dir_all(&client_data);

        let mut host = DesktopCore::new(host_data.clone(), host_data.join("test-resources"))
            .expect("create remote dogfood host state");
        let cancellation = CancellationContext::never();
        let host_runtime = host_data.join("runtime");
        let env_file = host_runtime.join("webcodex.env");
        let data_dir = host_runtime.join("data");
        tokio::fs::create_dir_all(&host_runtime)
            .await
            .expect("create remote dogfood host state");
        let listen = reserve_loopback_address().expect("reserve remote dogfood host port");
        let server_url = host
            .adapter
            .init_local_server(&listen, &data_dir, &env_file, &cancellation)
            .await
            .expect("initialize remote dogfood Server")
            .probe_url;
        let command = host
            .adapter
            .local_server_command(&env_file)
            .expect("build remote dogfood Server command");
        host.spawn_owned(ProcessKind::LocalServer, command, false, &cancellation)
            .await
            .expect("start remote dogfood Server");

        let mut client = DesktopCore::new(client_data.clone(), client_data.join("test-resources"))
            .expect("create remote dogfood client state");
        let result: DesktopResult<(
            DesktopStateSnapshot,
            DesktopStateSnapshot,
            DesktopStateSnapshot,
            DesktopStateSnapshot,
        )> = async {
            host.wait_for_server(
                &server_url,
                Some(&env_file),
                None,
                &cancellation,
                Deadline::after(SERVER_READY_TIMEOUT),
                true,
            )
            .await?;

            let first_local = client
                .configure_local_setup(Some(&project), &cancellation)
                .await?;
            let pairing_code = host
                .adapter
                .create_local_pairing(&server_url, &env_file, &cancellation)
                .await?;
            let first_remote = client
                .configure_remote_setup(&server_url, &pairing_code, Some(&project), &cancellation)
                .await?;
            let second_local = client
                .configure_local_setup(Some(&project), &cancellation)
                .await?;
            let second_remote = client
                .configure_remote_setup(&server_url, "", Some(&project), &cancellation)
                .await?;
            client.stop_local_runtime(&cancellation).await?;
            Ok((first_local, first_remote, second_local, second_remote))
        }
        .await;

        client.supervisor.lock().await.stop_all().await;
        host.supervisor.lock().await.stop_all().await;
        let _ = std::fs::remove_dir_all(&client_data);
        let _ = std::fs::remove_dir_all(&host_data);

        let (first_local, first_remote, second_local, second_remote) =
            result.expect("Local -> Remote -> Local -> same Remote dogfood should complete");
        for snapshot in [&first_local, &second_local] {
            assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
            assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
            assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
            assert!(snapshot.readiness.runtime_ready);
            assert!(matches!(
                snapshot.topology.as_ref().map(|topology| &topology.server),
                Some(ServerTopology::Local)
            ));
        }
        for snapshot in [&first_remote, &second_remote] {
            assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
            assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
            assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
            assert!(snapshot.readiness.runtime_ready);
            assert!(matches!(
                snapshot.topology.as_ref().map(|topology| &topology.server),
                Some(ServerTopology::Remote { .. })
            ));
        }
    }

    #[test]
    fn configured_https_exposure_stays_unverified_without_mcp_handoff_evidence() {
        let local = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        };
        let remote = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Remote {
                url: "https://example.com".to_string(),
            },
            runner: RunnerTopology::Local,
            exposure: Exposure::ExistingHttps {
                url: "https://example.com".to_string(),
            },
            enrollment: Enrollment::ManagedPairing,
        };
        assert_eq!(
            exposure_readiness(Some(&local)),
            ExposureReadiness::LocalReady
        );
        assert_eq!(
            exposure_readiness(Some(&remote)),
            ExposureReadiness::Unknown
        );
    }

    #[test]
    fn regular_tunnel_child_death_only_degrades_connection_readiness() {
        let mut state = Some(RegularTunnelState {
            provider: "openai".to_string(),
            status: RegularTunnelStatus::Ready,
            clipboard_state: "copied".to_string(),
            clipboard_contains: "tunnel_id".to_string(),
            ready_for_chatgpt: true,
        });
        assert_eq!(
            regular_tunnel_exposure(&mut state, true),
            Some(ExposureReadiness::LocalReady)
        );
        assert_eq!(
            regular_tunnel_exposure(&mut state, false),
            Some(ExposureReadiness::Error)
        );
        assert_eq!(state.as_ref().unwrap().status, RegularTunnelStatus::Error);
        assert!(!state.as_ref().unwrap().ready_for_chatgpt);

        let readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            ExposureReadiness::Error,
            ProjectReadiness::Ready,
        );
        assert!(readiness.runtime_ready);
        assert!(!readiness.ready_for_chatgpt);
    }

    #[test]
    fn regular_tunnel_local_handoff_waits_for_external_chatgpt_evidence() {
        let mut snapshot = DesktopStateSnapshot::default();
        snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            ExposureReadiness::LocalReady,
            ProjectReadiness::Ready,
        );
        apply_regular_tunnel_next_action(&mut snapshot, &ExposureReadiness::LocalReady);
        assert!(snapshot.readiness.runtime_ready);
        assert!(!snapshot.readiness.ready_for_chatgpt);
        assert_eq!(
            snapshot.readiness.summary_kind,
            ReadinessSummaryKind::TunnelReadyWaitingForChatGpt
        );
        assert_eq!(
            snapshot.readiness.next_action_kind,
            Some(ReadinessNextActionKind::CheckConnection)
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_failed_regular_tunnel_child_cannot_leave_fake_green_readiness() {
        let data_dir = std::env::temp_dir().join(format!(
            "webcodex-desktop-tunnel-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let mut core = DesktopCore::new(data_dir.clone(), data_dir.join("test-resources"))
            .expect("create tunnel failure state");
        let topology = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        };
        core.config.topology = Some(topology.clone());
        core.snapshot.topology = Some(topology);
        core.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            ExposureReadiness::LocalReady,
            ProjectReadiness::Ready,
        );
        core.snapshot.regular_tunnel = Some(RegularTunnelState {
            provider: "openai".to_string(),
            status: RegularTunnelStatus::Ready,
            clipboard_state: "copied".to_string(),
            clipboard_contains: "tunnel_id".to_string(),
            ready_for_chatgpt: true,
        });

        let mut command = std::process::Command::new("cmd.exe");
        command.args(["/D", "/C", "exit", "/B", "23"]);
        core.supervisor
            .lock()
            .await
            .spawn_owned(ProcessKind::RegularTunnel, command, false)
            .await
            .expect("start failing tunnel fixture");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        let snapshot = loop {
            let snapshot = core.get_state().await.expect("observe failed tunnel");
            if snapshot.readiness.exposure == ExposureReadiness::Error {
                break snapshot;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "fake tunnel did not exit"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };

        assert!(snapshot.readiness.runtime_ready);
        assert!(!snapshot.readiness.ready_for_chatgpt);
        assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
        assert_eq!(
            snapshot.readiness.next_action_kind,
            Some(ReadinessNextActionKind::RestartSecureTunnel)
        );
        core.supervisor.lock().await.stop_all().await;
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn regular_tunnel_selection_is_ephemeral_and_local_only() {
        let local = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        };
        let selected = effective_topology(Some(&local), true).unwrap();
        assert_eq!(selected.exposure, Exposure::OpenAiTunnel);
        assert_eq!(local.exposure, Exposure::None);

        let remote = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Remote {
                url: "https://example.test".to_string(),
            },
            runner: RunnerTopology::Local,
            exposure: Exposure::ExistingHttps {
                url: "https://example.test".to_string(),
            },
            enrollment: Enrollment::ManagedPairing,
        };
        assert_eq!(
            effective_topology(Some(&remote), true).unwrap().exposure,
            remote.exposure
        );
    }
}
