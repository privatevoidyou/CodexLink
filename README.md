# CodexLink

[English](README.md) | [简体中文](README.zh-CN.md)

**CodexLink connects ChatGPT, Claude, and other AI agents to the real projects on your own machine — your code never leaves it.**

Ask your assistant to read a repository, make a guarded edit, run the test suite, or investigate a failure. CodexLink executes that work through a local Runner inside the project roots you explicitly registered, and hands the model tool results instead of your filesystem. There is no hosted workspace to upload to, and no inbound port to open.

CodexLink is a product built on the [WebCodex](#engine-and-license) engine (Apache-2.0). The product layer — Windows desktop installer, project and authority management, runtime lifecycle, safe-by-default execution policy, diagnostics, and release discipline — belongs to CodexLink; the engine and runtime contracts are synchronized from upstream.

## How it fits together

```text
AI client (ChatGPT / Claude / Custom GPT)
   |
   |  MCP / GPT Actions / REST
   v
CodexLink Server        auth · policy · audit · session ledger
   |
   |  QUIC / WebSocket / polling  (the Runner dials out)
   v
Runner on your machine
   |
   +-- registered project roots
   +-- Git
   +-- compilers / tests / toolchain
```

Two invariants are worth knowing before anything else:

- **The Server never scans your disk and never reads project files.** Projects are registered by the Runner; the Server only addresses them by runtime project id `agent:<client_id>:<project_id>`.
- **The Runner always connects outward.** No public IP, no port forwarding, no inbound firewall rule — it works behind NAT and inside a private network.

For the full Server/Runner architecture, protocol surfaces, and authority boundaries, see [Architecture](docs/ARCHITECTURE.md), [MCP](docs/MCP.md), and [Authentication](docs/AUTH_MODEL.md).

## What you can do

- **Understand and edit code** — read, search, inspect, and make guarded changes inside configured projects.
- **Use the real toolchain** — run commands, tests, formatters, compilers, and project-specific tooling on the machine that owns the repository.
- **Work with Git** — inspect status and diffs while keeping repository operations visible and reviewable.
- **Handle long-running work** — jobs stay observable and resumable instead of requiring one model turn to stay open.
- **Keep a human in the loop** — the Runtime Console and task workflow let you guide, cancel, accept, or reject work where those actions are available.

## Safe by default

Managed CodexLink Runners start in **safe execution mode**. While the master execution gate is disabled, the model is not exposed to raw shell, arbitrary process or script execution, detached processes, persistent shells, SSH execution, validation execution, or coding-agent subprocesses. Typed file, patch, and LSP operations remain available.

Be clear about what that means: this is a **policy baseline, not an OS sandbox**. A process running under your normal Windows user token can still hold broader filesystem rights than the logical project root unless the operating system enforces the boundary. Keep `allowed_root` on a concrete project directory rather than a drive root, and read [SECURITY.md](SECURITY.md) for the complete model.

## Install and connect

The recommended path on Windows is **CodexLink Desktop + the official OpenAI Secure MCP Tunnel**:

1. Run the CodexLink installer (Windows x64, per-user install).
2. Add the project you want the assistant to work on, and keep the registered root narrow.
3. Create the tunnel and the runtime credential in the app.
4. Add the MCP connection in ChatGPT.

Follow [Desktop installation](docs/desktop-install.md) for the exact steps, and [Using Desktop](docs/desktop-guide.md) for projects, connections, and background operation afterwards.

## Versions

CodexLink reports two versions, and they are deliberately separate:

- **CodexLink product version** — what you installed (installer, activation, desktop shell).
- **WebCodex Engine version** — the runtime baseline the build was made from (currently `0.4.1`).

The About and diagnostic surfaces show both. A release record pins the product version, the source commit, the engine version, the tunnel client build, and the installer hash together.

## Platforms

- **Windows x64** — the CodexLink desktop product: local Server + Runner, official OpenAI Secure Tunnel, per-user installer.
- **Windows arm64** — engine CLI + Runner, local foreground Server, and `share`; the desktop installer is x64 only for now.
- **Linux x64/arm64** — engine-level `share`, Server, and Runner workflows.
- **macOS x64/arm64** — engine-level Desktop local Server + Runner, OpenAI Secure Tunnel, `share`, and standalone Runner workflows.

## Engine-level trial

The engine CLI can open a temporary, single-project, restricted environment for a quick look. This uses the upstream npm package, not the CodexLink product:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

`share` prints the connection values and stops working when the command exits. It is for trials and short-lived sharing, not a daily setup.

## Documentation

The documentation below is complete and current, but its body text is still being migrated from the engine name to the CodexLink product name. File paths and CLI names follow the engine.

- [Desktop installation](docs/desktop-install.md) — Windows/macOS path: Desktop + official OpenAI Secure Tunnel
- [Using Desktop](docs/desktop-guide.md) — projects, connections, activity, background operation
- [Full Setup](docs/PERSONAL_SETUP.md) — CLI, existing Server, self-hosting, advanced Server + Runner
- [Quick Trial](docs/QUICK_START.md) — temporarily try one repository with `share`
- [MCP](docs/MCP.md) — ChatGPT, Claude, authentication choices, MCP reference
- [Deployment](docs/DEPLOYMENT.md) — production, self-hosting, systemd/Docker, OAuth, proxies
- [Troubleshooting](docs/TROUBLESHOOTING.md) — connection and runtime problems
- [CLI](docs/CLI.md) — commands and credentials
- [Security](SECURITY.md) — security model and operational guidance
- [Delivery roadmap](docs/CODEXLINK_DELIVERY_ROADMAP.md) — product staging, safety gates, release policy
- [Documentation index](docs/INDEX.md) — all user and contributor documentation

## Build from source

Server and engine binaries:

```bash
cargo build --release --workspace --bins
```

Desktop application (Tauri + React):

```bash
cd apps/desktop
npm ci
npm run tauri build
```

## Contributing

Contributions are welcome, including contributions produced with CodexLink itself or another coding agent. Development workflow, review expectations, and the repository rules live in [CONTRIBUTING.md](CONTRIBUTING.md) and [AGENTS.md](AGENTS.md).

## Engine and license

This repository is a modified work derived from the **WebCodex** project and is distributed under the Apache License 2.0. Upstream copyright notices and the license text are retained in [LICENSE](LICENSE); changes made for CodexLink are documented in the commit history and in [the delivery roadmap](docs/CODEXLINK_DELIVERY_ROADMAP.md).

CodexLink is an independent product name and identity. The engine keeps its own name, version line, CLI, and documentation so that upstream synchronization stays traceable.

## Acknowledgements

Thanks to the WebCodex maintainers and contributors, whose engine makes this product possible, and to the [LINUX DO](https://linux.do/) community for a welcoming place to discuss and share technical work.
