local api = neopult.api

local DEFAULT_RESOLUTION = { 1920, 1080 }

local function setup(args)
    local P = {}

    args = args or {}

    local resolution = args.resolution or DEFAULT_RESOLUTION

    P.plugin_handle = api.register_plugin_instance("channel-banner", {
        on_cleanup = function()
            if P.window_handle then
                P.window_handle:max(resolution)
            end
        end
    })
    if P.plugin_handle then
        P.window_handle = P.plugin_handle:claim_window("zathura", {
            min_geometry = string.format("%dx%d+0+0", resolution[1], resolution[2]),
            ignore_managed = true
        })
        if P.window_handle then
            P.window_handle:max(resolution)
        end
    end
end

return { setup = setup }
