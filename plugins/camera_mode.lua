local api = neopult.api

local MODES = { "cameras-inside", "cameras-outside" }

local function setup(args)
    args = args or {}

    local store = args.store

    if not store then
        error("camera mode plugin setup called without mandatory `store` parameter")
    end

    local plugin_instance_handle = api.register_plugin_instance("camera-mode")
    if plugin_instance_handle then
        local module_handle = plugin_instance_handle:register_module("camera-mode")
        if module_handle then
            local initial_mode = MODES[1]
            store:set(initial_mode)
            module_handle:set_status(initial_mode)
            for _, mode in ipairs(MODES) do
                module_handle:register_action(mode, function()
                    store:set(mode)
                    module_handle:set_status(mode)
                end)
            end
        end
    end
end

return { setup = setup }
