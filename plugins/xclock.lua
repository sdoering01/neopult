local api = neopult.api
local log = neopult.log

local plugin_handle = nil
local module_handle = nil

local M = {}

M.setup = function() 
    log.debug("xclock module setup")
    plugin_handle = api.register_plugin_instance("xclock")
    if plugin_handle then
        plugin_handle:debug("sucessfully created plugin handle")
        module_handle = plugin_handle:register_module("xclock")
        if module_handle then
            module_handle:register_action("show", function() module_handle:info("show action called") end)
            module_handle:register_action("hide", function() module_handle:info("hide action called") end)
        end
    end
end

return M
