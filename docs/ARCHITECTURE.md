# Architecture

## Core invariant

An AI-generated change is **not** considered game-tested merely because source code parses or FXServer starts. A game-level success claim requires evidence produced through the managed bridge and, for client behavior, a connected FiveM client.

The desktop core owns explicit boundaries for workspace I/O, FXServer lifecycle, AI tools, game-test telemetry, NUI diagnostics and screenshot evidence.

## Desktop layers

### React + Monaco

The UI edits source text, selects a workspace, controls the managed runtime, displays server output and sends user intent to the Rust AI runner. Monaco is extended with a Lua language definition, common FiveM/Cfx completions and snippets for events, threads, NUI callbacks and manifests.

The UI also exposes deterministic test controls for ping/snapshot, teleport, vehicle spawn/cleanup, screenshots and NUI target discovery.

### Workspace sandbox

`workspace.rs` canonicalizes the root and constrains editor/AI I/O to it.

Controls:

- reject absolute paths and parent traversal;
- do not follow directory links during listing;
- canonical parent/target checks prevent symlink escape;
- bounded reads/writes;
- bounded file listing;
- exclude `.git`, `node_modules`, `target` and `.sdkai`.

### FXServer manager

`fxserver.rs` owns the server process.

Responsibilities:

1. Resolve Cfx.re's current recommended Windows build from the official artifact index.
2. Download the exact official `server.7z` over HTTPS.
3. Extract with a pure-Rust 7z decoder.
4. Locate `FXServer.exe` or `cfx-server.exe`.
5. Cache builds under application-local data and persist the active build pointer.
6. Bootstrap Cfx server-data resources and official `screenshot-basic`.
7. Spawn the server with piped stdin/stdout/stderr.
8. Keep a bounded structured log ring.
9. Accept only an explicit console-command allowlist.
10. Generate a fresh 256-bit bridge token for every server start.
11. Send structured tests through the authenticated loopback bridge.
12. Canonicalize, size-limit and hash screenshot evidence before exposing it to the UI.

The runtime config is generated in `.sdkai/runtime.cfg`. It executes the user's `server.cfg`, reapplies local development settings, adds the Cfx.re license key and injects the per-run bridge token. `.sdkai` is excluded from editor/AI listing and Git.

### sdkai_bridge

A normal FiveM resource containing `server.lua` and `client.lua`.

The primary automation transport is the resource HTTP handler at:

```text
http://127.0.0.1:30120/sdkai_bridge/test
```

FXServer itself is bound to loopback. The handler independently checks the request address and requires the per-run `x-sdkai-token`.

For client-side actions a request selects one connected player, allocates a request ID and records that exact player. Only a result from that player can complete the request. Completed IDs are removed, so replayed responses cannot satisfy another call. Requests expire after 15 seconds and payloads are bounded.

Implemented test actions:

- `ping`
- `snapshot`
- `teleport`
- `spawn_vehicle`
- `cleanup`
- `scenario`
- `screenshot`

`scenario` accepts 1-24 steps and only the safe operations `snapshot`, `teleport`, `spawn_vehicle`, `wait` and `cleanup`. Wait duration, coordinates, model names and payload sizes are bounded.

Console commands `sdkai_ping` and `sdkai_snapshot` remain as manual/fallback diagnostics.

Accepted activity is also emitted as structured stdout records:

```text
[SDKAI_EVENT]{...json...}
```

### Screenshot evidence

The official `screenshot-basic` server export requests a render capture from the selected real client and saves it directly beneath the workspace:

```text
.sdkai/screenshots/<request-id>.jpg
```

Raw screenshot base64 is never relayed through ordinary FiveM network events. When the bridge returns a screenshot path, Rust:

1. canonicalizes `.sdkai/screenshots`;
2. canonicalizes the returned file;
3. rejects any target outside that directory;
4. rejects empty or greater-than-10-MiB files;
5. computes SHA-256;
6. exposes the image data URL only to the local desktop UI.

The AI tool result receives the evidence path/hash/size/mime rather than a giant base64 payload.

### NUI / CEF diagnostics

`nui.rs` queries FiveM's documented CEF DevTools target endpoint on `127.0.0.1:13172/json/list`. Targets are surfaced in the IDE and to the AI as structured metadata. The IDE can also open the full DevTools UI in the user's browser.

This proves which CEF/NUI pages actually loaded. Deeper CDP console/network collection can be layered on the same endpoint without changing the trust boundary.

### AI tool loop

The provider interface is OpenAI-compatible tool calling. There is deliberately no generic shell tool.

```text
user request
   |
provider
   |
typed tool call
   |
Rust validation
   |
workspace / FXServer / bridge / NUI operation
   |
bounded structured evidence
   |
provider continues
```

The loop and tool-output sizes are capped. The system prompt forbids invented test success and tells the AI to prefer deterministic scenarios and cleanup.

## Command boundary

Allowed FXServer console operations are intentionally narrow:

- `refresh`
- `status`
- `ensure <resource>`
- `restart <resource>`
- `start <resource>`
- `stop <resource>`
- `sdkai_ping`
- `sdkai_snapshot`

Newlines, shell-like separators and non-allowlisted commands are rejected. Operations such as `exec`, `quit` and arbitrary convar mutation are not part of the AI surface.

## Test guarantees

A passing bridge `ping` proves:

1. managed FXServer is alive;
2. `sdkai_bridge` is running;
3. loopback authentication succeeded;
4. a real FiveM client is connected;
5. server-to-client event delivery worked;
6. client Lua executed inside FiveM;
7. client-to-server delivery worked;
8. the server accepted the response from the expected player;
9. the IDE received the structured result.

A passing scenario additionally proves each requested bridge step completed, but it does not magically prove unrelated gameplay logic. Tests should exercise the changed behavior and use snapshots/screenshots/NUI evidence appropriate to that feature.

## Threat model

Assume all of the following may be malformed:

- AI provider output;
- workspace content and resource paths;
- FiveM client event payloads;
- bridge HTTP bodies;
- upstream archive paths;
- screenshot paths/files;
- NUI DevTools responses;
- FXServer log text.

Controls include canonical path checks, content bounds, strict console allowlisting, HTTPS for upstream downloads, archive path safety, typed bridge actions, loopback binding, 256-bit per-run bridge authentication, exact-player request binding, request expiry, bounded scenarios, tracked test-entity cleanup and screenshot evidence validation.
