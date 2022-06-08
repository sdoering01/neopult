local api = neopult.api
local log = neopult.log


local M = {}

M.setup = function()
    log.debug("xapps module setup")
    M.plugin_handle = api.register_plugin_instance("xapps")
    if M.plugin_handle then
        M.plugin_handle:debug("sucessfully created plugin handle")

        local xclock_module_handle = M.plugin_handle:register_module("xclock")
        local xterm_module_handle = M.plugin_handle:register_module("xterm")
        local alacritty_module_handle = M.plugin_handle:register_module("alacritty")

        if xclock_module_handle then
            xclock_module_handle:register_action("start", function()
                xclock_module_handle:info("start action called")
                if not M.xclock_process_handle then
                    M.xclock_process_handle = M.plugin_handle:spawn_process("xclock")
                    M.xclock_handle = M.plugin_handle:claim_window("xclock", { min_geometry = "200x200-0+0" })
                end
            end)
            xclock_module_handle:register_action("stop", function()
                xclock_module_handle:info("stop action called")
                if M.xclock_process_handle then
                    M.xclock_handle:unclaim()
                    M.xclock_handle = nil

                    M.xclock_process_handle:kill()
                    M.xclock_process_handle = nil
                end
            end)
            xclock_module_handle:register_action("max", function()
                xclock_module_handle:info("max action called")
                if M.xclock_handle then
                    M.xclock_handle:max({ 400, 400 })
                end
            end)
            xclock_module_handle:register_action("max-xl", function()
                xclock_module_handle:info("max action called")
                if M.xclock_handle then
                    M.xclock_handle:max({ 600, 600 })
                end
            end)
            xclock_module_handle:register_action("min", function()
                xclock_module_handle:info("min action called")
                if M.xclock_handle then
                    M.xclock_handle:min()
                end
            end)
            xclock_module_handle:register_action("hide", function()
                xclock_module_handle:info("hide action called")
                if M.xclock_handle then
                    M.xclock_handle:hide()
                end
            end)
        end

        if xterm_module_handle then
            xterm_module_handle:register_action("start", function()
                xterm_module_handle:info("start action called")
                if not M.xterm_process_handle then
                    M.xterm_process_handle = M.plugin_handle:spawn_process("xterm")
                    M.xterm_handle = M.plugin_handle:claim_window("xterm", { min_geometry = "300x300-0-0" })
                end
            end)
            xterm_module_handle:register_action("stop", function()
                xterm_module_handle:info("stop action called")
                if M.xterm_process_handle then
                    M.xterm_handle:unclaim()
                    M.xterm_handle = nil

                    M.xterm_process_handle:kill()
                    M.xterm_process_handle = nil
                end
            end)
            xterm_module_handle:register_action("max", function()
                xterm_module_handle:info("max action called")
                if M.xterm_handle then
                    M.xterm_handle:max({ 500, 600 })
                end
            end)
            xterm_module_handle:register_action("min", function()
                xterm_module_handle:info("min action called")
                if M.xterm_handle then
                    M.xterm_handle:min()
                end
            end)
            xterm_module_handle:register_action("hide", function()
                xterm_module_handle:info("hide action called")
                if M.xterm_handle then
                    M.xterm_handle:hide()
                end
            end)
        end

        if alacritty_module_handle then
            alacritty_module_handle:register_action("start", function()
                alacritty_module_handle:info("start action called")
                if not M.alacritty_process_handle then
                    M.alacritty_process_handle = M.plugin_handle:spawn_process("alacritty")
                    -- For some reason we need to wait a bit before claiming the window of alacritty
                    -- If we don't do this, a window is claimed, but then alacritty creates a new window,
                    -- which means that the old handle is invalid
                    os.execute("sleep 0.5")
                    M.alacritty_handle = M.plugin_handle:claim_window("Alacritty", { min_geometry = "200x200+0+0" })
                end
            end)
            alacritty_module_handle:register_action("stop", function()
                alacritty_module_handle:info("stop action called")
                if M.alacritty_process_handle then
                    M.alacritty_handle:unclaim()
                    M.alacritty_handle = nil

                    M.alacritty_process_handle:kill()
                    M.alacritty_process_handle = nil
                end
            end)
            alacritty_module_handle:register_action("max", function()
                alacritty_module_handle:info("max action called")
                if M.alacritty_handle then
                    M.alacritty_handle:max({ 600, 500 })
                end
            end)
            alacritty_module_handle:register_action("min", function()
                alacritty_module_handle:info("min action called")
                if M.alacritty_handle then
                    M.alacritty_handle:min()
                end
            end)
            alacritty_module_handle:register_action("hide", function()
                alacritty_module_handle:info("hide action called")
                if M.alacritty_handle then
                    M.alacritty_handle:hide()
                end
            end)
        end
    end
end

return M
