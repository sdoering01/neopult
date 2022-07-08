local api = neopult.api
local log = neopult.log

local FEED_END = "ssvncviewer: VNC server closed connection"

local STATUS_WAITING = "waiting"
local STATUS_ACTIVE = "active"
local STATUS_INACTIVE = "inactive"

local VIEWER_BINARY = "ssvncviewer"

local M = {}

M.plugin_handle = nil
M.module_handle = nil
M.window_handle = nil

M.resolution = nil

M.handle_line = function(line)
    local cur_status = M.module_handle:get_status()
    if cur_status == STATUS_WAITING then
        -- example line: `try_create_image: created *non-shm* image: 1920x1080`
        local match_fn = string.gmatch(line, "try_create_image: created.* image: (%d+)x(%d+)")
        local width_str, height_str = match_fn()
        if width_str and height_str then
            local width = tonumber(width_str)
            local height = tonumber(height_str)
            M.resolution = { width, height }
            M.plugin_handle:debug("got new vnc feed with resolution " .. width .. "x" .. height)
            M.window_handle = M.plugin_handle:claim_window("ssvncviewer", { timeout_ms = 1000 })
            if M.window_handle then
                M.window_handle:max(M.resolution)
                M.module_handle:set_status(STATUS_ACTIVE)
            else
                M.plugin_handle:error("got feed but could not claim window in time")
            end
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

M.setup = function(args)
    args = args or {}

    local listen = args.listen

    if listen == nil then
        error("vnc plugin setup called without mandatory `listen` parameter")
    end

    log.debug("vnc module setup")
    M.plugin_handle = api.register_plugin_instance("vnc-" .. listen)
    if M.plugin_handle then
        M.plugin_handle:debug("sucessfully created plugin handle")

        M.module_handle = M.plugin_handle:register_module("vnc-" .. listen)

        if M.module_handle then
            M.module_handle:set_status(STATUS_INACTIVE)

            M.module_handle:register_action("start", function()
                M.module_handle:info("start action called")
                if M.module_handle:get_status() == STATUS_INACTIVE then
                    M.process_handle = M.plugin_handle:spawn_process(VIEWER_BINARY, {
                        args = { "-viewonly", "-listen", tostring(listen) },
                        on_output = M.handle_line,
                    })
                    M.module_handle:set_status(STATUS_WAITING)

                    local port = 5500 + listen
                    local address = "127.0.0.1:" .. port
                    local message = "with a vnc client connect to " .. address
                    M.module_handle:info(message)
                    M.module_handle:set_message(message)
                end
            end)
            M.module_handle:register_action("stop", function()
                M.module_handle:info("stop action called")
                local cur_status = M.module_handle:get_status()
                if cur_status == STATUS_ACTIVE then
                    M.window_handle:unclaim()
                    M.window_handle = nil

                    -- The viewer window is created in a new process, which is
                    -- not terminated, when the "listen process" is terminated.
                    -- We need to terminate that new process manually.
                    local kill_cmd = string.format("pkill -f '^%s.*-listen %d'", VIEWER_BINARY, listen)
                    M.module_handle:debug("killing viewer with command: " .. kill_cmd)
                    os.execute(kill_cmd)
                end
                if cur_status ~= STATUS_INACTIVE then
                    M.process_handle:kill()
                    M.process_handle = nil
                end
                M.module_handle:set_status(STATUS_INACTIVE)
                M.module_handle:set_message(nil)
            end)
            M.module_handle:register_action("max", function()
                M.module_handle:info("max action called")
                if M.module_handle:get_status() == STATUS_ACTIVE then
                    M.window_handle:max(M.resolution)
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
