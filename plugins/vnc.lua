local api = neopult.api
local log = neopult.log

local FEED_START = "Desktop name "
local FEED_END = "ssvncviewer: VNC server closed connection"

local STATUS_WAITING = "waiting"
local STATUS_ACTIVE = "active"
local STATUS_INACTIVE = "inactive"

local M = {}

M.plugin_handle = nil
M.module_handle = nil
M.window_handle = nil

M.handle_line = function(line)
    local cur_status = M.module_handle:get_status()
    if cur_status == STATUS_WAITING then
        if string.sub(line, 1, #FEED_START) == FEED_START then
            M.plugin_handle:debug("got new vnc feed")
            M.window_handle = M.plugin_handle:claim_window("ssvncviewer", { timeout_ms = 1000 })
            if M.window_handle then
                M.window_handle:max({ 1920, 1080 })
            end
            M.module_handle:set_status(STATUS_ACTIVE)
        end
    elseif cur_status == STATUS_ACTIVE then
        if string.sub(line, 1, #FEED_END) == FEED_END then
            M.plugin_handle:debug("vnc feed stopped")
            M.window_handle:unclaim()
            M.window_handle = nil
            M.module_handle:set_status(STATUS_WAITING)
        end
    end
end

M.setup = function()
    log.debug("vnc module setup")
    M.plugin_handle = api.register_plugin_instance("vnc")
    if M.plugin_handle then
        M.plugin_handle:debug("sucessfully created plugin handle")

        M.module_handle = M.plugin_handle:register_module("vnc")

        if M.module_handle then
            M.module_handle:set_status(STATUS_INACTIVE)

            M.module_handle:register_action("start", function()
                M.module_handle:info("start action called")
                if M.module_handle:get_status() == STATUS_INACTIVE then
                    M.process_handle = M.plugin_handle:spawn_process("ssvncviewer", {
                        args = { "-listen", "6" },
                        on_output = M.handle_line,
                    })
                    M.plugin_handle:info("connect to localhost:5506")
                    M.module_handle:set_status(STATUS_WAITING)
                end
            end)
            M.module_handle:register_action("stop", function()
                M.module_handle:info("stop action called")
                local cur_status = M.module_handle:get_status()
                if cur_status == STATUS_ACTIVE then
                    M.window_handle:unclaim()
                    M.window_handle = nil
                end
                if cur_status ~= M.INACTIVE then
                    M.process_handle:kill()
                    M.process_handle = nil
                end
                M.module_handle:set_status(STATUS_INACTIVE)
            end)
            M.module_handle:register_action("max", function()
                M.module_handle:info("max action called")
                if M.module_handle:get_status() == STATUS_ACTIVE then
                    M.window_handle:max({ 1920, 1080 })
                end
            end)
            M.module_handle:register_action("min", function()
                M.module_handle:info("min action called")
                if M.module_handle:get_status() == STATUS_ACTIVE then
                    M.window_handle:min()
                end
            end)
            M.module_handle:register_action("hide", function()
                M.module_handle:info("hide action called")
                if M.module_handle:get_status() == STATUS_ACTIVE then
                    M.window_handle:hide()
                end
            end)
        end
    end
end

return M
