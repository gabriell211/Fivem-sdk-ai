use std::fs;

use crate::{workspace, AppResult};

const FXMANIFEST: &str = r#"fx_version 'cerulean'
game 'gta5'

author 'FiveM SDK AI'
description 'Local development telemetry and test bridge'
version '0.2.0'

client_script 'client.lua'
server_script 'server.lua'
"#;

const CLIENT: &str = r#"local createdVehicles = {}

local function finiteNumber(value, limit)
    if type(value) ~= 'number' or value ~= value then return false end
    return math.abs(value) <= (limit or 100000.0)
end

local function safeSnapshot()
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
        gameTimer = GetGameTimer(),
        pauseMenu = IsPauseMenuActive()
    }
end

local function teleport(args)
    local x = args and args.x
    local y = args and args.y
    local z = args and args.z
    local heading = args and args.heading

    if not finiteNumber(x) or not finiteNumber(y) or not finiteNumber(z) then
        return { ok = false, action = 'teleport', error = 'invalid coordinates' }
    end
    if heading ~= nil and not finiteNumber(heading, 36000.0) then
        return { ok = false, action = 'teleport', error = 'invalid heading' }
    end

    local ped = PlayerPedId()
    RequestCollisionAtCoord(x, y, z)
    SetEntityCoordsNoOffset(ped, x, y, z, false, false, false)
    if heading ~= nil then
        SetEntityHeading(ped, heading + 0.0)
    end
    Wait(250)

    return { ok = true, action = 'teleport', snapshot = safeSnapshot() }
end

