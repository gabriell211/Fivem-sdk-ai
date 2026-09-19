# FiveM SDK AI

AI-first desktop IDE for FiveM resource development. The IDE owns a managed local FXServer, exposes a Monaco-based editor, streams server logs, and gives the AI a narrow set of typed tools so it can edit resources, restart them and request evidence from a real connected FiveM client.

> v0.1 is the architecture foundation. It intentionally refuses to call a change "tested in game" unless the local bridge returns evidence from a connected FiveM client.

## Implemented

- Tauri 2 desktop shell with a Rust core.
- React 19 + Monaco editor.
- Workspace explorer with path traversal and symlink-escape protection.
- Managed FXServer installer using Cfx.re's **LATEST RECOMMENDED** Windows artifact.
- Runtime downloads the exact official `server.7z`; FXServer binaries are never committed to this repository.
- Bootstrap of the upstream `cfx-server-data` resources.
- Local development `server.cfg` plus an isolated runtime config for the Cfx.re license key.
- Start/stop/connect controls and streamed FXServer stdout/stderr.
- Strict FXServer command allowlist: resource lifecycle, status and SDK test commands only.
- Generated `resources/[sdk-ai]/sdkai_bridge` FiveM resource.
- Real client test handshake for `ping` and player-state `snapshot`.
- OpenAI-compatible tool-calling provider adapter. Default UI points to a localhost endpoint.
- AI has no generic PowerShell/cmd/shell tool.
- Windows GitHub Actions job for web build, Rust fmt, tests and clippy.

## Architecture

```text
React 19 + Monaco
       |
    Tauri IPC
       |
Rust desktop core
  |-- workspace sandbox
  |-- typed AI tool runtime
  |-- FXServer process manager
  |-- official artifact installer/cache
  `-- structured log/test collector
                |
         FXServer stdin/stdout
                |
 resources/[sdk-ai]/sdkai_bridge
       |                    |
   server.lua  <------>  client.lua
                            |
                    real FiveM client
```

The AI tool surface currently contains:

- `list_files`
- `read_file`
- `write_file`
- `refresh_resources`
- `restart_resource`
- `run_ingame_test`
- `server_status`

There is deliberately no arbitrary process execution tool.

## Real in-game tests

The IDE sends a restricted SDK command to FXServer. The bridge picks an actually connected player and sends a narrow network event with a generated request ID. Client-side code executes inside FiveM and returns a bounded result. The server accepts the result only from the exact player that received the request, then emits a structured `[SDKAI_EVENT]` record to stdout. Rust waits for that matching record before the AI can claim that the in-game check passed.

Current test actions:

- `ping`: verifies the complete IDE -> FXServer -> server resource -> real client -> server -> IDE path.
- `snapshot`: returns player/server IDs, ped/model, health, armour, coordinates, heading, interior, vehicle, zone and game timer.

This is transport/runtime evidence, not a claim that every gameplay behavior is correct. Scenario-specific assertions are the next layer.

## Run locally

Requirements for full in-game testing:

- Windows 10/11
- GTA V + FiveM installed
- Cfx.re server license key
- Node.js 22+
- pnpm
- Rust stable + Tauri Windows prerequisites

```bash
pnpm install
pnpm tauri dev
```

Workflow:

1. Open an empty folder or existing FiveM server workspace.
2. Click **Preparar** to bootstrap base resources and install/update `sdkai_bridge`.
3. Click **Instalar FXServer** to resolve and cache Cfx.re's current recommended artifact.
4. Enter the Cfx.re license key.
5. Start FXServer.
6. Click **Entrar no jogo** to open `fivem://connect/127.0.0.1:30120`.
7. Once a FiveM client is connected, run **Ping in-game**, **Snapshot player**, or let the AI invoke the same typed test tools.

The key is written only to `.sdkai/runtime.cfg` in the local workspace at startup. `.sdkai/` is ignored by Git and excluded from the AI file listing.

## Security model

The AI provider and connected game client are treated as untrusted inputs.

- Canonical workspace-root checks.
- Absolute paths and `..` are rejected.
- Canonical parent checks block symlink escapes.
- Read/write size limits and file-count limits.
- Remote AI endpoints require HTTPS; plain HTTP is accepted only on localhost.
- FXServer console operations are allowlisted.
- `exec`, `quit`, arbitrary convar mutation and shell separators are not exposed to the AI.
- In-game requests have generated IDs, timeout, payload bounds and expected-player binding.
- The bridge only accepts known actions.
- The system prompt forbids fabricated test success.

## FiveM decisions taken from upstream docs

- New resources use `fxmanifest.lua`, `fx_version 'cerulean'` and `game 'gta5'`.
- Lua 5.4 is current/default; generated resources do not add the deprecated `lua54 'yes'` flag.
- Client-supplied state is never treated as server authority when the server can validate it.
- Network events are used only when the client/server boundary is actually crossed.
- NUI callbacks must return a response.
- The current secure NUI context uses `https://cfx-nui-...`.
- OneSync/server state should be preferred for authoritative assertions where applicable.
- FXServer lifecycle uses documented commands such as `refresh`, `ensure`, `start`, `stop` and `restart`.

Research notes: [docs/FIVEM-RESEARCH.md](docs/FIVEM-RESEARCH.md)  
Architecture and threat model: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)

## Next milestones

- Direct screenshot capture from `screenshot-basic` to a localhost IDE HTTP receiver.
- Visual assertions and before/after screenshot comparisons.
- NUI/CEF console and network diagnostics.
- Resource dependency graph from `fxmanifest.lua`.
- Lua/JS/C# native/event intelligence.
- `resmon`/profiler evidence.
- Test DSL for spawn, teleport, entity, NUI and cleanup scenarios.
- Multi-client/OneSync scenarios.
- FXServer cached-build selector and rollback UI.

FiveM, Cfx.re and GTA V belong to their respective owners. This repository is an independent developer tool and downloads runtime components from upstream sources at user runtime.
