# Architecture

## Core invariant

An AI-generated change is **not** considered game-tested merely because source code parses or FXServer starts. A game-level success claim requires evidence returned from a connected FiveM client.

The desktop core owns four explicit boundaries: workspace I/O, FXServer lifecycle, AI tools, and game-test telemetry.

## Desktop layers

### React + Monaco

The UI edits source text, selects a workspace, controls the managed runtime, displays server output and sends user intent to the Rust AI runner. It does not execute arbitrary child processes.

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
6. Bootstrap server-data resources into the workspace.
7. Spawn the server with piped stdin/stdout/stderr.
8. Keep a bounded structured log ring.
9. Accept only an explicit console-command allowlist.
10. Wait for structured in-game evidence after a test dispatch.

The runtime config is generated in `.sdkai/runtime.cfg`. It executes the user's `server.cfg`, reapplies local-only development settings and adds the Cfx.re license key. The key is not exposed in the editor file list or to the AI.

### sdkai_bridge

A normal FiveM resource containing `server.lua` and `client.lua`.

Current commands:

- `sdkai_ping`
- `sdkai_snapshot`

A request picks a currently connected player, allocates a request ID and records the exact target. The client performs the known read-only action. The result is accepted only when request ID, target player and action match. Requests expire; result cadence and encoded payload size are bounded.

Accepted evidence is emitted as one structured stdout record:

```text
[SDKAI_EVENT]{...json...}
```

Rust only resolves the test call after observing the matching event emitted after its dispatch sequence point.

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
workspace / FXServer / in-game operation
   |
bounded structured result
   |
provider continues
```

The loop and tool-output sizes are capped.

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

A passing `ping` proves:

1. managed FXServer is alive;
2. `sdkai_bridge` is running;
3. a real FiveM client is connected;
4. server-to-client event delivery worked;
5. client Lua executed inside FiveM;
6. client-to-server delivery worked;
7. the server accepted the response from the expected player;
8. the IDE observed the exact structured result after dispatch.

It does not prove an arbitrary gameplay feature is correct. Scenario-specific assertions will be layered above this transport.

## Screenshot architecture

Planned path:

```text
FiveM client / screenshot-basic
          |
   HTTP multipart upload
          v
127.0.0.1 random-port receiver owned by IDE
          |
 type/size/hash validation
          |
 vision/assertion tool
```

Raw image base64 should not be relayed through ordinary FXServer events.

## Threat model

Assume all of the following may be malformed:

- AI provider output;
- workspace content and resource paths;
- FiveM client event payloads;
- upstream archive paths;
- FXServer log text.

v0.1 controls include canonical path checks, content bounds, strict console allowlisting, HTTPS/host checks for runtime downloads, archive path safety, typed test actions, exact-player request binding, payload bounds and timeouts.
