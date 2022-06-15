local api = neopult.api
local log = neopult.log

local default_cameras = 4

local M = {}

local default_geometries = { "480x360-0-0", "480x360-0+0", "480x360+0+0", "480x360+0-0" }

M.camera_modules = {}
M.slot_active_states = {}
M.camera_handles = {}

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
            min_geometry = default_geometries[slot + 1]
        })
        M.plugin_handle:info("new feed on slot " .. slot)
    elseif type == "remove_feed" then
        local slot_str = string.sub(line, space + 1)
        local slot = tonumber(slot_str)
        M.slot_active_states[slot + 1] = false
        M.camera_handles[slot + 1]:unclaim()
        M.camera_handles[slot + 1] = nil
        M.plugin_handle:info("removed feed on slot " .. slot)
    elseif type == "custom_name" then
        M.plugin_handle:warn("camera server custom_name messages are not handled yet")
    end
end

M.setup = function(args)
    local args = args or {}
    local cameras = args.cameras or default_cameras

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
            module_handle:register_action("start", function()
                module_handle:info("start action")
                -- TODO: Generate secure token every time
                local token = "token"
                M.camera_server_handle:writeln("activate_slot " .. (camera - 1) .. " " .. token)
                module_handle:info("go to http://localhost:3000/camera-sender.html?token=" .. token .. "&slot=" .. (camera - 1))
            end)
            module_handle:register_action("stop", function()
                module_handle:info("stop action")
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
