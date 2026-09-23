# Documentation Index

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

Start with the goal that matches what you are trying to do.

## I want to use WebCodex normally

- [README](../README.md) — what WebCodex does and the difference between full use and a temporary trial
- [Desktop installation](desktop-install.md) — **recommended Windows/macOS entry**: Desktop + official OpenAI Secure Tunnel
- [Full Setup](PERSONAL_SETUP.md) — CLI, existing Server, Linux, and advanced regular Server + Runner setup
- [AI-assisted setup](AI_ONBOARDING.md) — have an AI agent configure WebCodex using ordinary user language
- [MCP](MCP.md) — ChatGPT, Claude, and other MCP clients

## I only want to try it for a few minutes

- [Quick Trial](QUICK_START.md) — temporarily try one repository with one `webcodex share` command

## I need production deployment or deep troubleshooting

- [Deployment](DEPLOYMENT.md) — systemd/Docker, multi-user self-hosting, and long-lived operations
- [Windows + OpenAI Secure MCP Tunnel deep dive](WINDOWS_OPENAI_TUNNEL.md) — independent Windows Server/Runner, Tunnel setup, and real troubleshooting notes
- [Runner](RUNNER.md) — Runner operations reference
- [CLI](CLI.md) — full command, configuration, and credential reference

## I need authentication or network options

- [MCP](MCP.md) — Bearer, query-token fallback, OAuth, private tunnel, and MCP protocol reference
- [Authentication](AUTH_MODEL.md) — detailed credential and authority boundaries
- [Deployment](DEPLOYMENT.md) — stable HTTPS origins, self-hosting, and production networking
- [GPT Actions](GPT_ACTIONS.md) — optional OpenAPI-based Custom GPT integration

## I need help

- [Troubleshooting](TROUBLESHOOTING.md) — installation, connection, runtime, and Runner problems
- [Security](../SECURITY.md) — safe operating guidance and security model

## I want to understand or extend WebCodex

- [Architecture](ARCHITECTURE.md) — how the major components fit together
- [Coding Workflow](CODING_WORKFLOW.md) — task bootstrap, guidance, validation, and closeout
- [Native Tool Plugins](PLUGINS.md) — add Runner-local tools with any executable language and no MCP SDK
- [Computer Use roadmap](COMPUTER_USE.md) — semantic-first desktop automation direction and validation priorities

## I want to contribute or release WebCodex

The `docs/agent/` pages below are maintainer/internal contracts. They intentionally
contain protocol fields, compatibility names, and implementation invariants that
ordinary users should not need to learn.

- [AGENTS.md](../AGENTS.md) — repository instructions for coding/AI agents
- [Maintenance](MAINTENANCE.md) — maintenance queue, dependency cadence, PR/CI expectations, and bilingual-doc policy
- [Testing](TESTING.md) — testing strategy
- [Release checklist](RELEASE_CHECKLIST.md) — release readiness
- [Architecture decisions](agent/architecture-decisions.md)
- [Runtime host context](agent/runtime-host-context.md) — Runner-configured planning context and runtime diagnostics
- [Job reliability and Runner concurrency](agent/job-reliability-and-concurrency.md) — restart recovery, observation semantics, shared Job capacity, and tool-description requirements
- [Tool request tracing](agent/tool-request-tracing.md) — maintainer forensic payload/correlation contract
- [Authority model](agent/permission-model.md)
- [Session model](agent/session-model.md)
- [Manual multi-window collaboration](agent/manual-window-collaboration.md)
- [OpenAPI guidelines](agent/openapi-guidelines.md)
- [Release process](agent/release-process.md)

### CodexLink product-fork planning

These are internal product documents for the CodexLink Windows fork rather than ordinary WebCodex user documentation.

- [CodexLink v1 acceptance and release status](CODEXLINK_V1_ACCEPTANCE.md) — current repository inventory, upstream comparison, customer journey, risks, and release gates
- [CodexLink first-release security boundary](CODEXLINK_SECURITY_BOUNDARY.md) — authoritative Destructive Guard, typed filesystem, bounded recursive-delete, and non-sandbox threat model
- [CodexLink first-release product shell & setup UX](CODEXLINK_FIRST_RELEASE_UI.md) — accepted RC5 visual baseline and the current activation → runtime bootstrap → Tunnel credentials → optional authorized roots → ChatGPT App flow
- [CodexLink commercial entitlement & activation flow](CODEXLINK_COMMERCIAL_FLOW.md) — authoritative MYWS purchase entitlement, download gate, same-code Desktop activation, and commercial acceptance flow
- [CodexLink License Operator](CODEXLINK_LICENSE_OPERATOR.md) — administrator-only local sales-code issuer, SSH/non-idempotency rules, and the completed `4b4043ca` checkpoint
