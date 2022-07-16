local api = neopult.api
local log = neopult.log

local FEED_END = "ssvncviewer: VNC server closed connection"

local STATUS_WAITING = "waiting"
local STATUS_ACTIVE = "active"
local STATUS_INACTIVE = "inactive"

local ACTION_MAX = "max"
local ACTION_HIDE = "hide"

local VIEWER_BINARY = "ssvncviewer"

-- TODO: Define those in camera_mode plugin and require them here
local CAMERAS_INSIDE = "cameras-inside"
local CAMERAS_OUTSIDE = "cameras-outside"
local CAMERAS_OUTSIDE_BOTTOM_MARGIN = 360

local function setup(args)
    local P = {
        plugin_handle = nil,
        module_handle = nil,
        window_handle = nil,
        resolution = nil,
        bottom_margin_should = 0,
        bottom_margin_is = 0,
    }

    P.max_window = function()
        if P.module_handle and P.module_handle:get_status() == STATUS_ACTIVE then
            P.window_handle:max(P.resolution, { margin = { bottom = P.bottom_margin_should }})
            P.bottom_margin_is = P.bottom_margin_should
            P.module_handle:set_active_actions({ ACTION_MAX })
        end
    end

    P.handle_line = function(line)
        local cur_status = P.module_handle:get_status()
        if cur_status == STATUS_WAITING then
            -- example line: `try_create_image: created *non-shm* image: 1920x1080`
            local match_fn = string.gmatch(line, "try_create_image: created.* image: (%d+)x(%d+)")
            local width_str, height_str = match_fn()
            if width_str and height_str then
                local width = tonumber(width_str)
                local height = tonumber(height_str)
                P.resolution = { width, height }
                P.plugin_handle:debug("got new vnc feed with resolution " .. width .. "x" .. height)
                P.window_handle = P.plugin_handle:claim_window("ssvncviewer", { timeout_ms = 1000 })
                if P.window_handle then
                    P.module_handle:set_status(STATUS_ACTIVE)
                    P.max_window()
                else
                    P.plugin_handle:error("got feed but could not claim window in time")
                end
            end
        elseif cur_status == STATUS_ACTIVE then
            if string.sub(line, 1, #FEED_END) == FEED_END then
                P.plugin_handle:debug("vnc feed stopped")
                P.window_handle:unclaim()
                P.window_handle = nil
                P.module_handle:set_status(STATUS_WAITING)
                P.module_handle:set_active_actions({})
            end
        end
    end

    P.handle_camera_mode_update = function(new_state)
        if new_state.mode == CAMERAS_INSIDE then
            P.bottom_margin_should = 0
        elseif new_state.mode == CAMERAS_OUTSIDE then
            if new_state.any_cameras_visible then
                P.bottom_margin_should = CAMERAS_OUTSIDE_BOTTOM_MARGIN
            else
                P.bottom_margin_should = 0
            end
        end

        if P.window_handle and P.window_handle:is_primary_window() and P.bottom_margin_should ~= P.bottom_margin_is then
            P.max_window()
        end
    end


    args = args or {}

    local listen = args.listen
    local listen_base_url = args.listen_base_url
    local camera_mode_store = args.camera_mode_store

    if listen == nil then
        error("vnc plugin setup called without mandatory `listen` parameter")
    end

    if listen_base_url == nil then
        error("vnc plugin setup called without mandatory `listen_base_url` parameter")
    end

    if camera_mode_store then
        P.handle_camera_mode_update(camera_mode_store:get())
        camera_mode_store:subscribe(P.handle_camera_mode_update)
    end

    log.debug("vnc module setup")
    P.plugin_handle = api.register_plugin_instance("vnc-" .. listen)
    if P.plugin_handle then
        P.plugin_handle:debug("sucessfully created plugin handle")

        P.module_handle = P.plugin_handle:register_module("vnc-" .. listen)

        if P.module_handle then
            P.module_handle:set_status(STATUS_INACTIVE)

            P.module_handle:register_action("start", function()
                P.module_handle:info("start action called")
                if P.module_handle:get_status() == STATUS_INACTIVE then
                    P.process_handle = P.plugin_handle:spawn_process(VIEWER_BINARY, {
                        args = { "-viewonly", "-listen", tostring(listen) },
                        on_output = P.handle_line,
                    })
                    P.module_handle:set_status(STATUS_WAITING)

                    local port = 5500 + listen
                    local address = listen_base_url .. ":" .. port
                    local message = "with a vnc client connect to " .. address
                    P.module_handle:info(message)
                    P.module_handle:set_message(message)
                end
            end)
            P.module_handle:register_action("stop", function()
                P.module_handle:info("stop action called")
                local cur_status = P.module_handle:get_status()
                if cur_status == STATUS_ACTIVE then
                    P.window_handle:unclaim()
                    P.window_handle = nil

                    -- The viewer window is created in a new process, which is
                    -- not terminated, when the "listen process" is terminated.
                    -- We need to terminate that new process manually.
                    local kill_cmd = string.format("pkill -f '^%s.*-listen %d'", VIEWER_BINARY, listen)
                    P.module_handle:debug("killing viewer with command: " .. kill_cmd)
                    os.execute(kill_cmd)
                end
                if cur_status ~= STATUS_INACTIVE then
                    P.process_handle:kill()
                    P.process_handle = nil
                end
                P.module_handle:set_status(STATUS_INACTIVE)
                P.module_handle:set_message(nil)
                P.module_handle:set_active_actions({})
            end)
            P.module_handle:register_action(ACTION_MAX, function()
                P.module_handle:info("max action called")
                P.max_window()
            end)
            P.module_handle:register_action("hide", function()
                P.module_handle:info("hide action called")
                if P.module_handle:get_status() == STATUS_ACTIVE then
                    P.window_handle:hide()
                    P.module_handle:set_active_actions({ ACTION_HIDE })
                end
            end)
        end
    end
end

return { setup = setup }
