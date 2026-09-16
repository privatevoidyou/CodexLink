import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DesktopState } from "./models/topology";
import { LocaleProvider } from "./i18n/locale";

const api = vi.hoisted(() => ({
  getState: vi.fn(),
  activateLicense: vi.fn(),
  updateTunnelConfig: vi.fn(),
  openPowerShellInstallGuide: vi.fn(),
  openProductLink: vi.fn(),
  refresh: vi.fn(),
  observeChatgptActivity: vi.fn(),
  resumeSavedRuntime: vi.fn(),
  updateTunnelProxy: vi.fn(),
  activity: vi.fn(),
  configureLocal: vi.fn(),
  activateLocalProject: vi.fn(),
  removeLocalProject: vi.fn(),
  configureRemote: vi.fn(),
  startQuickShare: vi.fn(),
  stopQuickShare: vi.fn(),
  stopLocalRuntime: vi.fn(),
  startRegularTunnel: vi.fn(),
  stopRegularTunnel: vi.fn(),
  cancelOperation: vi.fn(),
  inspectProject: vi.fn(),
  getLaunchAtLogin: vi.fn(),
  setLaunchAtLogin: vi.fn(),
  setUiLocale: vi.fn(),
}));

const clipboard = vi.hoisted(() => ({ writeText: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => clipboard);

const tauriEvents = vi.hoisted(() => ({
  handler: null as null | ((event: { payload: unknown }) => void),
  listen: vi.fn(),
}));

vi.mock("./lib/desktop-api", () => ({
  desktopApi: api,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: tauriEvents.listen,
}));

import App from "./App";
import { open } from "@tauri-apps/plugin-dialog";

const readyState: DesktopState = {
  license: { status: "activated" },
  topology: {
    experience: "full",
    server: { kind: "local" },
    runner: { kind: "local" },
    exposure: { kind: "none" },
    enrollment: { kind: "managed_pairing" },
  },
  readiness: {
    server: "ready",
    runner: "ready",
    exposure: "local_ready",
    project: "ready",
    runtime_ready: true,
    ready_for_chatgpt: false,
    summary: "Runtime ready on this computer",
    summary_kind: "runtime_ready_local_only",
    next_action: "Choose a ChatGPT connection in Connection.",
    next_action_kind: "choose_connection",
  },
  project: {
    path: "C:\\fixture\\repo",
    allowed_root: "C:\\fixture",
    is_git_repository: true,
    runtime_project_id: "agent:desktop:repo",
  },
  projects: [{
    path: "C:\\fixture\\repo",
    allowed_root: "C:\\fixture",
    is_git_repository: true,
    runtime_project_id: "agent:desktop:repo",
  }],
  binaries: {
    directory: "C:\\fixture\\bin",
    version: "0.3.9",
    git_commit: "0123456789abcdef",
    source: "WEBCODEX_DESKTOP_BIN_DIR",
  },
  quick_share: null,
  regular_tunnel: null,
  activity_sequence: 0,
  openai_tunnel_configured: true,
  openai_tunnel_config: {
    tunnel_id_present: true,
    api_key_present: true,
    source: "environment",
    saved_tunnel_id: null,
  },
  regular_tunnel_available: true,
  runtime_autostart: false,
  preferred_connection: "no_chat_gpt",
  tunnel_proxy: {
    mode: "auto",
    custom_url: null,
    effective_source: "direct",
    effective_url: null,
    detected_url: null,
  },
};

const firstRunState: DesktopState = {
  license: { status: "activated" },
  projects: [],
  readiness: {
    server: "unknown",
    runner: "unknown",
    exposure: "unknown",
    project: "none",
    runtime_ready: false,
    ready_for_chatgpt: false,
    summary: "CodexLink Service needs attention",
    summary_kind: "service_needs_attention",
    next_action: "Start or reconnect the CodexLink Service.",
    next_action_kind: "start_or_reconnect_service",
  },
  activity_sequence: 0,
  openai_tunnel_configured: true,
  openai_tunnel_config: {
    tunnel_id_present: true,
    api_key_present: true,
    source: "environment",
    saved_tunnel_id: null,
  },
  regular_tunnel_available: true,
  runtime_autostart: false,
  preferred_connection: "no_chat_gpt",
  tunnel_proxy: {
    mode: "auto",
    custom_url: null,
    effective_source: "direct",
    effective_url: null,
    detected_url: null,
  },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function setupState(): DesktopState {
  return {
    ...readyState,
    readiness: {
      ...readyState.readiness,
      server: "stopped",
      runner: "stopped",
      exposure: "disabled",
      project: "configured",
      runtime_ready: false,
      ready_for_chatgpt: false,
      summary: "CodexLink Service needs attention",
      summary_kind: "service_needs_attention",
      next_action: "Start or reconnect the CodexLink Service.",
      next_action_kind: "start_or_reconnect_service",
    },
    current_operation: null,
  };
}

function localSetupOperationState(
  phase: "running" | "cancelling" = "running",
  activitySequence = 1,
): DesktopState {
  return {
    ...setupState(),
    activity_sequence: activitySequence,
    current_operation: {
      id: "desktop-operation-a",
      kind: "local_setup",
      phase,
      started_at_ms: 1_000,
      cancellable: true,
    },
  };
}

function renderApp() {
  return render(
    <LocaleProvider>
      <App />
    </LocaleProvider>,
  );
}

async function findHomeHeading() {
  return screen.findByRole("heading", {
    level: 1,
    name: /一切就绪|再完成一步即可使用/,
  });
}

describe("semantic Desktop UI", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriEvents.handler = null;
    tauriEvents.listen.mockImplementation(
      async (_eventName: string, handler: (event: { payload: unknown }) => void) => {
        tauriEvents.handler = handler;
        return () => {
          if (tauriEvents.handler === handler) tauriEvents.handler = null;
        };
      },
    );
    api.activity.mockResolvedValue([]);
    api.getLaunchAtLogin.mockResolvedValue(false);
    api.openPowerShellInstallGuide.mockResolvedValue(undefined);
    api.openProductLink.mockResolvedValue(undefined);
    api.setUiLocale.mockResolvedValue(undefined);
    api.activateLicense.mockResolvedValue(readyState);
    api.setLaunchAtLogin.mockImplementation(async (enabled: boolean) => enabled);
    api.resumeSavedRuntime.mockResolvedValue(readyState);
    api.observeChatgptActivity.mockResolvedValue(readyState);
    api.updateTunnelProxy.mockResolvedValue(readyState);
    api.updateTunnelConfig.mockResolvedValue(readyState);
    api.startRegularTunnel.mockResolvedValue(readyState);
    api.stopRegularTunnel.mockResolvedValue(readyState);
  });

  it("shows device activation first, then starts onboarding and bootstraps the runtime in the background", async () => {
    const unactivated: DesktopState = {
      ...firstRunState,
      license: { status: "not_activated" },
    };
    const activated: DesktopState = {
      ...unactivated,
      license: { status: "activated" },
    };
    api.getState.mockResolvedValueOnce(unactivated).mockResolvedValueOnce(activated);
    api.activateLicense.mockResolvedValue(activated);
    api.resumeSavedRuntime.mockResolvedValue(readyState);

    renderApp();

    await screen.findByRole("heading", { name: "激活 CodexLink", level: 1 });
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
    const input = screen.getByLabelText("销售激活码");
    expect(input).toHaveAttribute("placeholder", "MYWS-XXXXX-XXXXX-XXXXX-XXXXX");
    expect(screen.getByText("首次使用时，请完成设备激活。")).toBeInTheDocument();
    fireEvent.change(input, { target: { value: "MYWS-ABCDE-12345-FGHIJ-67890" } });
    fireEvent.click(screen.getByRole("button", { name: "激活 CodexLink" }));

    await waitFor(() => expect(api.activateLicense).toHaveBeenCalledWith("MYWS-ABCDE-12345-FGHIJ-67890"));
    expect(await screen.findByRole("heading", { name: "添加 CodexLink 到 ChatGPT", level: 1 })).toBeInTheDocument();
    expect(window.localStorage.getItem("codexlink.desktop.v1.onboarding")).toBe("chatgpt");
    await waitFor(() => expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole("button", { name: "我已经添加过，跳过" }));
    await findHomeHeading();
    expect(window.localStorage.getItem("codexlink.desktop.v1.onboarding")).toBe("complete");
  });

  it("starts a newly activated device without Tunnel credentials at onboarding step 1", async () => {
    const noTunnel = {
      tunnel_id_present: false,
      api_key_present: false,
      source: "environment" as const,
      saved_tunnel_id: null,
    };
    const unactivated: DesktopState = {
      ...firstRunState,
      license: { status: "not_activated" },
      openai_tunnel_configured: false,
      openai_tunnel_config: noTunnel,
    };
    const activated: DesktopState = { ...unactivated, license: { status: "activated" } };
    const bootstrapped: DesktopState = {
      ...readyState,
      projects: [],
      openai_tunnel_configured: false,
      openai_tunnel_config: noTunnel,
      preferred_connection: "no_chat_gpt",
    };
    api.getState.mockResolvedValueOnce(unactivated).mockResolvedValueOnce(activated);
    api.activateLicense.mockResolvedValue(activated);
    api.resumeSavedRuntime.mockResolvedValue(bootstrapped);

    renderApp();
    const activationCode = await screen.findByLabelText("销售激活码");
    fireEvent.change(activationCode, { target: { value: "MYWS-ABCDE-12345-FGHIJ-67890" } });
    fireEvent.click(screen.getByRole("button", { name: "激活 CodexLink" }));

    expect(await screen.findByRole("heading", { name: "连接 ChatGPT", level: 1 })).toBeInTheDocument();
    expect(screen.getByLabelText("Tunnel ID")).toHaveValue("");
    expect(screen.getByLabelText("Restricted Runtime Key")).toHaveValue("");
    expect(window.localStorage.getItem("codexlink.desktop.v1.onboarding")).toBe("tunnel");
  });

  it("saves onboarding Tunnel credentials securely and advances to ChatGPT guidance", async () => {
    const missing: DesktopState = {
      ...readyState,
      readiness: { ...readyState.readiness, runtime_ready: false },
      openai_tunnel_configured: false,
      openai_tunnel_config: {
        tunnel_id_present: false,
        api_key_present: false,
        source: "environment",
        saved_tunnel_id: null,
      },
    };
    const configured: DesktopState = {
      ...missing,
      openai_tunnel_configured: true,
      openai_tunnel_config: {
        tunnel_id_present: true,
        api_key_present: true,
        source: "file",
        saved_tunnel_id: "tunnel_fixture",
        effective_tunnel_id: "tunnel_fixture",
      },
    };
    window.localStorage.setItem("codexlink.desktop.v1.onboarding", "tunnel");
    api.getState.mockResolvedValue(missing);
    api.updateTunnelConfig.mockResolvedValue(configured);

    renderApp();
    await screen.findByRole("heading", { name: "连接 ChatGPT", level: 1 });
    fireEvent.click(screen.getByRole("button", { name: "创建 Tunnel" }));
    await waitFor(() => expect(api.openProductLink).toHaveBeenCalledWith("openai_tunnels"));
    fireEvent.change(screen.getByLabelText("Tunnel ID"), { target: { value: "tunnel_fixture" } });
    fireEvent.change(screen.getByLabelText("Restricted Runtime Key"), { target: { value: "runtime-key-fixture" } });
    fireEvent.click(screen.getByRole("button", { name: "下一步" }));

    await waitFor(() => expect(api.updateTunnelConfig).toHaveBeenCalledWith({
      action: "save",
      tunnelId: "tunnel_fixture",
      apiKey: "runtime-key-fixture",
    }));
    expect(await screen.findByRole("heading", { name: "添加 CodexLink 到 ChatGPT", level: 1 })).toBeInTheDocument();
    expect(screen.queryByDisplayValue("runtime-key-fixture")).not.toBeInTheDocument();
    expect(window.localStorage.getItem("codexlink.desktop.v1.onboarding")).toBe("chatgpt");
  });

  it("lets first-use onboarding be skipped without changing runtime readiness", async () => {
    const missing: DesktopState = {
      ...readyState,
      readiness: { ...readyState.readiness, runtime_ready: false, ready_for_chatgpt: false },
      openai_tunnel_configured: false,
      openai_tunnel_config: {
        tunnel_id_present: false,
        api_key_present: false,
        source: "environment",
        saved_tunnel_id: null,
      },
    };
    window.localStorage.setItem("codexlink.desktop.v1.onboarding", "tunnel");
    api.getState.mockResolvedValue(missing);

    renderApp();
    await screen.findByRole("heading", { name: "连接 ChatGPT", level: 1 });
    fireEvent.click(screen.getByRole("button", { name: "我已经配置过，跳过" }));

    await findHomeHeading();
    expect(api.updateTunnelConfig).not.toHaveBeenCalled();
    expect(window.localStorage.getItem("codexlink.desktop.v1.onboarding")).toBe("complete");
    expect(screen.getAllByText("尚未连接").length).toBeGreaterThan(0);
  });

  it("resumes pending onboarding but leaves existing activated upgrades on Home", async () => {
    window.localStorage.setItem("codexlink.desktop.v1.onboarding", "chatgpt");
    api.getState.mockResolvedValue(readyState);
    const pending = renderApp();
    expect(await screen.findByRole("heading", { name: "添加 CodexLink 到 ChatGPT", level: 1 })).toBeInTheDocument();
    pending.unmount();

    window.localStorage.removeItem("codexlink.desktop.v1.onboarding");
    renderApp();
    await findHomeHeading();
    expect(screen.queryByRole("heading", { name: "添加 CodexLink 到 ChatGPT", level: 1 })).not.toBeInTheDocument();
  });

  it("reopens onboarding from Settings without making it pending on the next restart", async () => {
    api.getState.mockResolvedValue(readyState);
    renderApp();
    await findHomeHeading();
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(await screen.findByRole("button", { name: "重新打开首次使用向导" }));

    expect(await screen.findByRole("heading", { name: "连接 ChatGPT", level: 1 })).toBeInTheDocument();
    expect(window.localStorage.getItem("codexlink.desktop.v1.onboarding")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "我已经配置过，跳过" }));
    await findHomeHeading();
    expect(window.localStorage.getItem("codexlink.desktop.v1.onboarding")).toBeNull();
  });

  it("routes the onboarding advanced link into the existing setup chooser", async () => {
    window.localStorage.setItem("codexlink.desktop.v1.onboarding", "tunnel");
    api.getState.mockResolvedValue({
      ...readyState,
      openai_tunnel_configured: false,
      openai_tunnel_config: {
        tunnel_id_present: false,
        api_key_present: false,
        source: "environment",
        saved_tunnel_id: null,
      },
    });
    renderApp();
    await screen.findByRole("heading", { name: "连接 ChatGPT", level: 1 });
    fireEvent.click(screen.getByRole("button", { name: "使用其他连接方式" }));

    expect(await screen.findByRole("heading", { name: "你希望如何使用这台电脑？", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /连接现有 Server/ })).toBeInTheDocument();
    expect(window.localStorage.getItem("codexlink.desktop.v1.onboarding")).toBe("complete");
  });

  it("localizes activation failures instead of showing backend English", async () => {
    const unactivated: DesktopState = {
      ...firstRunState,
      license: { status: "not_activated" },
    };
    api.getState.mockResolvedValue(unactivated);
    api.activateLicense.mockRejectedValue({
      code: "activation_code_bound_elsewhere",
      message: "This activation code is already bound to another device",
      next_action: "Use the device that was first activated, or contact support.",
    });

    renderApp();
    const input = await screen.findByLabelText("销售激活码");
    fireEvent.change(input, { target: { value: "MYWS-ABCDE-12345-FGHIJ-67890" } });
    fireEvent.click(screen.getByRole("button", { name: "激活 CodexLink" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("此激活码已绑定到另一台设备");
    expect(screen.queryByText(/Use the device that was first activated/i)).not.toBeInTheDocument();
  });

  it("shows customer-level recovery without exposing licensing internals", async () => {
    api.getState.mockResolvedValue({
      ...firstRunState,
      license: { status: "invalid", reason_code: "device_identity_missing" },
    });

    renderApp();

    await screen.findByRole("heading", { name: "CodexLink 激活需要恢复", level: 1 });
    expect(screen.getByText(/请重新输入购买后收到的激活码/)).toBeInTheDocument();
    const input = screen.getByLabelText("销售激活码");
    expect(input).toBeEnabled();
    fireEvent.change(input, { target: { value: "MYWS-ABCDE-12345-FGHIJ-67890" } });
    expect(screen.getByRole("button", { name: "激活 CodexLink" })).toBeEnabled();
    expect(document.body.textContent).not.toMatch(/Ed25519|offline verification|private key|plaintext/i);
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();
  });

  it("keeps a user stop consistent after refresh and offers Start", async () => {
    const stopped = setupState();
    api.getState.mockResolvedValue(stopped);
    api.refresh.mockResolvedValue(stopped);
    renderApp();
    await findHomeHeading();
    expect(screen.getByRole("button", { name: /^启动/ })).toBeEnabled();
    expect(screen.queryByText("当前工作区")).not.toBeInTheDocument();
    expect(screen.queryByText(/运行环境正常/)).not.toBeInTheDocument();
    expect(screen.queryByText(/服务需要处理/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    await waitFor(() => expect(api.refresh).toHaveBeenCalledOnce());
    expect(screen.getByRole("button", { name: /^启动/ })).toBeEnabled();
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();
  });

  it("shows the active Tunnel ID without exposing the Restricted Runtime Key", async () => {
    api.getState.mockResolvedValue({
      ...readyState,
      openai_tunnel_config: { ...readyState.openai_tunnel_config, effective_tunnel_id: "tunnel_fixture" },
      regular_tunnel: { provider: "openai", status: "ready", clipboard_state: "unavailable", clipboard_contains: "tunnel_id", ready_for_chatgpt: true },
    });
    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    expect(screen.getByText("tunnel_fixture")).toBeInTheDocument();
    expect(screen.queryByText("Restricted Runtime Key")).not.toBeInTheDocument();
    expect(document.body.textContent).not.toContain("test-only-key");
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
    expect(api.stopRegularTunnel).not.toHaveBeenCalled();
  });

  it("keeps Activity diagnostics available through the host event without putting it in main navigation", async () => {
    api.getState.mockResolvedValue(readyState);
    api.activity.mockResolvedValue([
      { sequence: 1, timestamp_ms: 1, source: "desktop", level: "info", event_kind: "operation_started", message: "Desktop operation started: local_project_activate" },
      { sequence: 2, timestamp_ms: 2, source: "runner", level: "info", event_kind: "process_started", message: "" },
      { sequence: 3, timestamp_ms: 3, source: "desktop", level: "info", event_kind: "project_activated", message: "sample-project" },
    ]);
    renderApp();
    await findHomeHeading();
    expect(screen.queryByRole("button", { name: "活动" })).not.toBeInTheDocument();
    await waitFor(() => expect(tauriEvents.handler).not.toBeNull());
    act(() => tauriEvents.handler?.({ payload: "activity" }));
    await screen.findByText("已切换到 sample-project");
    expect(screen.getAllByRole("article")[0]).toHaveTextContent("已切换到 sample-project");
    expect(screen.getAllByRole("article")).toHaveLength(2);
    expect(screen.getAllByRole("article")[1]).toHaveTextContent("正在激活项目");
  });

  it("retries a failed Secure Link without exposing provider choices", async () => {
    api.getState.mockResolvedValue(readyState);
    api.startRegularTunnel.mockRejectedValueOnce({ code: "tunnel_unavailable", message: "Tunnel failed", next_action: "Retry." });
    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    expect(screen.queryByRole("radiogroup")).not.toBeInTheDocument();
    const retry = screen.getByRole("button", { name: "重新建立安全链路" });
    fireEvent.click(retry);
    expect(await screen.findByRole("alert")).toHaveTextContent("tunnel_unavailable");
    await waitFor(() => expect(retry).toBeEnabled());
    fireEvent.click(retry);
    await waitFor(() => expect(api.startRegularTunnel).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("keeps stop failures visible and never starts a replacement tunnel implicitly", async () => {
    api.getState.mockResolvedValue({
      ...readyState,
      regular_tunnel: { provider: "openai", status: "error", clipboard_state: "unavailable", clipboard_contains: "tunnel_id", ready_for_chatgpt: false },
    });
    api.stopRegularTunnel.mockRejectedValueOnce({ code: "tunnel_unavailable", message: "Stop failed", next_action: "Retry." });
    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    const stop = document.querySelector<HTMLButtonElement>('[data-webcodex-action="stop-regular-tunnel"]')!;
    fireEvent.click(stop);
    expect(await screen.findByRole("alert")).toHaveTextContent("Stop failed");
    await waitFor(() => expect(stop).toBeEnabled());
    fireEvent.click(stop);
    await waitFor(() => expect(screen.getByRole("button", { name: "重新建立安全链路" })).toBeEnabled());
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
  });

  it("shows authorized paths without inventing a primary workspace and handles picker errors", async () => {
    api.getState.mockResolvedValue(readyState);
    vi.mocked(open).mockRejectedValueOnce({ code: "project_invalid", message: "Picker unavailable", next_action: "Retry." });
    renderApp();
    await findHomeHeading();
    expect(screen.getByText("已授权 1 个本地路径供 ChatGPT App 访问。")).toBeInTheDocument();
    expect(screen.queryByText("当前工作区")).not.toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "查看项目" })[0]);
    expect(screen.getByRole("heading", { level: 1, name: "此电脑上的项目" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "repo" })).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: /添加项目/ })[0]);
    expect(await screen.findByRole("alert")).toHaveTextContent("Picker unavailable");
    expect(api.activateLocalProject).not.toHaveBeenCalled();
    expect(api.configureLocal).not.toHaveBeenCalled();
  });

  it("saves Tunnel credentials in Settings, clears the secret, and automatically prepares Secure Link", async () => {
    api.getState.mockResolvedValue(readyState);
    const savedState: DesktopState = {
      ...readyState,
      openai_tunnel_configured: true,
      openai_tunnel_config: {
        ...readyState.openai_tunnel_config,
        tunnel_id_present: true,
        api_key_present: true,
        source: "file",
        saved_tunnel_id: "tunnel_saved",
      },
    };
    const linkedState: DesktopState = {
      ...savedState,
      regular_tunnel: { provider: "openai", status: "ready", clipboard_state: "unavailable", clipboard_contains: "tunnel_id", ready_for_chatgpt: true },
    };
    api.updateTunnelConfig.mockResolvedValue(savedState);
    api.startRegularTunnel.mockResolvedValue(linkedState);
    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    fireEvent.change(screen.getByLabelText("Tunnel ID"), { target: { value: "tunnel_saved" } });
    const key = screen.getByLabelText("Restricted Runtime Key");
    expect(key).toHaveAttribute("type", "password");
    fireEvent.change(key, { target: { value: "test-only-key" } });
    fireEvent.click(screen.getByRole("button", { name: "保存 Tunnel 凭据" }));
    await waitFor(() => expect(api.updateTunnelConfig).toHaveBeenCalledWith({ action: "save", tunnelId: "tunnel_saved", apiKey: "test-only-key" }));
    expect(key).toHaveValue("");
    await waitFor(() => expect(api.startRegularTunnel).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/Tunnel 凭据已安全保存并应用/)).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("test-only-key");
  });

  it("clears submitted Runtime Keys on save failure and never submits project setup from a credential input", async () => {
    api.getState.mockResolvedValue(readyState);
    api.updateTunnelConfig.mockRejectedValue({ code: "tunnel_config_save_failed", message: "Could not save", next_action: "Retry." });
    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    const tunnelInput = screen.getByLabelText("Tunnel ID");
    fireEvent.change(tunnelInput, { target: { value: "tunnel_test" } });
    const key = screen.getByLabelText("Restricted Runtime Key");
    fireEvent.change(key, { target: { value: "test-only-key" } });
    const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    key.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(api.configureLocal).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "保存 Tunnel 凭据" }));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(key).toHaveValue("");
    expect(tunnelInput).toHaveValue("tunnel_test");
  });

  it("keeps keyboard navigation working without intercepting Activity search typing", async () => {
    api.getState.mockResolvedValue(readyState);
    api.activity.mockResolvedValue([
      { sequence: 1, timestamp_ms: 1, source: "runner", level: "info", event_kind: "process_started", message: "" },
      { sequence: 2, timestamp_ms: 2, source: "service", level: "error", event_kind: "process_exited", message: "" },
    ]);
    renderApp();
    await findHomeHeading();
    await waitFor(() => expect(tauriEvents.handler).not.toBeNull());
    act(() => tauriEvents.handler?.({ payload: "activity" }));
    const search = await screen.findByRole("searchbox");
    await waitFor(() => expect(screen.getAllByRole("article")).toHaveLength(1));
    fireEvent.click(screen.getByRole("checkbox", { name: "显示进程详情" }));
    expect(screen.getAllByRole("article")).toHaveLength(2);
    fireEvent.change(search, { target: { value: "no matching source" } });
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
    fireEvent.keyDown(search, { key: "a", ctrlKey: true });
    expect(screen.getByRole("searchbox")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "2", ctrlKey: true });
    expect(screen.getByRole("button", { name: "项目" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("adds another authorized local path in place while preserving an active Tunnel", async () => {
    const tunneledState: DesktopState = {
      ...readyState,
      topology: { ...readyState.topology!, exposure: { kind: "open_ai_tunnel" } },
      regular_tunnel: {
        provider: "openai",
        status: "ready",
        clipboard_state: "copied",
        clipboard_contains: "tunnel_id",
        ready_for_chatgpt: true,
      },
      preferred_connection: "open_ai_tunnel",
    };
    const projectC = {
      path: "C:\\work\\next",
      allowed_root: "C:\\work\\next",
      is_git_repository: true,
      runtime_project_id: "agent:desktop:next",
    };
    const switched: DesktopState = {
      ...tunneledState,
      project: projectC,
      projects: [...tunneledState.projects, projectC],
    };
    api.getState.mockResolvedValue(tunneledState);
    api.observeChatgptActivity.mockResolvedValue(tunneledState);
    api.activateLocalProject.mockResolvedValue(switched);
    vi.mocked(open).mockResolvedValue(projectC.path);

    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(screen.getAllByRole("button", { name: /添加项目/ })[0]);

    await waitFor(() => expect(api.activateLocalProject).toHaveBeenCalledWith(projectC.path));
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { level: 2, name: "repo" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "next" })).toBeInTheDocument();
    expect(screen.getByText(projectC.path)).toBeInTheDocument();
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
  });

  it("removes one authorized path through the Desktop backend instead of hiding only the card", async () => {
    const secondProject = {
      path: "D:\\Mina_UE",
      allowed_root: "D:\\Mina_UE",
      is_git_repository: true,
      runtime_project_id: "agent:desktop:mina-ue",
    };
    const multiProject: DesktopState = {
      ...readyState,
      projects: [...readyState.projects, secondProject],
    };
    const afterRemoval: DesktopState = {
      ...readyState,
      projects: [...readyState.projects],
    };
    api.getState.mockResolvedValue(multiProject);
    api.observeChatgptActivity.mockResolvedValue(multiProject);
    api.removeLocalProject.mockResolvedValue(afterRemoval);

    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    expect(screen.getByRole("heading", { level: 2, name: "Mina_UE" })).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "移除项目" })[1]);

    await waitFor(() => expect(api.removeLocalProject).toHaveBeenCalledWith(secondProject.path));
    expect(screen.queryByRole("heading", { level: 2, name: "Mina_UE" })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "repo" })).toBeInTheDocument();
  });

  it("adds a first authorized path on an already-ready local runtime without reopening topology choices", async () => {
    const noAuthorizedPaths: DesktopState = { ...readyState, projects: [] };
    const configured: DesktopState = { ...readyState, projects: [readyState.project!] };
    api.getState.mockResolvedValue(noAuthorizedPaths);
    api.observeChatgptActivity.mockResolvedValue(noAuthorizedPaths);
    api.activateLocalProject.mockResolvedValue(configured);
    vi.mocked(open).mockResolvedValue(readyState.project!.path);

    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(screen.getAllByRole("button", { name: /添加项目/ })[0]);

    await waitFor(() => expect(api.activateLocalProject).toHaveBeenCalledWith(readyState.project!.path));
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: /连接现有 Server/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /快速共享项目/ })).not.toBeInTheDocument();
  });

  it("falls back to bounded local setup for a legacy Runner without restarting Tunnel", async () => {
    const tunneledState: DesktopState = {
      ...readyState,
      topology: { ...readyState.topology!, exposure: { kind: "open_ai_tunnel" } },
      regular_tunnel: {
        provider: "openai",
        status: "ready",
        clipboard_state: "copied",
        clipboard_contains: "tunnel_id",
        ready_for_chatgpt: true,
      },
      preferred_connection: "open_ai_tunnel",
    };
    const projectC = { ...readyState.project!, path: "C:\\work\\legacy", allowed_root: "C:\\work\\legacy" };
    const switched: DesktopState = { ...tunneledState, project: projectC, projects: [...tunneledState.projects, projectC] };
    api.getState.mockResolvedValue(tunneledState);
    api.observeChatgptActivity.mockResolvedValue(tunneledState);
    api.activateLocalProject.mockRejectedValue({
      code: "project_activation_restart_required",
      message: "Runner restart required",
      next_action: "Refresh this Runner.",
    });
    api.configureLocal.mockResolvedValue(switched);
    vi.mocked(open).mockResolvedValue(projectC.path);

    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(screen.getAllByRole("button", { name: /添加项目/ })[0]);

    await waitFor(() => expect(api.activateLocalProject).toHaveBeenCalledWith(projectC.path));
    await waitFor(() => expect(api.configureLocal).toHaveBeenCalledWith(projectC.path));
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
  });

  it("does not start a duplicate Tunnel when local setup runs while one is already active", async () => {
    const tunneledState: DesktopState = {
      ...readyState,
      topology: { ...readyState.topology!, exposure: { kind: "open_ai_tunnel" } },
      regular_tunnel: {
        provider: "openai",
        status: "ready",
        clipboard_state: "copied",
        clipboard_contains: "tunnel_id",
        ready_for_chatgpt: true,
      },
      preferred_connection: "open_ai_tunnel",
    };
    api.getState.mockResolvedValue(tunneledState);
    api.observeChatgptActivity.mockResolvedValue(tunneledState);
    api.configureLocal.mockResolvedValue(tunneledState);

    renderApp();
    await findHomeHeading();
    fireEvent.click(screen.getByRole("button", { name: "其他配置方式" }));
    fireEvent.click(screen.getByRole("button", { name: /在此电脑使用 CodexLink/ }));
    expect(screen.queryByRole("checkbox", { name: "配置完成后连接 ChatGPT" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "配置 CodexLink" }));

    await waitFor(() => expect(api.configureLocal).toHaveBeenCalledWith(tunneledState.project!.path));
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
    expect(await findHomeHeading()).toBeInTheDocument();
  });

  it("exposes exactly the four first-release navigation pages and no provider chooser", async () => {
    api.getState.mockResolvedValue(readyState);
    api.refresh.mockResolvedValue(readyState);
    renderApp();

    await findHomeHeading();
    const home = screen.getByRole("button", { name: "首页" });
    const connection = screen.getByRole("button", { name: "连接" });
    expect(home).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "项目" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "设置" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "活动" })).not.toBeInTheDocument();

    fireEvent.click(connection);
    expect(await screen.findByRole("heading", { level: 1, name: "ChatGPT 连接" })).toBeInTheDocument();
    expect(connection).toHaveAttribute("aria-current", "page");
    expect(home).not.toHaveAttribute("aria-current");
    expect(screen.queryByRole("radiogroup")).not.toBeInTheDocument();
    expect(screen.getByText("运行环境就绪")).toBeInTheDocument();
    expect(screen.getByText("Tunnel 凭据已配置")).toBeInTheDocument();
  });

  it("marks ChatGPT accessible when the verified Secure Link is ready without requiring observed activity", async () => {
    const accessible: DesktopState = {
      ...readyState,
      regular_tunnel: { provider: "openai", status: "ready", clipboard_state: "unavailable", clipboard_contains: "tunnel_id", ready_for_chatgpt: true },
      chatgpt_activity: null,
    };
    api.getState.mockResolvedValue(accessible);
    api.refresh.mockResolvedValue(accessible);
    api.observeChatgptActivity.mockResolvedValue(accessible);
    renderApp();

    await findHomeHeading();
    expect(screen.getByRole("listitem", { name: "ChatGPT" })).toHaveClass("ready");
    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    expect(screen.getByRole("listitem", { name: "ChatGPT 可访问" })).toHaveClass("complete");
    expect(screen.getAllByText("ChatGPT 可访问").length).toBeGreaterThan(0);
  });

  it("keeps the connection path healthy with zero authorized projects and guides the user to add one", async () => {
    const connectedWithoutProjects: DesktopState = {
      ...readyState,
      project: undefined,
      projects: [],
      readiness: {
        ...readyState.readiness,
        project: "none",
        runtime_ready: true,
        ready_for_chatgpt: false,
        summary: "Project is not ready",
        summary_kind: "project_not_ready",
        next_action: "Prepare and activate the selected project.",
        next_action_kind: "add_or_reload_project",
      },
      regular_tunnel: { provider: "openai", status: "ready", clipboard_state: "unavailable", clipboard_contains: "tunnel_id", ready_for_chatgpt: true },
      chatgpt_activity: null,
    };
    api.getState.mockResolvedValue(connectedWithoutProjects);
    api.refresh.mockResolvedValue(connectedWithoutProjects);
    api.observeChatgptActivity.mockResolvedValue(connectedWithoutProjects);
    renderApp();

    await findHomeHeading();
    expect(screen.getByRole("listitem", { name: "本机" })).toHaveClass("ready");
    expect(screen.getByRole("listitem", { name: "Engine" })).toHaveClass("ready");
    expect(screen.getByRole("listitem", { name: "Secure Link" })).toHaveClass("ready");
    expect(screen.getByRole("listitem", { name: "ChatGPT" })).toHaveClass("pending");
    const nextActions = screen.getByRole("heading", { level: 2, name: "接下来做什么" }).closest("section") as HTMLElement | null;
    expect(nextActions).not.toBeNull();
    expect(within(nextActions!).getByRole("button", { name: /^添加项目/ })).toBeEnabled();
    const systemStatus = screen.getByRole("heading", { level: 2, name: "系统状态" }).closest("section");
    expect(systemStatus).not.toBeNull();
    const chatgptStatus = within(systemStatus!).getByText("ChatGPT 连接").closest(".system-status-line") as HTMLElement | null;
    expect(chatgptStatus).not.toBeNull();
    expect(within(chatgptStatus!).getByText("尚未连接")).toBeInTheDocument();
  });

  it("treats a verified ready Secure Link as customer-facing ChatGPT accessibility", async () => {
    const linkReady: DesktopState = {
      ...readyState,
      regular_tunnel: { provider: "openai", status: "ready", clipboard_state: "unavailable", clipboard_contains: "tunnel_id", ready_for_chatgpt: true },
      chatgpt_activity: null,
    };
    api.getState.mockResolvedValue(linkReady);
    api.refresh.mockResolvedValue(linkReady);
    api.observeChatgptActivity.mockResolvedValue(linkReady);
    renderApp();

    await findHomeHeading();
    expect(screen.getByRole("listitem", { name: "Secure Link" })).toHaveClass("ready");
    expect(screen.getByRole("listitem", { name: "ChatGPT" })).toHaveClass("ready");
    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    expect(screen.getByRole("listitem", { name: "ChatGPT 可访问" })).toHaveClass("complete");
  });

  it("keeps ChatGPT activity observation as background evidence when the user returns", async () => {
    vi.useFakeTimers();
    try {
      const observed: DesktopState = {
        ...readyState,
        regular_tunnel: { provider: "openai", status: "ready", clipboard_state: "unavailable", clipboard_contains: "tunnel_id", ready_for_chatgpt: true },
        chatgpt_activity: {
          observed: true,
          last_meaningful_activity_at_ms: 5_000,
        },
      };
      let serverObserved = false;
      api.getState.mockImplementation(async () => serverObserved ? observed : readyState);
      api.observeChatgptActivity.mockImplementation(async () => serverObserved ? observed : readyState);
      const view = renderApp();
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(api.observeChatgptActivity).toHaveBeenCalledTimes(1);
      expect(screen.getByRole("listitem", { name: "ChatGPT" })).toHaveClass("pending");

      await act(async () => {
        window.dispatchEvent(new Event("blur"));
        await Promise.resolve();
      });
      serverObserved = true;
      await act(async () => {
        window.dispatchEvent(new Event("focus"));
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(api.observeChatgptActivity).toHaveBeenCalledTimes(2);
      expect(screen.getByRole("listitem", { name: "ChatGPT" })).toHaveClass("pending");

      await act(async () => {
        await vi.advanceTimersByTimeAsync(60_000);
      });
      expect(api.observeChatgptActivity).toHaveBeenCalledTimes(2);
      view.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps Tunnel credential diagnostics presence-only and secret-free in Settings", async () => {
    const missingKey: DesktopState = {
      ...readyState,
      openai_tunnel_configured: false,
      openai_tunnel_config: {
        tunnel_id_present: true,
        api_key_present: false,
        source: "environment",
        saved_tunnel_id: null,
      },
    };
    api.getState.mockResolvedValue(missingKey);
    renderApp();
    await findHomeHeading();
    fireEvent.click(screen.getByRole("button", { name: "设置" }));

    expect(screen.getByText("Tunnel 凭据")).toBeInTheDocument();
    expect(screen.getByLabelText("Restricted Runtime Key")).toHaveValue("");
    expect(document.body.textContent).not.toContain("CONTROL_PLANE_API_KEY");
    expect(document.body.textContent).not.toContain("test-only-key");
    fireEvent.click(screen.getByText("高级"));
    api.getState.mockResolvedValueOnce(missingKey);
    fireEvent.click(screen.getByRole("button", { name: "重新检测配置" }));
    await waitFor(() => expect(api.getState).toHaveBeenCalledTimes(2));
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
  });

  it("navigates to existing Activity and Settings pages from the tray host event", async () => {
    api.getState.mockResolvedValue(readyState);
    renderApp();
    await findHomeHeading();
    await waitFor(() => expect(tauriEvents.handler).not.toBeNull());

    act(() => {
      tauriEvents.handler?.({ payload: "settings" });
    });
    expect(await screen.findByRole("heading", { level: 1, name: "设置" })).toBeInTheDocument();

    act(() => {
      tauriEvents.handler?.({ payload: "activity" });
    });
    expect(await screen.findByRole("heading", { level: 1, name: "最近的运行活动" })).toBeInTheDocument();
    await waitFor(() => expect(api.activity).toHaveBeenCalled());

    act(() => {
      tauriEvents.handler?.({ payload: "https://example.invalid" });
    });
    expect(screen.getByRole("heading", { level: 1, name: "最近的运行活动" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "活动" })).not.toBeInTheDocument();
  });

  it("reads and updates Launch at Login through the narrow Desktop host API", async () => {
    api.getState.mockResolvedValue(readyState);
    renderApp();
    await findHomeHeading();
    fireEvent.click(screen.getByRole("button", { name: "设置" }));

    const launchAtLogin = await screen.findByRole("checkbox", { name: "登录时启动 CodexLink" });
    await waitFor(() => expect(launchAtLogin).toBeEnabled());
    expect(launchAtLogin).not.toBeChecked();
    expect(screen.getByText(/CodexLink 会在后台启动/)).toBeInTheDocument();

    fireEvent.click(launchAtLogin);
    await waitFor(() => expect(api.setLaunchAtLogin).toHaveBeenCalledWith(true));
    await waitFor(() => expect(launchAtLogin).toBeChecked());
  });

  it("bootstraps a fresh activated Desktop on its internal runtime anchor without asking for a user path", async () => {
    const freshWithoutTunnel: DesktopState = {
      ...firstRunState,
      openai_tunnel_configured: false,
      openai_tunnel_config: {
        tunnel_id_present: false,
        api_key_present: false,
        source: "environment",
        saved_tunnel_id: null,
      },
    };
    const bootstrapped: DesktopState = {
      ...readyState,
      projects: [],
      openai_tunnel_configured: false,
      openai_tunnel_config: freshWithoutTunnel.openai_tunnel_config,
      preferred_connection: "no_chat_gpt",
    };
    api.getState.mockResolvedValue(freshWithoutTunnel);
    api.resumeSavedRuntime.mockResolvedValue(bootstrapped);
    renderApp();

    await findHomeHeading();
    expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1);
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
    expect(screen.queryByRole("heading", { level: 1, name: "在此电脑配置 CodexLink" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "项目" }));
    expect(await screen.findByText("0")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /添加项目/ }).every((button) => !button.hasAttribute("disabled"))).toBe(true);
  });

  it("keeps PowerShell guidance available only in the explicit local setup flow", async () => {
    const missingPwsh: DesktopState = {
      ...readyState,
      powershell_runtime: {
        pwsh_available: false,
        windows_powershell_available: true,
      },
    };
    const detectedPwsh: DesktopState = {
      ...missingPwsh,
      powershell_runtime: {
        pwsh_available: true,
        windows_powershell_available: true,
      },
    };
    api.getState.mockResolvedValueOnce(missingPwsh).mockResolvedValueOnce(detectedPwsh);
    renderApp();
    await findHomeHeading();
    expect(screen.queryByText("建议安装 PowerShell 7")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "其他配置方式" }));
    fireEvent.click(screen.getByRole("button", { name: /在此电脑使用 CodexLink/ }));
    fireEvent.click(screen.getByText("诊断"));
    const guidance = screen.getByText("建议安装 PowerShell 7").closest("article");
    expect(guidance).not.toBeNull();
    expect(guidance).toHaveTextContent("Windows PowerShell 5.1");
    expect(guidance).toHaveTextContent("winget install --id Microsoft.PowerShell --source winget");

    fireEvent.click(within(guidance!).getByRole("button", { name: "打开 Microsoft 安装说明" }));
    await waitFor(() => expect(api.openPowerShellInstallGuide).toHaveBeenCalledTimes(1));
    fireEvent.click(within(guidance!).getByRole("button", { name: "重新检测" }));
    await waitFor(() => expect(screen.queryByText("建议安装 PowerShell 7")).not.toBeInTheDocument());
  });

  it("keeps a fresh internal-runtime bootstrap failure visible without forcing a project choice", async () => {
    const bootstrapError = {
      code: "server_unreachable",
      message: "CodexLink Service did not become ready",
      next_action: "Retry the local runtime.",
    };
    api.getState.mockResolvedValue(firstRunState);
    api.resumeSavedRuntime.mockRejectedValueOnce(bootstrapError).mockResolvedValueOnce(readyState);
    renderApp();

    expect(await screen.findByRole("alert")).toHaveTextContent("server_unreachable");
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "选择文件夹" })).not.toBeInTheDocument();

    expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1);
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "其他配置方式" })).toBeEnabled();
  });

  it("shows an initial status failure and retries the complete fresh-start bootstrap", async () => {
    const retryState = deferred<DesktopState>();
    api.getState
      .mockRejectedValueOnce({
        code: "desktop_state_unavailable",
        message: "Desktop status is temporarily unavailable",
        next_action: "Retry reading the Desktop status.",
      })
      .mockReturnValueOnce(retryState.promise)
      .mockResolvedValue(readyState);
    renderApp();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("desktop_state_unavailable");
    expect(screen.queryByText("正在加载 CodexLink…")).not.toBeInTheDocument();
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("status")).toHaveTextContent("正在加载 CodexLink…");
    expect(screen.queryByRole("button", { name: "重试" })).not.toBeInTheDocument();
    await waitFor(() => expect(api.getState).toHaveBeenCalledTimes(2));

    await act(async () => {
      retryState.resolve(firstRunState);
    });
    await findHomeHeading();
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps repeated initial status failures visible without automatically retrying", async () => {
    api.getState.mockRejectedValue({
      code: "desktop_state_unavailable",
      message: "Desktop status is unavailable",
      next_action: "Retry reading the Desktop status.",
    });
    renderApp();

    expect(await screen.findByRole("alert")).toHaveTextContent("desktop_state_unavailable");
    expect(api.getState).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("desktop_state_unavailable");
    expect(screen.getByRole("button", { name: "重试" })).toBeEnabled();
    expect(api.getState).toHaveBeenCalledTimes(2);
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();
  });

  it("renders localized failures as an alert while keeping safe diagnostics available", async () => {
    const setupState: DesktopState = {
      ...readyState,
      readiness: {
        ...readyState.readiness,
        runtime_ready: false,
        ready_for_chatgpt: false,
        server: "stopped",
        runner: "stopped",
        project: "configured",
        exposure: "disabled",
        summary_kind: "service_needs_attention",
        next_action_kind: "start_or_reconnect_service",
      },
    };
    api.getState.mockResolvedValue(setupState);
    api.refresh.mockResolvedValue(setupState);
    api.resumeSavedRuntime.mockRejectedValue({
      code: "server_unreachable",
      message: "CodexLink Service did not become ready",
      next_action: "Check diagnostics.",
    });

    renderApp();
    const submit = await screen.findByRole("button", { name: /^启动/ });
    fireEvent.click(submit);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("CodexLink 服务不可用");
    expect(alert).toHaveTextContent("server_unreachable");
  });

  it("offers a product recovery action when the selected project did not load", async () => {
    api.getState.mockResolvedValue(readyState);
    api.configureLocal
      .mockRejectedValueOnce({
        code: "project_not_loaded",
        message: "The selected project did not become ready in the Desktop-owned Runner",
        next_action: "Retry project setup.",
      })
      .mockResolvedValueOnce(readyState);
    renderApp();
    await findHomeHeading();
    fireEvent.click(screen.getByRole("button", { name: "其他配置方式" }));
    fireEvent.click(screen.getByRole("button", { name: /在此电脑使用 CodexLink/ }));
    fireEvent.click(screen.getByRole("button", { name: "配置 CodexLink" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("项目尚未就绪");
    expect(alert).not.toHaveTextContent("registry");
    fireEvent.click(screen.getByRole("button", { name: "重新激活项目" }));
    await waitFor(() => expect(api.configureLocal).toHaveBeenCalledTimes(2));
  });

  it("uses the backend restart_quick_share presentation identity after a stopped share", async () => {
    const stoppedShare: DesktopState = {
      ...readyState,
      topology: {
        experience: "quick_share",
        server: { kind: "local" },
        runner: { kind: "local" },
        exposure: { kind: "cloudflare" },
        enrollment: { kind: "existing_profile", profile: "temporary_share" },
      },
      readiness: {
        server: "stopped",
        runner: "stopped",
        exposure: "error",
        project: "configured",
        runtime_ready: false,
        ready_for_chatgpt: false,
        summary: "Quick Share stopped",
        summary_kind: "quick_share_stopped",
        next_action: "Start Quick Share again.",
        next_action_kind: "restart_quick_share",
      },
      quick_share: {
        provider: "cloudflare",
        project: "C:\\fixture\\repo",
        mcp_url: null,
        clipboard_state: "unavailable",
        clipboard_contains: "bearer_credential",
        ready_for_chatgpt: false,
      },
    };
    api.getState.mockResolvedValue(stoppedShare);
    api.refresh.mockResolvedValue(stoppedShare);
    renderApp();

    expect(await screen.findByText("重新启动 Quick Share。")).toBeInTheDocument();
  });

  it("does not offer a duplicate start when the regular tunnel is running but handoff is degraded", async () => {
    const degradedTunnel: DesktopState = {
      ...readyState,
      topology: {
        ...readyState.topology!,
        exposure: { kind: "open_ai_tunnel" },
      },
      readiness: {
        ...readyState.readiness,
        exposure: "degraded",
        ready_for_chatgpt: false,
        summary: "ChatGPT connection is not verified",
        summary_kind: "connection_unverified",
        next_action: "Restore clipboard access, then restart the secure tunnel handoff.",
        next_action_kind: "restore_clipboard_handoff",
      },
      regular_tunnel: {
        provider: "openai",
        status: "ready",
        clipboard_state: "unavailable",
        clipboard_contains: "tunnel_id",
        ready_for_chatgpt: false,
      },
    };
    api.getState.mockResolvedValue(degradedTunnel);
    api.refresh.mockResolvedValue(degradedTunnel);
    renderApp();
    await findHomeHeading();

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    expect(screen.queryByRole("radiogroup")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新建立安全链路" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止安全隧道" })).toBeInTheDocument();
  });

  it("keeps navigation and Activity usable while a pending setup is globally observable", async () => {
    vi.useFakeTimers();
    try {
      const initial = setupState();
      const running = localSetupOperationState("running", 1);
      const runningWithNewActivity = localSetupOperationState("running", 2);
      const setupResult = deferred<DesktopState>();
      api.getState
        .mockResolvedValueOnce(initial)
        .mockResolvedValueOnce(running)
        .mockResolvedValueOnce(runningWithNewActivity)
        .mockResolvedValue(runningWithNewActivity);
      api.refresh.mockResolvedValue(initial);
      api.resumeSavedRuntime.mockReturnValue(setupResult.promise);
      api.activity.mockResolvedValue([]);

      const view = renderApp();
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });

      fireEvent.click(screen.getByRole("button", { name: /^启动/ }));
      expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_500);
      });

      const operationStatus = screen.getByRole("status", { name: "当前 Desktop 操作" });
      expect(operationStatus).toHaveTextContent("正在配置本机 CodexLink");
      expect(operationStatus).toHaveTextContent("停止当前操作不会自动重复执行尚未确认的步骤");

      expect(tauriEvents.handler).not.toBeNull();
      act(() => tauriEvents.handler?.({ payload: "activity" }));
      await act(async () => { await Promise.resolve(); });
      expect(screen.getByRole("heading", { level: 1, name: "最近的运行活动" })).toBeInTheDocument();
      expect(api.activity).toHaveBeenCalledTimes(1);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_000);
      });
      expect(api.activity).toHaveBeenCalledTimes(2);

      setupResult.resolve(readyState);
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(screen.queryByRole("status", { name: "当前 Desktop 操作" })).not.toBeInTheDocument();
      view.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels only the observed operation id and presents the cancelling phase", async () => {
    const running = localSetupOperationState("running", 1);
    const cancelling = localSetupOperationState("cancelling", 2);
    api.getState.mockResolvedValueOnce(running).mockResolvedValue(cancelling);
    api.cancelOperation.mockResolvedValue(cancelling);

    renderApp();
    const cancel = await screen.findByRole("button", { name: "停止当前操作" });
    fireEvent.click(cancel);

    await waitFor(() => {
      expect(api.cancelOperation).toHaveBeenCalledWith("desktop-operation-a");
    });
    const cancellingStatus = screen.getByRole("status", { name: "当前 Desktop 操作" });
    expect(cancellingStatus).toHaveTextContent("正在停止当前操作…");
    const disabledCancel = screen.getByRole("button", { name: "正在停止当前操作…" });
    expect(disabledCancel).toBeDisabled();
    fireEvent.click(disabledCancel);
    expect(api.cancelOperation).toHaveBeenCalledTimes(1);
  });

  it("does not let an older polling response resurrect a completed operation", async () => {
    vi.useFakeTimers();
    try {
      const running = localSetupOperationState("running", 1);
      const stalePoll = deferred<DesktopState>();
      const terminal = { ...readyState, current_operation: null, activity_sequence: 2 };
      api.getState
        .mockResolvedValueOnce(running)
        .mockReturnValueOnce(stalePoll.promise)
        .mockResolvedValue(terminal);
      api.cancelOperation.mockResolvedValue(terminal);

      const view = renderApp();
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(screen.getByRole("status", { name: "当前 Desktop 操作" })).toBeInTheDocument();

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_000);
      });
      expect(api.getState).toHaveBeenCalledTimes(2);

      fireEvent.click(screen.getByRole("button", { name: "停止当前操作" }));
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(screen.queryByRole("status", { name: "当前 Desktop 操作" })).not.toBeInTheDocument();

      stalePoll.resolve(running);
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(screen.queryByRole("status", { name: "当前 Desktop 操作" })).not.toBeInTheDocument();
      view.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("drops ChatGPT accessibility immediately when a verified Regular Tunnel degrades", async () => {
    vi.useFakeTimers();
    try {
      const locallyReadyTunnel: DesktopState = {
        ...readyState,
        topology: { ...readyState.topology!, exposure: { kind: "open_ai_tunnel" } },
        readiness: {
          ...readyState.readiness,
          exposure: "local_ready",
          ready_for_chatgpt: false,
          summary: "OpenAI Secure Tunnel is ready; waiting for ChatGPT to connect",
          summary_kind: "tunnel_ready_waiting_for_chat_gpt",
          next_action: "Connect the Tunnel in ChatGPT, then verify it with one real project read.",
          next_action_kind: "check_connection",
        },
        regular_tunnel: {
          provider: "openai",
          status: "ready",
          clipboard_state: "copied",
          clipboard_contains: "tunnel_id",
          ready_for_chatgpt: true,
        },
        chatgpt_activity: null,
      };
      const failedTunnel: DesktopState = {
        ...locallyReadyTunnel,
        readiness: {
          ...locallyReadyTunnel.readiness,
          exposure: "error",
          ready_for_chatgpt: false,
          summary: "ChatGPT connection is not verified",
          summary_kind: "connection_unverified",
          next_action: "Restart the secure tunnel.",
          next_action_kind: "restart_secure_tunnel",
        },
        regular_tunnel: { ...locallyReadyTunnel.regular_tunnel!, status: "error", ready_for_chatgpt: false },
      };

      api.getState.mockResolvedValueOnce(locallyReadyTunnel).mockResolvedValueOnce(failedTunnel);
      api.refresh.mockResolvedValue(locallyReadyTunnel);
      api.observeChatgptActivity.mockResolvedValue(locallyReadyTunnel);
      const view = renderApp();
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(screen.getByRole("listitem", { name: "Secure Link" })).toHaveClass("ready");
      expect(screen.getByRole("listitem", { name: "ChatGPT" })).toHaveClass("ready");
      expect(api.getState).toHaveBeenCalledTimes(1);

      await act(async () => { await vi.advanceTimersByTimeAsync(1_500); });
      expect(api.getState).toHaveBeenCalledTimes(2);
      expect(api.refresh).toHaveBeenCalledTimes(0);
      expect(screen.getByRole("listitem", { name: "Secure Link" })).toHaveClass("pending");
      expect(screen.getByRole("listitem", { name: "ChatGPT" })).toHaveClass("pending");

      view.unmount();
      const callsAfterUnmount = api.getState.mock.calls.length;
      await act(async () => { await vi.advanceTimersByTimeAsync(3_000); });
      expect(api.getState).toHaveBeenCalledTimes(callsAfterUnmount);
    } finally {
      vi.useRealTimers();
    }
  });

  it("automatically repairs a remembered Tunnel that drops after launch", async () => {
    vi.useFakeTimers();
    try {
      const connected: DesktopState = {
        ...readyState,
        topology: { ...readyState.topology!, exposure: { kind: "open_ai_tunnel" } },
        runtime_autostart: true,
        preferred_connection: "open_ai_tunnel",
        readiness: {
          ...readyState.readiness,
          exposure: "local_ready",
          ready_for_chatgpt: true,
          summary: "Ready to use with ChatGPT",
          summary_kind: "ready_for_chat_gpt",
          next_action: null,
          next_action_kind: null,
        },
        regular_tunnel: {
          provider: "openai",
          status: "ready",
          clipboard_state: "copied",
          clipboard_contains: "tunnel_id",
          ready_for_chatgpt: true,
        },
      };
      const failedTunnel: DesktopState = {
        ...connected,
        readiness: {
          ...connected.readiness,
          exposure: "error",
          ready_for_chatgpt: false,
          summary: "ChatGPT connection is not verified",
          summary_kind: "connection_unverified",
          next_action: "Restart the secure tunnel.",
          next_action_kind: "restart_secure_tunnel",
        },
        regular_tunnel: {
          ...connected.regular_tunnel!,
          status: "error",
          ready_for_chatgpt: false,
        },
      };

      api.getState
        .mockResolvedValueOnce(connected)
        .mockResolvedValueOnce(failedTunnel)
        .mockResolvedValue(connected);
      api.resumeSavedRuntime
        .mockResolvedValueOnce(connected)
        .mockResolvedValueOnce(failedTunnel);
      api.startRegularTunnel.mockResolvedValue(connected);

      const view = renderApp();
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1);
      expect(api.startRegularTunnel).toHaveBeenCalledTimes(0);
      api.resumeSavedRuntime.mockClear();
      api.startRegularTunnel.mockClear();

      await act(async () => { await vi.advanceTimersByTimeAsync(1_500); });
      expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1);
      expect(api.startRegularTunnel).toHaveBeenCalledTimes(1);
      expect(screen.getByRole("listitem", { name: "Secure Link" })).toHaveClass("ready");
      expect(screen.getByRole("listitem", { name: "ChatGPT" })).toHaveClass("ready");

      view.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps regular tunnel controls off a remote Server connection page", async () => {
    const remote: DesktopState = {
      ...readyState,
      topology: {
        experience: "full",
        server: { kind: "remote", url: "https://server.example.test" },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: "https://server.example.test" },
        enrollment: { kind: "managed_pairing" },
      },
      runtime_autostart: false,
      readiness: {
        ...readyState.readiness,
        exposure: "unknown",
        ready_for_chatgpt: false,
        summary_kind: "connection_unverified",
        next_action_kind: "check_connection",
      },
    };
    api.getState.mockResolvedValue(remote);
    api.refresh.mockResolvedValue(remote);
    renderApp();
    await findHomeHeading();

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    expect(await screen.findByRole("heading", { level: 2, name: "远程 CodexLink Server" })).toBeInTheDocument();
    expect(screen.getByText("由远程 Server 管理")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "启动安全隧道" })).not.toBeInTheDocument();
  });

  it("automatically resumes a saved full runtime on Desktop launch", async () => {
    const stopped = { ...setupState(), runtime_autostart: true };
    api.getState.mockResolvedValue(stopped);
    api.resumeSavedRuntime.mockResolvedValue(readyState);

    renderApp();

    await waitFor(() => expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1));
    expect(await findHomeHeading()).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "配置 CodexLink" })).not.toBeInTheDocument();
  });

  it("reconnects the remembered OpenAI Tunnel after restoring the runtime", async () => {
    const stopped: DesktopState = {
      ...setupState(),
      runtime_autostart: true,
      preferred_connection: "open_ai_tunnel",
    };
    const resumed: DesktopState = {
      ...readyState,
      runtime_autostart: true,
      preferred_connection: "open_ai_tunnel",
    };
    api.getState.mockResolvedValue(stopped);
    api.resumeSavedRuntime.mockResolvedValue(resumed);
    api.startRegularTunnel.mockResolvedValue(resumed);

    renderApp();

    await waitFor(() => expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(api.startRegularTunnel).toHaveBeenCalledTimes(1));
  });

  it("reuses an existing remote enrollment when the user selects a different project", async () => {
    const remoteState: DesktopState = {
      ...readyState,
      topology: {
        experience: "full",
        server: { kind: "remote", url: "https://server.example.test" },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: "https://server.example.test" },
        enrollment: { kind: "managed_pairing" },
      },
      project: {
        path: "C:\\fixture\\project-a",
        allowed_root: "C:\\fixture\\project-a",
        is_git_repository: true,
        runtime_project_id: "agent:desktop:project-a",
      },
    };
    const projectB = {
      path: "C:\\fixture\\project-b",
      allowed_root: "C:\\fixture\\project-b",
      is_git_repository: true,
      runtime_project_id: null,
    };
    api.getState.mockResolvedValue(remoteState);
    api.inspectProject.mockResolvedValue(projectB);
    api.configureRemote.mockResolvedValue({
      ...remoteState,
      project: { ...projectB, runtime_project_id: "agent:desktop:project-b" },
    });
    vi.mocked(open).mockResolvedValue(projectB.path);
    renderApp();
    await findHomeHeading();

    fireEvent.click(screen.getByRole("button", { name: "其他配置方式" }));
    fireEvent.click(screen.getByRole("button", { name: /连接现有 Server/ }));
    expect(screen.getByText("将复用现有连接")).toBeInTheDocument();
    expect(screen.queryByLabelText("一次性登录码")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "更改文件夹" }));
    await waitFor(() => expect(api.inspectProject).toHaveBeenCalledWith(projectB.path));
    expect(screen.getByText("将复用现有连接")).toBeInTheDocument();
    expect(screen.queryByLabelText("一次性登录码")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新连接电脑" }));
    await waitFor(() =>
      expect(api.configureRemote).toHaveBeenCalledWith(
        "https://server.example.test",
        "",
        projectB.path,
      ),
    );
  });

  it("exposes pairing recovery when a saved remote enrollment is no longer reusable", async () => {
    const remoteState: DesktopState = {
      ...readyState,
      topology: {
        experience: "full",
        server: { kind: "remote", url: "https://server.example.test" },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: "https://server.example.test" },
        enrollment: { kind: "managed_pairing" },
      },
      project: {
        path: "C:\\fixture\\project-a",
        allowed_root: "C:\\fixture\\project-a",
        is_git_repository: true,
        runtime_project_id: "agent:desktop:project-a",
      },
    };
    const projectB = {
      path: "C:\\fixture\\project-b",
      allowed_root: "C:\\fixture\\project-b",
      is_git_repository: true,
      runtime_project_id: null,
    };
    api.getState.mockResolvedValue(remoteState);
    api.inspectProject.mockResolvedValue(projectB);
    api.configureRemote
      .mockRejectedValueOnce({
        code: "pairing_code_invalid",
        message: "The saved Runner identity is not reusable and no new CodexLink pairing code was provided",
        next_action: "Refresh this Runner connection with a new code.",
      })
      .mockResolvedValueOnce({
        ...remoteState,
        project: { ...projectB, runtime_project_id: "agent:desktop:project-b" },
      });
    vi.mocked(open).mockResolvedValue(projectB.path);
    renderApp();
    await findHomeHeading();

    fireEvent.click(screen.getByRole("button", { name: "其他配置方式" }));
    fireEvent.click(screen.getByRole("button", { name: /连接现有 Server/ }));
    fireEvent.click(screen.getByRole("button", { name: "更改文件夹" }));
    await waitFor(() => expect(api.inspectProject).toHaveBeenCalledWith(projectB.path));
    expect(screen.queryByLabelText("一次性登录码")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新连接电脑" }));
    expect(await screen.findByLabelText("一次性登录码")).toBeInTheDocument();
    expect(screen.queryByText("将复用现有连接")).not.toBeInTheDocument();
    const submit = screen.getByRole("button", { name: "连接电脑" });
    expect(submit).toBeDisabled();

    fireEvent.change(screen.getByLabelText("一次性登录码"), {
      target: { value: "wc_pair_recovery" },
    });
    expect(submit).toBeEnabled();
    fireEvent.click(submit);
    await waitFor(() =>
      expect(api.configureRemote).toHaveBeenLastCalledWith(
        "https://server.example.test",
        "wc_pair_recovery",
        projectB.path,
      ),
    );
  });

  it("keeps Remote Server as a first-class runtime choice after local setup", async () => {
    api.getState.mockResolvedValue(readyState);
    renderApp();
    await findHomeHeading();

    fireEvent.click(screen.getByRole("button", { name: "其他配置方式" }));

    expect(await screen.findByRole("button", { name: /连接现有 Server/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /在此电脑使用 CodexLink/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /快速共享项目/ })).toBeInTheDocument();
  });

  it("allows Local setup with zero authorized roots", async () => {
    const zeroRootLocal: DesktopState = {
      ...readyState,
      project: undefined,
      projects: [],
      readiness: {
        ...readyState.readiness,
        project: "none",
        runtime_ready: true,
        ready_for_chatgpt: false,
        summary: "Project is not ready",
        summary_kind: "project_not_ready",
        next_action: "Prepare and activate the selected project.",
        next_action_kind: "add_or_reload_project",
      },
    };
    api.getState.mockResolvedValue(zeroRootLocal);
    api.configureLocal.mockResolvedValue(zeroRootLocal);
    renderApp();
    await findHomeHeading();

    fireEvent.click(screen.getByRole("button", { name: "其他配置方式" }));
    fireEvent.click(document.querySelector<HTMLButtonElement>('[data-webcodex-action="choose-local-setup"]')!);
    const submit = document.querySelector<HTMLButtonElement>('[data-webcodex-action="configure-local"]')!;
    expect(submit).toBeEnabled();
    fireEvent.click(submit);
    await waitFor(() => expect(api.configureLocal).toHaveBeenCalledWith(null));
  });

  it("allows Remote saved-profile recovery with zero authorized roots and no new pairing code", async () => {
    const remoteZeroRoot: DesktopState = {
      ...readyState,
      topology: {
        experience: "full",
        server: { kind: "remote", url: "https://server.example.test" },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: "https://server.example.test" },
        enrollment: { kind: "managed_pairing" },
      },
      project: undefined,
      projects: [],
      readiness: {
        ...readyState.readiness,
        server: "ready",
        runner: "ready",
        exposure: "remote_ready",
        project: "none",
        runtime_ready: true,
        ready_for_chatgpt: false,
        summary: "Project is not ready",
        summary_kind: "project_not_ready",
        next_action: "Prepare and activate the selected project.",
        next_action_kind: "add_or_reload_project",
      },
      openai_tunnel_configured: false,
      regular_tunnel: null,
    };
    const localZeroRootStart: DesktopState = {
      ...readyState,
      project: undefined,
      projects: [],
      readiness: {
        ...readyState.readiness,
        project: "none",
        runtime_ready: true,
        ready_for_chatgpt: false,
        summary: "Project is not ready",
        summary_kind: "project_not_ready",
        next_action: "Prepare and activate the selected project.",
        next_action_kind: "add_or_reload_project",
      },
    };
    api.getState.mockResolvedValue(localZeroRootStart);
    api.configureRemote.mockResolvedValue(remoteZeroRoot);
    renderApp();
    await findHomeHeading();

    fireEvent.click(screen.getByRole("button", { name: "其他配置方式" }));
    fireEvent.click(document.querySelector<HTMLButtonElement>('[data-webcodex-action="choose-remote-setup"]')!);
    fireEvent.change(document.querySelector<HTMLInputElement>('#setup-server-url')!, {
      target: { value: "https://server.example.test" },
    });
    const submit = document.querySelector<HTMLButtonElement>('[data-webcodex-action="configure-remote"]')!;
    expect(submit).toBeEnabled();
    fireEvent.click(submit);
    await waitFor(() =>
      expect(api.configureRemote).toHaveBeenCalledWith(
        "https://server.example.test",
        "",
        null,
      ),
    );
  });

  it("projects Remote readiness without requiring local OpenAI Tunnel state", async () => {
    const remoteReady: DesktopState = {
      ...readyState,
      topology: {
        experience: "full",
        server: { kind: "remote", url: "https://server.example.test" },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: "https://server.example.test" },
        enrollment: { kind: "managed_pairing" },
      },
      readiness: {
        ...readyState.readiness,
        server: "ready",
        runner: "ready",
        exposure: "remote_ready",
        project: "ready",
        runtime_ready: true,
        ready_for_chatgpt: true,
        summary: "Ready to use with ChatGPT",
        summary_kind: "ready_for_chat_gpt",
        next_action: null,
        next_action_kind: null,
      },
      openai_tunnel_configured: false,
      openai_tunnel_config: {
        tunnel_id_present: false,
        api_key_present: false,
        source: "environment",
        saved_tunnel_id: null,
      },
      regular_tunnel: null,
    };
    api.getState.mockResolvedValue(remoteReady);
    renderApp();

    expect(await screen.findByRole("heading", { level: 1, name: "一切就绪" })).toBeInTheDocument();
    expect(screen.getByRole("listitem", { name: "远程 CodexLink Server" })).toHaveClass("ready");
    expect(screen.queryByRole("listitem", { name: "Secure Link" })).not.toBeInTheDocument();
  });

  it("does not paint a saved Remote Server URL green while the real probe is offline", async () => {
    const remoteOffline: DesktopState = {
      ...readyState,
      topology: {
        experience: "full",
        server: { kind: "remote", url: "https://server.example.test" },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: "https://server.example.test" },
        enrollment: { kind: "managed_pairing" },
      },
      readiness: {
        ...readyState.readiness,
        server: "offline",
        runner: "unknown",
        exposure: "error",
        project: "ready",
        runtime_ready: false,
        ready_for_chatgpt: false,
        summary: "CodexLink Service needs attention",
        summary_kind: "service_needs_attention",
        next_action: "Start or reconnect the CodexLink Service.",
        next_action_kind: "start_or_reconnect_service",
      },
      regular_tunnel: null,
    };
    api.getState.mockResolvedValue(remoteOffline);
    renderApp();
    await findHomeHeading();
    fireEvent.click(screen.getByRole("button", { name: "连接详情" }));

    const serverValue = await screen.findByText("https://server.example.test · 离线");
    expect(serverValue.closest("dd")?.querySelector(".status-dot")).toHaveClass("pending");
  });
});
