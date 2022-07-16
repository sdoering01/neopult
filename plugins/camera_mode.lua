-- Has to be loaded before vnc and cvh_camera

local api = neopult.api

local MODES = { "cameras-inside", "cameras-outside" }

local function setup(args)
    args = args or {}

    local store = args.store

    if not store then
        error("camera mode plugin setup called without mandatory `store` parameter")
    end

    local state = { mode = MODES[1], any_cameras_visible = false }

    local plugin_instance_handle = api.register_plugin_instance("camera-mode")
    if plugin_instance_handle then
        local module_handle = plugin_instance_handle:register_module("camera-mode")
        if module_handle then
            store:set(state)
            module_handle:set_active_actions({ state.mode })
            for _, mode in ipairs(MODES) do
                module_handle:register_action(mode, function()
                    state.mode = mode
                    store:set(state)
                    module_handle:set_active_actions({ mode })
                end)
            end
        end
    end
end

return { setup = setup }
