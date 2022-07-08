local api = neopult.api
local log = neopult.log

local DEFAULT_GEOMETRIES = { "480x360-0-0", "480x360-0+0", "480x360+0+0", "480x360+0-0" }

local STATUS_WAITING = "waiting"
local STATUS_ACTIVE = "active"
local STATUS_INACTIVE = "inactive"

local M = {}

M.camera_modules = {}
M.slot_active_states = {}
M.camera_handles = {}

M.sender_base_url = nil
M.generate_secure_tokens = true

local function generate_sender_message(sender_link)
    return "follow <a href=\"" .. sender_link .. "\" target=\"_blank\">this link</a> to the camera sender"
end

local function handle_notify(line)
    M.plugin_handle:info("camera server notify line " .. line)
    local space = string.find(line, ' ')
    if not space then
        M.plugin_handle:error("camera server notify line didn't specify a slot")
        return
    end

    local type = string.sub(line, 1, space - 1)
    if type == "new_feed" then
        local slot_str = string.sub(line, space + 1)
        local slot = tonumber(slot_str)
        M.slot_active_states[slot + 1] = true
        M.camera_handles[slot + 1] = M.plugin_handle:create_virtual_window("camera-" .. (slot + 1), {
            set_geometry = function(x_offset, y_offset, width, height, alignment, z)
                M.camera_server_handle:writeln("set_geometry_relative_to_canvas " .. slot ..  " " .. alignment .. " " .. x_offset .. " " .. y_offset .. " " .. width .. " " .. height .. " " .. z)
            end,
            map = function()
                M.camera_server_handle:writeln("show " .. slot)
            end,
            unmap = function()
                M.camera_server_handle:writeln("hide " .. slot)
            end,
            primary_demotion_action = "make_min",
            min_geometry = DEFAULT_GEOMETRIES[slot + 1]
        })
        M.plugin_handle:info("new feed on slot " .. slot)
        M.camera_modules[slot + 1]:set_status(STATUS_ACTIVE)
    elseif type == "remove_feed" then
        local slot_str = string.sub(line, space + 1)
        local slot = tonumber(slot_str)
        M.slot_active_states[slot + 1] = false
        M.camera_handles[slot + 1]:unclaim()
        M.camera_handles[slot + 1] = nil
        M.plugin_handle:info("removed feed on slot " .. slot)
        -- Only set status to waiting when status was active previously. This
        -- prevents overwriting an inactive status.
        if M.camera_modules[slot + 1]:get_status() == STATUS_ACTIVE then
            M.camera_modules[slot + 1]:set_status(STATUS_WAITING)
        end
    elseif type == "custom_name" then
        M.plugin_handle:warn("camera server custom_name messages are not handled yet")
    end
end

M.setup = function(args)
    args = args or {}

    local channel = api.get_channel()
    local channel_home = api.get_channel_home()

    local port = args.port or (5000 + channel)
    local cameras = args.cameras or 4
    local notify_path = args.notify_path or (channel_home .. "/camera-server-output-" .. channel)
    local janus_url = args.janus_url or "http://localhost:8088/janus"
    local janus_room = args.janus_room or (1000 + channel)
    local janus_room_secret = args.janus_room_secret or "default"
    local janus_room_pin = args.janus_room_pin or "default"
    local janus_bitrate = args.janus_bitrate or 128000
    local janus_admin_key = args.janus_admin_key or "secret"

    if args.generate_secure_tokens == false then
        M.generate_secure_tokens = false
    end

    if args.camera_server_path == nil then
        error("cvh_camera plugin setup called without mandatory `camera_server_path` parameter")
    end

    if args.sender_base_url == nil then
        error("cvh_camera plugin setup called without mandatory `sender_base_url` parameter")
    end
    M.sender_base_url = args.sender_base_url

    M.plugin_handle = api.register_plugin_instance("cvh-camera")
    if M.plugin_handle then
        M.camera_server_handle = M.plugin_handle:spawn_process("node", {
            args = { args.camera_server_path },
            envs = {
                CVH_CAMERA_CONFIG_port = tostring(port),
                CVH_CAMERA_CONFIG_cameraSlots = tostring(cameras),
                CVH_CAMERA_CONFIG_notifyPath = tostring(notify_path),
                CVH_CAMERA_CONFIG_janusURL = tostring(janus_url),
                CVH_CAMERA_CONFIG_janusRoom = tostring(janus_room),
                CVH_CAMERA_CONFIG_janusRoomSecret = tostring(janus_room_secret),
                CVH_CAMERA_CONFIG_janusRoomPin = tostring(janus_room_pin),
                CVH_CAMERA_CONFIG_janusBitrate = tostring(janus_bitrate),
                CVH_CAMERA_CONFIG_janusAdminKey = tostring(janus_admin_key),
            },
        })

        if not M.camera_server_handle then
            M.plugin_handle:error("couldn't spawn camera server -- aborting plugin setup")
            return
        end

        local notify_create_cmd = string.format([[sh -c 'test -p "%s" || mkfifo "%s"']], notify_path, notify_path)
        os.execute(notify_create_cmd)

        M.notify_handle = M.plugin_handle:spawn_process("tail", {
            args = { "-f", notify_path },
            on_output = handle_notify,
        })

        if not M.notify_handle then
            M.plugin_handle:error("couldn't spawn notify listener -- aborting plugin setup")
            M.camera_server_handle:kill()
            return
        end

        for camera = 1, cameras do
            -- local camera = camera_
            M.slot_active_states[camera] = false
            local module_handle = M.plugin_handle:register_module("camera-" .. camera)
            M.camera_modules[camera] = module_handle
            module_handle:set_status(STATUS_INACTIVE)
            module_handle:register_action("start", function()
                module_handle:info("start action")

                local token
                if M.generate_secure_tokens then
                    token = api.generate_token(20)
                else
                    token = "token"
                end

                M.camera_server_handle:writeln("activate_slot " .. (camera - 1) .. " " .. token)
                module_handle:set_status(STATUS_WAITING)

                local sender_link = string.format("%s?slot=%d&room=%d&token=%s", M.sender_base_url, camera - 1, janus_room, token)
                local sender_message = generate_sender_message(sender_link)
                module_handle:info(sender_message)
                module_handle:set_message(sender_message)
            end)
            module_handle:register_action("stop", function()
                module_handle:info("stop action")
                module_handle:set_status(STATUS_INACTIVE)
                module_handle:set_message(nil)
                M.camera_server_handle:writeln("deactivate_slot " .. (camera - 1))
            end)
            module_handle:register_action("hide", function()
                module_handle:info("hide action")
                if M.camera_handles[camera] then
                    M.camera_handles[camera]:hide()
                end
            end)
            module_handle:register_action("max", function()
                module_handle:info("max action")
                if M.camera_handles[camera] then
                    M.camera_handles[camera]:max({ 1200, 900 })
                end
            end)
            module_handle:register_action("min", function()
                module_handle:info("min action")
                if M.camera_handles[camera] then
                    M.camera_handles[camera]:min()
                end
            end)
        end
    end

end

return M
