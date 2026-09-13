# CodexLink Delivery Roadmap

> Status: active working plan for the first commercial CodexLink release.
>
> This document defines the order of work, safety gates, checkpoint discipline, and release policy. Do not skip a stage because a later feature is more visible or easier to demo.

## 1. Working principles

CodexLink is a product layer built on a selectively maintained WebCodex engine baseline. The customer-facing product, installer, license flow, diagnostics, project workflow, and website belong to CodexLink. Upstream WebCodex remains the engine/security/runtime source that we selectively synchronize.

Every meaningful stage follows the same sequence:

1. make a bounded change;
2. run focused automated validation;
3. run real Windows validation where the behavior touches Windows/runtime/install/security;
4. inspect the diff and commit it;
5. push/store the checkpoint in the private CodexLink GitHub repository;
6. only then continue to the next stage.

Never allow the only copy of unreleased source to exist on one local disk.

## 2. Non-negotiable safety rules

### 2.1 No unbounded destructive shell operations

For product code and our own development workflow, do not use broad string-built destructive commands such as recursive `rmdir`, `rm -rf`, recursive PowerShell deletion, wildcard deletion, or equivalent shell text when a typed operation can be used.

Prefer, in order:

1. typed file APIs with an exact project-relative target and content/SHA precondition;
2. direct executable + literal argv when a process must be started;
3. bounded script execution only when shell semantics are genuinely required.

A future CodexLink recursive-delete helper must fail closed and require all of the following:

- canonicalized target path;
- target must not be a filesystem root, user-profile root, configured project root, share root, or other protected root;
- target must be under a CodexLink-owned expected parent;
- a CodexLink-created sentinel/ownership marker must be present when deleting managed temporary state;
- the operation must be bounded and auditable;
- unexpected scope/size must stop the operation rather than continue.

### 2.2 Model-authored arbitrary local execution is off by default

Current safety checkpoint: `c087b47442704fc966d498b31ea25f99ccf40e75`.

CodexLink-managed Runners default to safe execution mode. Model-authored raw shell, arbitrary process execution, arbitrary script execution, detached processes, persistent shell, SSH execution, validation execution, and coding-agent subprocess execution are not exposed while the master execution gate is disabled.

Typed file/patch/LSP operations and bounded Runner-generated internal operations remain available.

This is a safety baseline, not a replacement for an OS sandbox. A process running under the normal Windows user token can still possess broader filesystem rights than the logical project root unless Windows itself enforces the boundary.

### 2.3 Project-root authorization is not an OS sandbox

`allowed_root` answers “what CodexLink intends to authorize”, but it does not by itself remove the Windows account's filesystem rights from a subprocess. Therefore:

