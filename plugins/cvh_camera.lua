local api = neopult.api
local log = neopult.log

local default_cameras = 4

local M = {}

local camera_modules = {}

M.setup = function(args)
    local args = args or {}
    local cameras = args.cameras or default_cameras

    local plugin_handle = api.register_plugin_instance("cvh-camera")
    if plugin_handle then
        for camera = 1, cameras do
            module_handle = plugin_handle:register_module("camera-" .. camera)
            if module_handle then
                camera_modules[#camera_modules + 1] = module_handle
                module_handle:register_action("show", function() module_handle:info("show action") end)
                module_handle:register_action("hide", function() module_handle:info("hide action") end)
                module_handle:register_action("max", function() module_handle:info("max action") end)
                module_handle:register_action("min", function() module_handle:info("min action") end)
            end
        end
    end

end

return M
