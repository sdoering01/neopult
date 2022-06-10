local api = neopult.api
local log = neopult.log

local default_cameras = 4

local M = {}

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
        M.plugin_handle:info("new feed on slot " .. slot)
    elseif type == "remove_feed" then
        local slot_str = string.sub(line, space + 1)
        local slot = tonumber(slot_str)
        M.slot_active_states[slot + 1] = false
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

        M.camera_modules = {}
        M.slot_active_states = {}
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
            module_handle:register_action("hide", function() module_handle:info("hide action") end)
            module_handle:register_action("max", function() module_handle:info("max action") end)
            module_handle:register_action("min", function() module_handle:info("min action") end)
        end
    end

end

return M
