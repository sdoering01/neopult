local api = neopult.api

local DEFAULT_RESOLUTION = { 1920, 1080 }

local M = {}

M.setup = function(args)
    args = args or {}

    local resolution = args.resolution or DEFAULT_RESOLUTION

    M.plugin_handle = api.register_plugin_instance("channel-banner", { on_cleanup = function()
        if M.window_handle then
            M.window_handle:max(resolution)
        end
    end })
    if M.plugin_handle then
        M.window_handle = M.plugin_handle:claim_window('zathura', { ignore_managed = true })
        if M.window_handle then
            M.window_handle:max(resolution)
        end
    end
end

return M
