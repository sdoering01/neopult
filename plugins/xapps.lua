local api = neopult.api
local log = neopult.log

local plugin_handle = nil

local M = {}

M.setup = function()
    log.debug("xapps module setup")
    plugin_handle = api.register_plugin_instance("xapps")
    if plugin_handle then
        plugin_handle:debug("sucessfully created plugin handle")

        local xclock_module_handle = plugin_handle:register_module("xclock")
        local xterm_module_handle = plugin_handle:register_module("xterm")
        local alacritty_module_handle = plugin_handle:register_module("alacritty")

        plugin_handle:spawn_process("xclock")
        plugin_handle:spawn_process("xterm")
        plugin_handle:spawn_process("alacritty")

        local xclock_handle = plugin_handle:claim_window("xclock", { min_geometry = "200x200-0+0" })
        local xterm_handle = plugin_handle:claim_window("xterm", { min_geometry = "300x300-0-0" })
        -- For some reason we need to wait a bit before claiming the window of alacritty
        -- If we don't do this, a window is claimed, but then alacritty creates a new window,
        -- which means that the old handle is invalid
        os.execute("sleep 0.25")
        local alacritty_handle =  plugin_handle:claim_window("Alacritty", { min_geometry = "200x200+0+0" })

        if xclock_module_handle and xclock_handle then
            xclock_module_handle:register_action("max", function()
                xclock_module_handle:info("max action called")
                xclock_handle:max({ 400, 400 })
            end)
            xclock_module_handle:register_action("min", function()
                xclock_module_handle:info("min action called")
                xclock_handle:min()
            end)
            xclock_module_handle:register_action("hide", function()
                xclock_module_handle:info("hide action called")
                xclock_handle:hide()
            end)
        end

        if xterm_module_handle and xterm_handle then
            xterm_module_handle:register_action("max", function()
                xterm_module_handle:info("max action called")
                xterm_handle:max({ 500, 600 })
            end)
            xterm_module_handle:register_action("min", function()
                xterm_module_handle:info("min action called")
                xterm_handle:min()
            end)
            xterm_module_handle:register_action("hide", function()
                xterm_module_handle:info("hide action called")
                xterm_handle:hide()
            end)
        end

        if alacritty_module_handle and alacritty_handle then
            alacritty_module_handle:register_action("max", function()
                alacritty_module_handle:info("max action called")
                alacritty_handle:max({ 600, 500 })
            end)
            alacritty_module_handle:register_action("min", function()
                alacritty_module_handle:info("min action called")
                alacritty_handle:min()
            end)
            alacritty_module_handle:register_action("hide", function()
                alacritty_module_handle:info("hide action called")
                alacritty_handle:hide()
            end)
        end
    end
end

return M
