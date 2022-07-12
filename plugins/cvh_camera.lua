local api = neopult.api
local log = neopult.log


local STATUS_WAITING = "waiting"
local STATUS_ACTIVE = "active"
local STATUS_INACTIVE = "inactive"

-- TODO: Define those in camera_mode plugin and require them here
local CAMERAS_INSIDE = "cameras-inside"
local CAMERAS_OUTSIDE = "cameras-outside"

local DEFAULT_GEOMETRIES = {
    [CAMERAS_INSIDE] = { "480x360-0-0", "480x360-0+0", "480x360+0+0", "480x360+0-0" },
    [CAMERAS_OUTSIDE] = { "480x360-0-0", "480x360-480-0", "480x360-960-0", "480x360-1440-0" },
}

local function setup(args)
    local P = {
        camera_modules = {},
        slot_active_states = {},
        camera_handles = {},
        sender_base_url = nil,
        mode = CAMERAS_INSIDE,
        camera_visible_states = {},
        camera_mode_store = nil,
        dynamic_camera_mode = true,
    }

    P.any_cameras_visible = function()
        for _, visible in ipairs(P.camera_visible_states) do
            if visible then
                return true
            end
        end

        return false
    end

    P.update_camera_visible_state = function(camera, visible)
        P.camera_visible_states[camera] = visible

        if P.camera_mode_store and P.dynamic_camera_mode then
            local state = P.camera_mode_store:get()
            local any_cameras_visible_before = state.any_cameras_visible
            local any_cameras_visible_after = P.any_cameras_visible()
            api.run_later(function()
                if not any_cameras_visible_before and any_cameras_visible_after then
                    state.any_cameras_visible = true
                end
                if any_cameras_visible_before and not any_cameras_visible_after then
                    state.any_cameras_visible = false
                end
                -- Always call, so that vnc plugin can adjust margin, when
                -- visibility doesn't change. For example, when camera goes from
                -- max to min.
                P.camera_mode_store:set(state)
            end)
        end
    end

    P.generate_sender_message = function(sender_link)
        return "follow <a href=\"" .. sender_link .. "\" target=\"_blank\">this link</a> to the camera sender"
    end

    P.handle_notify = function(line)
        P.plugin_handle:info("camera server notify line " .. line)
        local space = string.find(line, ' ')
        if not space then
            P.plugin_handle:error("camera server notify line didn't specify a slot")
            return
        end

        local type = string.sub(line, 1, space - 1)
        if type == "new_feed" then
            local slot_str = string.sub(line, space + 1)
            local slot = tonumber(slot_str)
            P.slot_active_states[slot + 1] = true
            P.camera_handles[slot + 1] = P.plugin_handle:create_virtual_window("camera-" .. (slot + 1), {
                set_geometry = function(x_offset, y_offset, width, height, alignment, z)
                    local cmd = string.format(
                        "set_geometry_relative_to_canvas %d %s %d %d %d %d %d",
                        slot, alignment, x_offset, y_offset, width, height, z
                    )
                    P.camera_server_handle:writeln(cmd)
                    P.update_camera_visible_state(slot + 1, true)
                end,
                map = function()
                    P.camera_server_handle:writeln("show " .. slot)
                    P.update_camera_visible_state(slot + 1, true)
                end,
                unmap = function()
                    P.camera_server_handle:writeln("hide " .. slot)
                    P.update_camera_visible_state(slot + 1, false)
                end,
                primary_demotion_action = "make_min",
                min_geometry = function()
                    return DEFAULT_GEOMETRIES[P.mode][slot + 1]
                end,
            })
            P.plugin_handle:info("new feed on slot " .. slot)
            P.camera_modules[slot + 1]:set_status(STATUS_ACTIVE)
        elseif type == "remove_feed" then
            local slot_str = string.sub(line, space + 1)
            local slot = tonumber(slot_str)
            P.slot_active_states[slot + 1] = false
            P.camera_handles[slot + 1]:unclaim()
            P.camera_handles[slot + 1] = nil
            P.update_camera_visible_state(slot + 1, false)
            P.plugin_handle:info("removed feed on slot " .. slot)
            -- Only set status to waiting when status was active previously. This
            -- prevents overwriting an inactive status.
            if P.camera_modules[slot + 1]:get_status() == STATUS_ACTIVE then
                P.camera_modules[slot + 1]:set_status(STATUS_WAITING)
            end
        elseif type == "custom_name" then
            P.plugin_handle:warn("camera server custom_name messages are not handled yet")
        end
    end

    P.handle_camera_mode_update = function(new_state)
        if P.mode ~= new_state.mode then
            P.mode = new_state.mode
            api.reposition_windows()
        end
    end

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
    local ping_janus = args.ping_janus ~= false
    local generate_secure_tokens = args.generate_secure_tokens ~= false

    local camera_mode_store = args.camera_mode_store
    local dynamic_camera_mode = args.dynamic_camera_mode ~= false

    if cameras > 4 then
        log.warn("cvh camera plugin currently supports only up to 4 cameras, setting cameras to 4")
        cameras = 4
    end

    if args.camera_server_path == nil then
        error("cvh_camera plugin setup called without mandatory `camera_server_path` parameter")
    end
    if os.execute("test -f " .. args.camera_server_path) ~= 0 then
        error("`camera_server_path` parameter points to non-existent file, please change it to the entry point (e.g. `server.js`) of the cvh-camera camera server dist/ directory and make sure that the noepult user can access it")
    end

    if args.sender_base_url == nil then
        error("cvh_camera plugin setup called without mandatory `sender_base_url` parameter")
    end
    P.sender_base_url = args.sender_base_url

    if ping_janus ~= false then
        local janus_online_cmd = string.format([[curl --location --silent --fail --request POST --data '{"janus":"ping","transaction":"foobar"}' %s >/dev/null]], janus_url)
        if os.execute(janus_online_cmd) ~= 0 then
            error("janus can't be reached under the provided `janus_url`, try starting it on this system (e.g. `systemctl start janus`), make sure that janus.transport.http.jcfg is configured correctly")
        end
    end

    if camera_mode_store then
        P.camera_mode_store = camera_mode_store
        P.dynamic_camera_mode = dynamic_camera_mode

        local state = camera_mode_store:get()
        if not dynamic_camera_mode then
            state.any_cameras_visible = true
            camera_mode_store:set(state)
        end
        P.handle_camera_mode_update(state)
        camera_mode_store:subscribe(P.handle_camera_mode_update)
    end

    P.plugin_handle = api.register_plugin_instance("cvh-camera")
    if P.plugin_handle then
        P.camera_server_handle = P.plugin_handle:spawn_process("node", {
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

        if not P.camera_server_handle then
            P.plugin_handle:error("couldn't spawn camera server -- aborting plugin setup")
            return
        end

        local notify_create_cmd = string.format([[sh -c 'test -p "%s" || mkfifo "%s"']], notify_path, notify_path)
        os.execute(notify_create_cmd)

        P.notify_handle = P.plugin_handle:spawn_process("tail", {
            args = { "-f", notify_path },
            on_output = P.handle_notify,
        })

        if not P.notify_handle then
            P.plugin_handle:error("couldn't spawn notify listener -- aborting plugin setup")
            P.camera_server_handle:kill()
            return
        end

        for camera = 1, cameras do
            -- local camera = camera
            P.slot_active_states[camera] = false
            P.camera_visible_states[camera] = false
            local module_handle = P.plugin_handle:register_module("camera-" .. camera)
            if module_handle then
                P.camera_modules[camera] = module_handle
                module_handle:set_status(STATUS_INACTIVE)
                module_handle:register_action("start", function()
                    module_handle:info("start action")

                    local token
                    if generate_secure_tokens then
                        token = api.generate_token(20)
                    else
                        token = "token"
                    end

                    P.camera_server_handle:writeln("activate_slot " .. (camera - 1) .. " " .. token)
                    module_handle:set_status(STATUS_WAITING)

                    local sender_link = string.format(
                        "%s?slot=%d&room=%d&token=%s",
                        P.sender_base_url, camera - 1, janus_room, token
                    )
                    local sender_message = P.generate_sender_message(sender_link)
                    module_handle:info(sender_message)
                    module_handle:set_message(sender_message)
                end)
                module_handle:register_action("stop", function()
                    module_handle:info("stop action")
                    module_handle:set_status(STATUS_INACTIVE)
                    module_handle:set_message(nil)
                    P.camera_server_handle:writeln("deactivate_slot " .. (camera - 1))
                end)
                module_handle:register_action("hide", function()
                    module_handle:info("hide action")
                    if P.camera_handles[camera] then
                        P.camera_handles[camera]:hide()
                    end
                end)
                module_handle:register_action("max", function()
                    module_handle:info("max action")
                    if P.camera_handles[camera] then
                        P.camera_handles[camera]:max({ 1200, 900 })
                    end
                end)
                module_handle:register_action("min", function()
                    module_handle:info("min action")
                    if P.camera_handles[camera] then
                        P.camera_handles[camera]:min()
                    end
                end)
            end
        end
    end
end

return { setup = setup }
