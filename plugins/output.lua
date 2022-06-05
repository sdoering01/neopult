local api = neopult.api
local log = neopult.log

local plugin_handle = nil
local module_handle = nil

local M = {}

M.setup = function()
    log.debug("output module setup")
    plugin_handle = api.register_plugin_instance("output")
    if plugin_handle then
        module_handle = plugin_handle:register_module("output")
        if module_handle then
            local cat_handle = plugin_handle:spawn_process("cat", {
                on_output = function(line)
                    plugin_handle:info("cat line: " .. line)
                end,
                foobar = nil
            })

            if cat_handle then
                module_handle:register_action("ping", function()
                    cat_handle:write("ping ")
                    cat_handle:writeln("pong")
                end)
            end
        end

        plugin_handle:spawn_process("date", {
            on_output = function(line)
                plugin_handle:info("date line: " .. line)
            end
        })

    end
end

return M
