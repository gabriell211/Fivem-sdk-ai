use std::fs;
use crate::{workspace, AppResult};

const FXMANIFEST: &str = r#"fx_version 'cerulean'
game 'gta5'

author 'FiveM SDK AI'
description 'Local development telemetry bridge'
version '0.1.0'

client_script 'client.lua'
server_script 'server.lua'
"#;

const CLIENT: &str = r#"local function safeSnapshot()
    local ped = PlayerPedId()
    local coords = GetEntityCoords(ped)
    local vehicle = GetVehiclePedIsIn(ped, false)

    return {
        playerId = PlayerId(),
        serverId = GetPlayerServerId(PlayerId()),
        ped = ped,
        model = GetEntityModel(ped),
        health = GetEntityHealth(ped),
        armor = GetPedArmour(ped),
        coords = { x = coords.x, y = coords.y, z = coords.z },
        heading = GetEntityHeading(ped),
        interior = GetInteriorFromEntity(ped),
        vehicle = vehicle ~= 0 and vehicle or nil,
        zone = GetNameOfZone(coords.x, coords.y, coords.z),
        gameTimer = GetGameTimer()
    }
end

RegisterNetEvent('sdkai:request', function(requestId, action)
    if source ~= 65535 then return end
    if type(requestId) ~= 'string' or type(action) ~= 'string' then return end

    local payload
    if action == 'ping' then
        payload = { ok = true, action = action, gameTimer = GetGameTimer() }
    elseif action == 'snapshot' then
        payload = { ok = true, action = action, snapshot = safeSnapshot() }
    else
        payload = { ok = false, action = action, error = 'unsupported action' }
    end

    TriggerServerEvent('sdkai:result', requestId, payload)
end)
"#;

const SERVER: &str = r#"local pending = {}
local lastResultAt = {}
local sequence = 0

local function emit(event)
    print('[SDKAI_EVENT]' .. json.encode(event))
end

local function request(action)
    local players = GetPlayers()
    if #players == 0 then
        emit({ type = 'test_result', ok = false, action = action, error = 'no FiveM client connected' })
        return
    end

    sequence = sequence + 1
    local requestId = ('%s:%d:%d'):format(action, os.time(), sequence)
    local targetPlayer = tonumber(players[1])
    pending[requestId] = { action = action, createdAt = os.time(), player = targetPlayer }
    TriggerClientEvent('sdkai:request', targetPlayer, requestId, action)
    emit({ type = 'test_dispatched', ok = true, action = action, requestId = requestId, player = targetPlayer })
end

RegisterCommand('sdkai_ping', function(source)
    if source ~= 0 then return end
    request('ping')
end, true)

RegisterCommand('sdkai_snapshot', function(source)
    if source ~= 0 then return end
    request('snapshot')
end, true)

RegisterNetEvent('sdkai:result', function(requestId, payload)
    local playerSource = source
    if type(requestId) ~= 'string' or type(payload) ~= 'table' then return end

    local expected = pending[requestId]
    if not expected or expected.player ~= playerSource then return end

    local now = os.time()
    if lastResultAt[playerSource] and now - lastResultAt[playerSource] < 1 then return end
    lastResultAt[playerSource] = now
    pending[requestId] = nil

    local encoded = json.encode(payload)
    if #encoded > 32768 then
        emit({ type = 'test_result', ok = false, action = expected.action, error = 'client payload exceeded 32 KiB' })
        return
    end

    emit({ type = 'test_result', ok = payload.ok == true, action = expected.action, requestId = requestId, player = playerSource, payload = payload })
end)

AddEventHandler('playerDropped', function()
    lastResultAt[source] = nil
end)

CreateThread(function()
    while true do
        Wait(5000)
        local now = os.time()
        for requestId, item in pairs(pending) do
            if now - item.createdAt > 15 then
                pending[requestId] = nil
                emit({ type = 'test_result', ok = false, action = item.action, requestId = requestId, error = 'client response timeout' })
            end
        end
    end
end)

emit({ type = 'bridge_ready', ok = true })
"#;

pub fn install(workspace_path: &str) -> AppResult<()> {
    let root = workspace::root(workspace_path)?;
    let bridge = root.join("resources").join("[sdk-ai]").join("sdkai_bridge");
    fs::create_dir_all(&bridge)?;
    fs::write(bridge.join("fxmanifest.lua"), FXMANIFEST)?;
    fs::write(bridge.join("client.lua"), CLIENT)?;
    fs::write(bridge.join("server.lua"), SERVER)?;
    Ok(())
}
