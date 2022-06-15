local api = neopult.api
local log = neopult.log


local FEED_START = "Desktop name "
local FEED_END = "ssvncviewer: VNC server closed connection"

local WAITING = "waiting"
local ACTIVE = "active"
local INACTIVE = "inactive"

local M = {}

M.state = INACTIVE

M.handle_line = function(line)
    if M.state == WAITING then
        if string.sub(line, 1, #FEED_START) == FEED_START then
            M.plugin_handle:debug("got new vnc feed")
            M.window_handle = M.plugin_handle:claim_window("ssvncviewer", { timeout_ms = 1000 })
            if M.window_handle then
                M.window_handle:max({ 1920, 1080 })
            end
            M.state = ACTIVE
        end
    elseif M.state == ACTIVE then
        if string.sub(line, 1, #FEED_END) == FEED_END then
            M.plugin_handle:debug("vnc feed stopped")
            M.window_handle:unclaim()
            M.window_handle = nil
            M.state = WAITING
        end
    end
end

M.setup = function()
    log.debug("vnc module setup")
    M.plugin_handle = api.register_plugin_instance("vnc")
    if M.plugin_handle then
        M.plugin_handle:debug("sucessfully created plugin handle")

        local module_handle = M.plugin_handle:register_module("vnc")

        if module_handle then
            module_handle:register_action("start", function()
                module_handle:info("start action called")
                if M.state == INACTIVE then
                    M.process_handle = M.plugin_handle:spawn_process("ssvncviewer", {
                        args = { "-listen", "6" },
                        on_output = M.handle_line,
                    })
                    M.plugin_handle:info("connect to localhost:5506")
                    M.state = WAITING
                end
            end)
            module_handle:register_action("stop", function()
                module_handle:info("stop action called")
                if M.state == ACTIVE then
                    M.window_handle:unclaim()
                    M.window_handle = nil
                end
                if M.state ~= M.INACTIVE then
                    M.process_handle:kill()
                    M.process_handle = nil
                end
                M.state = INACTIVE
            end)
            module_handle:register_action("max", function()
                module_handle:info("max action called")
                if M.state == ACTIVE then
                    M.window_handle:max({ 1920, 1080 })
                end
            end)
            module_handle:register_action("min", function()
                module_handle:info("min action called")
                if M.state == ACTIVE then
                    M.window_handle:min()
                end
            end)
            module_handle:register_action("hide", function()
                module_handle:info("hide action called")
                if M.state == ACTIVE then
                    M.window_handle:hide()
                end
            end)
        end
    end
end

return M
