# FiveM / Cfx.re research notes

These notes record the upstream rules that directly shaped the IDE. The point is to keep code-generation and testing behavior traceable to official documentation rather than assumptions.

## Server provisioning

Official server setup documents use FXServer artifacts and `cfx-server-data` as the baseline server-data repository. txAdmin is included with modern FXServer builds.

Implementation consequence: the IDE does not vendor FXServer. It resolves Cfx.re's current **LATEST RECOMMENDED** Windows artifact at runtime, downloads the exact official `server.7z`, caches by build number and locates both historical `FXServer.exe` and newer `cfx-server.exe` packaging names.

References:

- https://docs.fivem.net/docs/server-manual/setting-up-a-server/
- https://docs.fivem.net/docs/server-manual/setting-up-a-server-vanilla/
- https://docs.fivem.net/docs/server-manual/setting-up-a-server-txadmin/
- https://runtime.fivem.net/artifacts/fivem/build_server_windows/master/
- https://github.com/citizenfx/cfx-server-data

## Resource manifests and runtimes

Current resources use `fxmanifest.lua`. The current documented FX version is `cerulean`. Lua 5.4 is now the default runtime and the old `lua54 'yes'` manifest flag is deprecated.

Implementation consequence: generated bridge/resources use:

```lua
fx_version 'cerulean'
game 'gta5'
```

and do not emit `lua54 'yes'`.

Reference:

- https://docs.fivem.net/docs/scripting-reference/resource-manifest/resource-manifest/

## Resource lifecycle

FXServer exposes resource lifecycle commands including `refresh`, `ensure`, `start`, `stop` and `restart`.

Implementation consequence: the AI receives narrow wrappers for the lifecycle operations it needs instead of arbitrary console or OS-shell access.

Reference:

- https://docs.fivem.net/docs/server-manual/server-commands/

## Client/server event security

Cfx.re explicitly distinguishes local events from network events and warns against trusting client-provided values for authoritative server decisions.

Implementation consequence: the SDK bridge networks only its narrow request/result transport. The server tracks request ID, action and the exact target player; a different client cannot satisfy the request. Payload size, cadence and expiry are bounded. Future gameplay tests must verify authoritative properties server-side whenever possible.

References:

- https://docs.fivem.net/docs/developers/server-security/
- https://docs.fivem.net/docs/scripting-manual/working-with-events/listening-for-events/

## OneSync and state bags

OneSync provides server-side entity/state awareness. State bags have documented ownership and replication semantics.

Implementation consequence: scenario assertions should prefer authoritative server/OneSync state for server-owned facts, while client snapshots are evidence for render/runtime observations rather than authority.

References:

- https://docs.fivem.net/docs/scripting-reference/onesync/
- https://docs.fivem.net/docs/scripting-manual/networking/state-bags/

## NUI / CEF

FiveM NUI runs through CEF. NUI callbacks must return a response, and secure-context resource URLs use `https://cfx-nui-...`.

Implementation consequence: the AI's coding rules include callback completion. A later IDE milestone will capture CEF console/network diagnostics for NUI tests instead of relying only on FXServer logs.

References:

- https://docs.fivem.net/docs/scripting-manual/nui-development/
- https://docs.fivem.net/docs/scripting-manual/nui-development/nui-callbacks/

## FxDK precedent

The official FxDK already demonstrates the minimum observability bar expected from a FiveM development environment: project resources, live restart, Game View, game/server consoles and resource monitoring.

Implementation consequence: FiveM SDK AI is not designed as merely a text editor next to FXServer. Its differentiator is an auditable AI edit -> restart -> real-client evidence loop layered on top of equivalent development primitives.

Reference:

- https://docs.fivem.net/docs/server-manual/fxdk/

## screenshot-basic

The official `screenshot-basic` resource exposes client screenshot APIs and a server-triggered client screenshot path. Its guidance makes relaying large screenshot base64 blobs through ordinary server events unattractive.

Implementation consequence: the planned visual-testing path sends screenshot data directly from the client to an IDE-owned localhost HTTP receiver, validates size/type/hash there, and returns only bounded metadata/evidence to the AI layer.

Reference:

- https://github.com/citizenfx/screenshot-basic

## Compatibility

The upstream `cfx-server-data` repository is archived/finalized, while official vanilla-server docs still reference it. The IDE treats it as bootstrap data only and keeps project-specific resources inside the workspace.

The runtime locator accepts both `FXServer.exe` and `cfx-server.exe` to tolerate packaging evolution.