- do not present project-root authorization as a complete security boundary;
- re-evaluate drive-root authorization (`C:\`, `D:\`) before the first commercial release;
- prefer a concrete project directory in the commercial default;
- evaluate a real Windows sandbox before allowing broad execution capabilities.

### 2.4 Stop-the-line rule

If an operation unexpectedly expands scope, targets a parent/root path, produces an unexplained destructive diff, or loses its expected precondition, stop immediately. Do not “clean up” by issuing broader delete/reset commands.

## 3. Stage 1 — Development infrastructure and security baseline

### Goals

- make `main` the single formal CodexLink development line;
- keep short-lived recovery branches only as temporary insurance;
- ensure each accepted checkpoint exists in the private GitHub repository;
- document the security model and destructive-operation rules;
- diagnose the local Windows Git-to-GitHub TLS problem without blocking source backup;
- keep upstream WebCodex as a clearly named upstream remote/source, not as the CodexLink product identity.

### Gate to close this stage

- private GitHub `main` points at an accepted CodexLink checkpoint;
- clean and repeatable checkpoint procedure exists;
- safety rules are documented and followed by the tooling/workflow;
- no stage depends on a single local SSD copy.

## 4. Stage 2 — Align the WebCodex Server baseline

The current browser-to-Windows development path may still be using an older WebCodex Server. Do not assume its version.

### Work

1. read-only confirm the deployed Server version, build revision, runtime status, and protocol compatibility;
2. back up the Server database/state, environment/configuration, service definition, enrollment/token state, tunnel/public endpoint configuration, and current runnable server build/install method;
3. prepare an explicit rollback procedure;
4. upgrade the Server to the selected WebCodex 0.4.1 baseline;
5. verify Server 0.4.1 with the Windows Runner/Engine baseline;
6. verify authentication, project routing, MCP/tool discovery, Runner reconnect, job lifecycle, and tunnel access.

### Gate

Do not begin product restoration that depends on new protocol/runtime behavior until the development Server and Runner baseline is known and validated.

## 5. Stage 3 — Restore CodexLink in bounded product layers

Restore functionality layer by layer rather than recreating the old RC in one large change.

### 3A. Product identity and desktop shell

Restore:

- CodexLink product name and branding;
- Tauri identifier and product-specific namespace;
- persistent app-data namespace (`dev.codexlink.desktop`);
- program-install vs persistent-user-data separation;
- single-instance behavior;
- product navigation and customer-facing terminology;
- About/diagnostic version separation: CodexLink product version vs WebCodex Engine version.

### 3B. Project management and authority

Restore:

- multi-project state;
- canonical exact-root registration;
- add/remove project operations;
- no silent widening to a parent path;
- add/remove operation completes only after the CodexLink-owned Runner is restarted/reloaded as required and the affected project is Ready;
- Server and Tunnel must not be restarted merely because a project was added/removed;
- removal may delete only the exact product-owned registry record after ID + canonical path verification; never recursively delete the user's project directory.

Before commercial release, decide whether filesystem roots and similarly broad paths are rejected outright until a stronger sandbox exists.

### 3C. Runtime lifecycle

Restore:

- Server/Runner/Tunnel lifecycle management;
- login-start/autostart semantics;
- dynamic tunnel health discovery (never hardcode a fixed health port);
- owned-process supervision and single-instance behavior;
- correct recovery UI/status handling.

### 3D. Tunnel packaging

Restore the prior product boundary:

- customer builds bundle the pinned official OpenAI tunnel client;
- use an explicit packaged binary override rather than falling back to the legacy WebCodex tool cache;
- fail closed when a required bundled customer tunnel binary is missing or has the wrong hash;
- retain the fixed tunnel-client provenance/hash in release records.

Previously accepted tunnel-client v0.0.12 SHA256:

`6649169733686805CA16CCCD91774594D0C017FD729C37AD4CE1CD18323D9AE8`

### 3E. License and migration

Restore:

- current MYWS activation-code compatibility (do not change the prefix casually);
- DPAPI-protected local secrets/identity where applicable;
- simplified customer-facing activation screen;
- bounded copy-only migration from known legacy product locations;
- migration of only known product state;
- rebase only CodexLink-owned internal absolute paths;
- never rewrite a user's project/allowed-root path;
- migration marker preventing deleted new-state artifacts from being resurrected later from legacy folders.

### 3F. UI polish

Only after the behavior is stable, restore/refine the commercial UI. Avoid customer-facing implementation details that do not help users operate the product.

### Gate

Each sub-layer must have its own tested commit/checkpoint. Do not wait until the entire desktop is rebuilt before backing it up.

## 6. Stage 4 — Restore supporting tools

### 4A. Launcher

Rebuild the previously stable Launcher behavior first. Do not redesign it while recovering functionality. Validate it separately and freeze a known-good checkpoint.

### 4B. Activation-code / License Issuer tool

Rebuild the issuer so generated sales activation codes remain compatible with the CodexLink license-verification contract. Private signing material must never be bundled into the customer application.

### Backup rule

Launcher and issuer must each have independent source backup/checkpoints; they must not once again exist only on the same local disk as CodexLink.

## 7. Stage 5 — Windows sandbox proof of concept

Treat sandboxing as an experiment first, not as an untested production promise.

Evaluate combinations such as:

- AppContainer;
- Restricted Token;
- Job Object;
- filesystem ACL/capability grants;
- other Windows-supported isolation primitives where appropriate.

### Test matrix

For every candidate sandbox, test at minimum:

- reading/writing the explicitly authorized project;
- denial outside authorized project scope;
- Git;
- Node.js / npm;
- Python;
- Rust / Cargo;
- compiler/tool subprocesses;
- child process creation;
- LSP servers;
- environment variables and tool discovery;
- outbound networking where needed;
- WebCodex Runner connectivity;
- OpenAI Tunnel connectivity;
- performance and operational overhead.

### Decision gate

Produce a compatibility/security matrix before integrating a sandbox into the normal CodexLink runtime. Then decide explicitly whether:

- 1.0 ships with an OS sandbox; or
- 1.0 ships with safe execution mode + conservative path policy, and sandboxing becomes a later release.

## 8. Stage 6 — Second security-hardening pass

Use the sandbox findings to finalize the commercial security model.

Review and test:

- root selection policy;
- dangerous-operation policy;
- canonicalization and traversal handling;
- symlink/junction escape handling;
- protected/sensitive path rules;
- typed delete/rename/move semantics;
- subprocess/tool policy;
- capability advertisement vs actual local policy;
- “Pause AI access” / master local-access control;
- minimum diagnostic/log information;
- license signing key separation;
- DPAPI secret handling;
- signed release binaries and provenance.

The design principle is: assume an AI/tool will eventually make a bad decision; the operating system and product boundaries should limit the blast radius.

## 9. Stage 7 — Product website and novice tutorial

Build the website after the product workflow and security decisions are sufficiently stable to avoid repeatedly invalidating screenshots.

Site scope:

- product/home page;
- purchase/download;
- installation;
- activation;
- adding a local project;
- creating/configuring the OpenAI Secure MCP Tunnel;
- creating the required Runtime Key/credentials;
- enabling ChatGPT Developer Mode;
- creating the ChatGPT App/MCP connection;
- first successful use;
- FAQ/troubleshooting;
- version history/download archive;
- third-party/open-source notices;
- support/contact.

### Tutorial rules

- use real screenshots, never generated screenshots;
- one action group per screenshot;
- redact Runtime Keys, activation codes, personal email/account/org data;
- verify current official OpenAI URLs, UI paths, plan/permission constraints at documentation time rather than guessing from old routes;
- product UI should link to stable tutorial sections instead of embedding fast-changing OpenAI screenshots inside the executable.

## 10. Stage 8 — Release-candidate and commercial release process

Do not overwrite or reuse accepted/rejected candidate names.

Every RC record must include:

- CodexLink product version;
- source commit;
- WebCodex Engine version/commit;
- tunnel-client version/SHA;
- installer SHA256;
- build/signing status;
- validation date;
- changes;
- known issues;
- recommended/rejected status.

### Required Windows validation

At minimum validate:

- clean install;
- custom install directory;
- upgrade;
- uninstall;
- preservation of intended persistent state;
- activation/recovery;
- project add/remove;
- automatic Runner recovery after project changes;
- Server/Runner/Tunnel lifecycle;
- Tunnel/ChatGPT connection;
- single instance;
- login autostart when enabled;
- migration/rollback behavior;
- safe-execution policy;
- security regression cases discovered during sandbox/hardening work.

### Distribution policy

CodexLink does not auto-update and does not perform an in-product network “check for updates”. Stable releases are manually validated and published. Historical versions remain archived with hashes and metadata so users can choose whether and when to upgrade or downgrade.

## 11. Current order of execution

The active order is:

1. close Stage 1 infrastructure/security baseline;
2. confirm and align the Server to the selected 0.4.1 baseline;
3. restore CodexLink layer by layer;
4. restore Launcher and License Issuer;
5. run the Windows sandbox POC;
6. complete the second security-hardening pass;
7. build the website/tutorial;
8. create new RCs and proceed toward the commercial release.

Do not start a later stage merely because it is visually more complete. The preceding gate must be either closed or explicitly documented as a conscious exception.