local function spawnVehicle(args)
    local modelName = args and args.model
    if type(modelName) ~= 'string' or #modelName < 1 or #modelName > 64 or not modelName:match('^[%w_%-]+$') then
        return { ok = false, action = 'spawn_vehicle', error = 'invalid vehicle model' }
    end

    local model = GetHashKey(modelName)
    if not IsModelInCdimage(model) or not IsModelAVehicle(model) then
        return { ok = false, action = 'spawn_vehicle', error = 'model is not a valid vehicle' }
    end

    RequestModel(model)
    local deadline = GetGameTimer() + 5000
    while not HasModelLoaded(model) and GetGameTimer() < deadline do
        Wait(0)
    end
    if not HasModelLoaded(model) then
        return { ok = false, action = 'spawn_vehicle', error = 'vehicle model load timeout' }
    end

    local ped = PlayerPedId()
    local coords = GetEntityCoords(ped)
    local heading = GetEntityHeading(ped)
    local vehicle = CreateVehicle(model, coords.x + 2.5, coords.y, coords.z, heading, true, false)
    SetModelAsNoLongerNeeded(model)

    if vehicle == 0 or not DoesEntityExist(vehicle) then
        return { ok = false, action = 'spawn_vehicle', error = 'CreateVehicle failed' }
    end

    SetEntityAsMissionEntity(vehicle, true, true)
    createdVehicles[#createdVehicles + 1] = vehicle
    if not args or args.warp ~= false then
        TaskWarpPedIntoVehicle(ped, vehicle, -1)
    end
    Wait(150)

    return {
        ok = true,
        action = 'spawn_vehicle',
        entity = vehicle,
        networkId = NetworkGetNetworkIdFromEntity(vehicle),
        model = GetEntityModel(vehicle),
        snapshot = safeSnapshot()
    }
end

local function cleanup()
    local removed = 0
    for index = #createdVehicles, 1, -1 do
        local entity = createdVehicles[index]
        if entity and DoesEntityExist(entity) then
            SetEntityAsMissionEntity(entity, true, true)
            DeleteEntity(entity)
            removed = removed + 1
        end
        createdVehicles[index] = nil
    end
    return { ok = true, action = 'cleanup', removed = removed, snapshot = safeSnapshot() }
end

RegisterNetEvent('sdkai:request', function(requestId, action, args)
    if source ~= 65535 then return end
    if type(requestId) ~= 'string' or type(action) ~= 'string' then return end
    if args ~= nil and type(args) ~= 'table' then return end

    local payload
    if action == 'ping' then
        payload = { ok = true, action = action, gameTimer = GetGameTimer() }
    elseif action == 'snapshot' then
        payload = { ok = true, action = action, snapshot = safeSnapshot() }
    elseif action == 'teleport' then
        payload = teleport(args or {})
    elseif action == 'spawn_vehicle' then
        payload = spawnVehicle(args or {})
    elseif action == 'cleanup' then
        payload = cleanup()
    else
        payload = { ok = false, action = action, error = 'unsupported client action' }
    end

    TriggerServerEvent('sdkai:result', requestId, payload)
end)

AddEventHandler('onResourceStop', function(resourceName)
    if resourceName == GetCurrentResourceName() then
        cleanup()
    end
end)
"#;

const SERVER: &str = r#"local pending = {}
local sequence = 0
local token = GetConvar('sdkai_token', '')

local function emit(event)
    print('[SDKAI_EVENT]' .. json.encode(event))
end

local function sendJson(response, status, payload)
    if not response then return end
    response.writeHead(status, {
        ['Content-Type'] = 'application/json; charset=utf-8',
        ['Cache-Control'] = 'no-store'
    })
    response.send(json.encode(payload))
end

local function firstPlayer()
    local players = GetPlayers()
    if #players == 0 then return nil end
    return tonumber(players[1])
end

local function nextRequestId(action)
    sequence = sequence + 1
    return ('%s:%d:%d'):format(action, os.time(), sequence)
end

local function dispatchClient(action, args, response)
    local targetPlayer = firstPlayer()
    if not targetPlayer then
        local result = { type = 'test_result', ok = false, action = action, error = 'no FiveM client connected' }
        emit(result)
        sendJson(response, 409, result)
        return
    end

    local requestId = nextRequestId(action)
    pending[requestId] = {
        action = action,
        createdAt = os.time(),
        player = targetPlayer,
        response = response
    }

    TriggerClientEvent('sdkai:request', targetPlayer, requestId, action, args or {})
    emit({
        type = 'test_dispatched',
        ok = true,
        action = action,
        requestId = requestId,
        player = targetPlayer
    })
end

local function captureScreenshot(response)
    local targetPlayer = firstPlayer()
    if not targetPlayer then
        sendJson(response, 409, { ok = false, action = 'screenshot', error = 'no FiveM client connected' })
        return
    end
    if GetResourceState('screenshot-basic') ~= 'started' then
        sendJson(response, 503, { ok = false, action = 'screenshot', error = 'screenshot-basic is not started' })
        return
    end

    local requestId = nextRequestId('screenshot')
    local fileName = ('.sdkai/screenshots/%s.jpg'):format(requestId:gsub('[^%w%-_]', '_'))

    exports['screenshot-basic']:requestClientScreenshot(targetPlayer, {
        fileName = fileName,
        encoding = 'jpg',
        quality = 0.82
    }, function(err, data)
        if err then
            local result = { ok = false, action = 'screenshot', error = tostring(err) }
            emit({ type = 'test_result', ok = false, action = 'screenshot', error = tostring(err) })
            sendJson(response, 500, result)
            return
        end

        local result = {
            ok = true,
            action = 'screenshot',
            player = targetPlayer,
            path = tostring(data or fileName)
        }
        emit({ type = 'test_result', ok = true, action = 'screenshot', player = targetPlayer, payload = result })
        sendJson(response, 200, result)
    end)
end

local function handleTest(payload, response)
    if type(payload) ~= 'table' or type(payload.action) ~= 'string' then
        sendJson(response, 400, { ok = false, error = 'invalid test request' })
        return
    end

    local action = payload.action
    local args = payload.args
    if args ~= nil and type(args) ~= 'table' then
        sendJson(response, 400, { ok = false, action = action, error = 'args must be an object' })
        return
    end

    if action == 'screenshot' then
        captureScreenshot(response)
        return
    end

    if action ~= 'ping'
        and action ~= 'snapshot'
        and action ~= 'teleport'
        and action ~= 'spawn_vehicle'
        and action ~= 'cleanup'
    then
        sendJson(response, 400, { ok = false, action = action, error = 'unsupported action' })
        return
    end

    dispatchClient(action, args or {}, response)
end

RegisterCommand('sdkai_ping', function(source)
    if source ~= 0 then return end
    dispatchClient('ping', {}, nil)
end, true)

RegisterCommand('sdkai_snapshot', function(source)
    if source ~= 0 then return end
    dispatchClient('snapshot', {}, nil)
end, true)

RegisterNetEvent('sdkai:result', function(requestId, payload)
    local playerSource = source
    if type(requestId) ~= 'string' or type(payload) ~= 'table' then return end

    local expected = pending[requestId]
    if not expected or expected.player ~= playerSource then return end

    pending[requestId] = nil

    local encoded = json.encode(payload)
    if #encoded > 32768 then
        local result = {
            type = 'test_result',
            ok = false,
            action = expected.action,
            requestId = requestId,
            error = 'client payload exceeded 32 KiB'
        }
        emit(result)
        sendJson(expected.response, 413, result)
        return
    end

    local result = {
        type = 'test_result',
        ok = payload.ok == true,
        action = expected.action,
        requestId = requestId,
        player = playerSource,
        payload = payload
    }
    emit(result)
    sendJson(expected.response, result.ok and 200 or 422, result)
end)

SetHttpHandler(function(request, response)
    if request.address ~= '127.0.0.1'
        and request.address ~= '::1'
        and request.address ~= '::ffff:127.0.0.1'
    then
        sendJson(response, 403, { ok = false, error = 'loopback only' })
        return
    end

    local suppliedToken = request.headers['x-sdkai-token']
    if token == '' or suppliedToken ~= token then
        sendJson(response, 401, { ok = false, error = 'unauthorized' })
        return
    end

    if request.method == 'GET' and request.path == '/health' then
        sendJson(response, 200, {
            ok = true,
            resource = GetCurrentResourceName(),
            players = #GetPlayers()
        })
        return
    end

    if request.method ~= 'POST' or request.path ~= '/test' then
        sendJson(response, 404, { ok = false, error = 'not found' })
        return
    end

    request.setDataHandler(function(data)
        if type(data) ~= 'string' or #data > 16384 then
            sendJson(response, 413, { ok = false, error = 'request body too large' })
            return
        end

        local ok, payload = pcall(json.decode, data)
        if not ok then
            sendJson(response, 400, { ok = false, error = 'invalid JSON' })
            return
        end
        handleTest(payload, response)
    end)
end)

AddEventHandler('playerDropped', function()
    local playerSource = source
    for requestId, item in pairs(pending) do
        if item.player == playerSource then
            pending[requestId] = nil
            local result = {
                type = 'test_result',
                ok = false,
                action = item.action,
                requestId = requestId,
                error = 'target player disconnected'
            }
            emit(result)
            sendJson(item.response, 410, result)
        end
    end
end)

CreateThread(function()
    while true do
        Wait(1000)
        local now = os.time()
        for requestId, item in pairs(pending) do
            if now - item.createdAt > 15 then
                pending[requestId] = nil
                local result = {
                    type = 'test_result',
                    ok = false,
                    action = item.action,
                    requestId = requestId,
                    error = 'client response timeout'
                }
                emit(result)
                sendJson(item.response, 504, result)
            end
        end
    end
end)

emit({ type = 'bridge_ready', ok = true, http = true })
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
