# FiveM SDK AI

AI-first desktop IDE for FiveM resource development. The IDE owns a managed local FXServer, exposes a Monaco-based editor, streams server logs, and gives the AI a narrow set of typed tools so it can edit resources, restart them and request evidence from a real connected FiveM client.

> The IDE intentionally refuses to call a change "tested in game" unless the local bridge returns evidence from a connected FiveM client. CI validates the desktop code, while GTA/FiveM runtime evidence is produced only on a real local client.

## Implemented

- Tauri 2 desktop shell with a Rust core.
- React 19 + Monaco editor with a registered Lua grammar, FiveM completions and safe snippets.
- Workspace explorer with path traversal and symlink-escape protection.
- Managed FXServer installer using Cfx.re's **LATEST RECOMMENDED** Windows artifact.
- Runtime downloads the exact official `server.7z`; FXServer binaries are never committed to this repository.
- Bootstrap of upstream `cfx-server-data` plus the official `screenshot-basic` resource.
- Local development `server.cfg` plus an isolated runtime config for the Cfx.re license key.
- Start/stop/connect controls and streamed FXServer stdout/stderr.
- Strict FXServer command allowlist: resource lifecycle, status and SDK test commands only.
- Generated `resources/[sdk-ai]/sdkai_bridge` FiveM resource with an authenticated loopback-only HTTP test API.
- Real client tests for `ping`, player `snapshot`, teleport validation, vehicle spawn, cleanup and bounded multi-step scenarios.
- Screenshot evidence through `screenshot-basic`, validated to remain inside `.sdkai/screenshots`, capped at 10 MiB and hashed with SHA-256.
- FiveM NUI/CEF target discovery through the documented remote DevTools endpoint on `127.0.0.1:13172`.
- In-app NUI DevTools launcher and screenshot preview.
- OpenAI-compatible tool-calling provider adapter. Default UI points to a localhost endpoint.
- AI has no generic PowerShell/cmd/shell tool.
- Windows GitHub Actions job for web build, Rust formatting, tests and clippy.
- Release workflow that builds Windows MSI/NSIS installers on tags or manual dispatch.

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
  |-- structured log/test collector
  |-- NUI DevTools discovery
  `-- screenshot evidence validator
                |
     loopback HTTP + FXServer logs
                |
 resources/[sdk-ai]/sdkai_bridge
       |                    |
   server.lua  <------>  client.lua
       |                    |
 screenshot-basic       real FiveM client
```

The AI tool surface currently contains:

- `list_files`
- `read_file`
- `write_file`
- `refresh_resources`
- `restart_resource`
- `run_ingame_test`
- `server_status`
- `nui_targets`

`run_ingame_test` supports structured arguments and the actions `ping`, `snapshot`, `teleport`, `spawn_vehicle`, `cleanup`, `scenario` and `screenshot`. There is deliberately no arbitrary process execution tool.

## Real in-game tests

At FXServer startup the Rust core generates a 256-bit token and passes it to the private development bridge through the ignored runtime config. FXServer is bound to `127.0.0.1:30120`; the bridge HTTP handler additionally checks loopback origin and the token before accepting a typed test request.

For client-side actions the bridge selects a connected player, creates a request ID and sends only the known action plus bounded JSON arguments. A result is accepted only from the exact player assigned to that request. Replays fail because a request is removed after completion, and stale requests time out.

Current actions:

- `ping`: proves IDE -> HTTP bridge -> server resource -> real client -> server -> IDE connectivity.
- `snapshot`: returns ped/model, health, armour, coordinates, heading, interior, vehicle, zone and game timer.
- `teleport`: validates bounded coordinates, teleports the local test player and returns the resulting snapshot.
- `spawn_vehicle`: validates a model name, loads it with a timeout, spawns a tracked test vehicle and returns entity/network evidence.
- `cleanup`: removes vehicles created by the bridge.
- `scenario`: executes 1-24 safe steps (`snapshot`, `teleport`, `spawn_vehicle`, `wait`, `cleanup`) and reports the exact failed step.
- `screenshot`: asks official `screenshot-basic` for a client render capture, then Rust canonicalizes the returned path, checks size, hashes it and exposes a preview in the IDE.

For NUI work the IDE also queries FiveM's remote CEF DevTools target list and can open the browser DevTools UI. This is evidence about what actually loaded in CEF; it is separate from server logs.

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
7. Once a FiveM client is connected, use **Ping**, **Snapshot**, **Screenshot**, teleport/spawn/cleanup controls, NUI discovery, or let the AI invoke the typed test/scenario tools.

The Cfx.re key and per-run bridge token are written only to `.sdkai/runtime.cfg` in the local workspace. Screenshots also live under `.sdkai/`. That directory is ignored by Git and excluded from the AI workspace listing.

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
- The HTTP bridge is loopback-only, authenticated by a per-run 256-bit token and accepts only known actions.
- Scenario execution is limited to 24 safe steps; waits and numeric coordinates are bounded.
- Test-created vehicles are tracked for cleanup.
- Screenshot paths are canonicalized beneath the managed evidence directory and size-limited before use.
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

## Remaining advanced work

The core edit -> run -> observe -> correct loop is implemented. The main areas that can still be expanded are deeper Chrome DevTools Protocol collection (console/network events instead of target discovery only), resource dependency graph visualization, profiler/resmon ingestion, multi-client OneSync scenarios and cached FXServer rollback selection.

FiveM, Cfx.re and GTA V belong to their respective owners. This repository is an independent developer tool and downloads runtime components from upstream sources at user runtime.
