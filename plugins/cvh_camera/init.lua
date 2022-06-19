local api = neopult.api
local log = neopult.log

local default_cameras = 4

local DEFAULT_GEOMETRIES = { "480x360-0-0", "480x360-0+0", "480x360+0+0", "480x360+0-0" }

local STATUS_WAITING = "waiting"
local STATUS_ACTIVE = "active"
local STATUS_INACTIVE = "inactive"

local M = {}

M.camera_modules = {}
M.slot_active_states = {}
M.camera_handles = {}

M.generate_secure_tokens = true

local function generate_sender_message(sender_link)
    return "follow <a href=\"" .. sender_link .. "\" target=\"_blank\">this link</a> to the camera sender"
end

local function handle_output(line)
    M.plugin_handle:info("camera server output line " .. line)
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
    local args = args or {}
    local cameras = args.cameras or default_cameras
    if args.generate_secure_tokens == false then
        M.generate_secure_tokens = false
    end

    M.plugin_handle = api.register_plugin_instance("cvh-camera")
    if M.plugin_handle then
        M.camera_server_handle = M.plugin_handle:spawn_process("node", {
            args = { "/home/simon/shared/code/cvh-camera/camera-server/dist/server.js" },
            envs = { CONFIG_PATH = "./plugins/cvh_camera/config.json" },
            on_output = handle_output
        })

        if not M.camera_server_handle then
            M.plugin_handle:error("couldn't spawn camera server -- aborting plugin setup")
            return
        end

        -- TODO: Read notify path from config file
        M.notify_handle = M.plugin_handle:spawn_process("tail", {
            args = { "-f", "camera-server-output" },
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

                local sender_link = "http://localhost:3000/camera-sender.html?token=" .. token .. "&slot=" .. (camera - 1)
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
