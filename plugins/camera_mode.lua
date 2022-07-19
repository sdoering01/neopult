-- Has to be loaded before vnc and cvh_camera

local api = neopult.api

local MODES = { { "cameras-inside", "Cameras inside VNC" }, { "cameras-outside", "Cameras outside VNC" } }

local function setup(args)
    args = args or {}

    local store = args.store

    if not store then
        error("camera mode plugin setup called without mandatory `store` parameter")
    end

    local state = { mode = MODES[1][1], any_cameras_visible = false }

    local plugin_instance_handle = api.register_plugin_instance("camera-mode")
    if plugin_instance_handle then
        local module_handle = plugin_instance_handle:register_module("camera-mode", { display_name = "Camera Mode" })
        if module_handle then
            store:set(state)
            module_handle:set_active_actions({ state.mode })
            for _, mode in ipairs(MODES) do
                local action = mode[1]
                local display_name = mode[2]
                module_handle:register_action(action, function()
                    state.mode = action
                    store:set(state)
                    module_handle:set_active_actions({ action })
                end, { display_name = display_name })
            end
        end
    end
end

return { setup = setup }
