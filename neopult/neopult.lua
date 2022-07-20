-- This file describes the Neopult Lua API. The functionality is written in
-- Rust and injected into the Lua runtime before the plugins are loaded.
--
-- The provided type annotations work well with the Lua language server
-- (https://github.com/sumneko/lua-language-server).

--- @diagnostic disable: unused-local

--- @class PluginInstanceHandle
PluginInstanceHandle = {}

-- Registers a module with the given `name`. The name must be unique across all
-- modules of the plugin instance.
--
-- Returns nil if an error occurs (e.g. name is already taken).
--- @param name string name of the module
--- @param opts? table options
---  Keys:
---  - display_name?: string
---    Name that should be displayed in the interface.
--- @return ModuleHandle|nil #module handle or nil if an error occurred
function PluginInstanceHandle:register_module(name, opts) end

-- Runs the provided `cmd`. Returns a process handle or nil if an error
-- occurs.
--
-- Note that relative paths of the command are relative to the neopult
-- binary, not the plugin script. Prefer to use executables in the PATH or
-- provide absolute paths.
--- @param cmd string executable to be spawned
--- @param opts? table options
---  Keys:
---  - on_output?: function(line: string)
---    called for each line (line ending excluded) of the process output
--- @return ProcessHandle|nil #process handle or nil if an error occurred
function PluginInstanceHandle:spawn_process(cmd, opts) end

-- Claims a window that is not already managed and whose class (WM_CLASS atom)
-- contains `class`. This puts the window to min mode automatically. This
-- operation blocks for at most `opts.timeout_ms` milliseconds.
--
-- This function should be used to claim a window of a process that was spawned
-- before.
--- @param class string substring of window's class
--- @param opts? table options
---  Keys:
---  - timeout_ms?: integer (DEFAULT: 250)
---    how many milliseconds to wait for the window at most
---  - min_geometry?: string
---    geometry to use in the min mode; if not present a default value is used;
---    the string should have the format
---    "<width>x<height><'+'|'-'><x_offset><'+'|'-'><y_offset>". Positive
---    x_offset and y_offset define the offset from the top and left, negative
---    x_offset and y_offset define the offset from the bottom and right.
--- @return WindowHandle|nil #window handle or nil if an error occurred
function PluginInstanceHandle:claim_window(class, opts) end

-- Creates a virtual window -- a window that is not shown on the screen but
-- managed by the window manager. This puts the window to min mode
-- automatically.
--
-- IMPORTANT: You must not call window manager specific functions directly
-- inside the callbacks of (virtual) windows. This is due to the callbacks
-- being called in the middle of manipulating the window manager's state. If
-- you need to do something like this, you may use `neopult.api.run_later`.
--- @param name string name of the virtual window, this makes debugging easier
--- @param opts table options
---  Keys:
---  - set_geometry fun(x_offset: integer, y_offset: integer, width: integer, height: integer, alignment: "lt"|"rt"|"rb"|"lb", z: integer) function that will be called when the window manager sets the geometry of the window
---  - map fun() function that will be called when the window manager maps (shows) the window
---  - unmap fun() function that will be called when the window manager unmaps (hides) the window
---  - primary_demotion_action "do_nothing"|"make_min"|"hide"? (DEFAULT: "do_nothing") defines what should be done when another window becomes the primary window while this window is the primary window
---  - min_geometry string? same as in `PluginInstanceHandle:claim_window`
--- @return WindowHandle|nil #window handle or nil if an error occurred
function PluginInstanceHandle:create_virtual_window(name, opts) end

-- Like `neopult.log.debug`, but scoped to the plugin instance.
--- @param msg string message to log
function PluginInstanceHandle:debug(msg) end

-- Like `neopult.log.info`, but scoped to the plugin instance.
--- @param msg string message to log
function PluginInstanceHandle:info(msg) end

-- Like `neopult.log.warn`, but scoped to the plugin instance.
--- @param msg string message to log
function PluginInstanceHandle:warn(msg) end

-- Like `neopult.log.error`, but scoped to the plugin instance.
--- @param msg string message to log
function PluginInstanceHandle:error(msg) end


--- @class ModuleHandle
ModuleHandle = {}

-- Registers an action with the given `name` for the module. The name must be
-- unique across all actions of the module. When the action is called,
-- `callback` will be executed.
--- @param name string name of the action
--- @param opts? table options
---  Keys:
---  - display_name?: string
---    Name that should be displayed in the interface.
--- @param callback function function to be executed when the action is called
function ModuleHandle:register_action(name, callback, opts) end

-- Sets the status of the module.
--- @param status string|nil the new status; nil to clear the status
function ModuleHandle:set_status(status) end

-- Returns the current status of the module or nil if the status is not set.
--- @return string|nil
function ModuleHandle:get_status() end

-- Sets the message of the module.
--
-- NOTE: This message will be rendered as HTML in the default web interface. DO
-- NOT INCLUDE UNTRUSTED USER INPUT or if you really need to, escape it
-- correctly. If you allow untrusted user input to be set as messages, the
-- client is made vulnerable to Cross Site Scripting (XSS) attacks.
--- @param status string|nil the new status; nil to clear the status
function ModuleHandle:set_message(status) end

-- Sets the active actions of the module. The default web interface will style
-- active actions differently.
--- @param actions string[] names (not display names!) of the actions to be set to active
function ModuleHandle:set_active_actions(actions) end

-- Like `neopult.log.debug`, but scoped to the module.
--- @param msg string message to log
function ModuleHandle:debug(msg) end

-- Like `neopult.log.info`, but scoped to the module.
--- @param msg string message to log
function ModuleHandle:info(msg) end

-- Like `neopult.log.warn`, but scoped to the module.
--- @param msg string message to log
function ModuleHandle:warn(msg) end

-- Like `neopult.log.error`, but scoped to the module.
--- @param msg string message to log
function ModuleHandle:error(msg) end


--- @class ProcessHandle
ProcessHandle = {}

-- Writes a line to the stdin of the process.
--- @param str string string to be written
function ProcessHandle:write(str) end

-- Same as `ProcessHandle:write` but appends '\n' to the line.
--- @param line string line to be written
function ProcessHandle:writeln(line) end

-- Kills the process with a SIGKILL signal. It is safe to call this on a
-- process handle that refers to a dead process.
function ProcessHandle:kill() end


--- @class WindowHandle
WindowHandle = {}

-- Puts the window to max mode and adds an optional margin. This will make the
-- window the primary window. Adding a margin will cause the window manager to
-- increase the screen size and leave some space in the corresponding
-- direction, when the window is the primary window.
--- @param size integer[] size of the window (e.g. { 1920, 1080 })
--- @param opts? table options
---  Keys:
---  - margin?: table
---    Keys:
---    - top?: integer (DEFAULT: 0)
---    - right?: integer (DEFAULT: 0)
---    - bottom?: integer (DEFAULT: 0)
---    - left?: integer (DEFAULT: 0)
function WindowHandle:max(size, opts) end

-- Puts the window to min mode which sets the geometry according to the
-- `min_geometry` parameter in `PluginInstanceHandle:claim_window`.
function WindowHandle:min() end

-- Puts the window to hide mode, which hides it.
function WindowHandle:hide() end

-- Unclaims the window. This means that the window manager won't manage it
-- anymore. This should generally only be done with window handles of
-- terminated processes.
--
-- Windows that have been unclaimed can only be claimed again with the
-- `ignore_managed` option of `PluginInstanceHandle:claim_window`.
function WindowHandle:unclaim() end

-- Checks whether the window is the primary window.
--- @return boolean
function WindowHandle:is_primary_window() end


--- @class StoreSubscription
StoreSubscription = {}


--- @class Store
Store = {}

-- Returns the current value of the store.
--- @return any
function Store:get() end

-- Sets the value of the store and calls all registered subscriber callbacks.
--- @param value any
function Store:set(value) end

-- Subscribes to the store. The callback will be called with the new value
-- every time the store is updated.
--- @param callback fun(new_value: any)
--- @return StoreSubscription
function Store:subscribe(callback) end

-- Remove the subscription from the store.
--- @param subscription StoreSubscription
function Store:unsubscribe(subscription) end



--- @diagnostic disable-next-line: lowercase-global
neopult = {}


-- API functions
neopult.api = {}

-- Registers a plugin instance with the given `name`. The name must be unique
-- across all plugin instances.
-- Returns nil if an error occurs (e.g. name is already taken).
--- @param name string name of the plugin instance
--- @param opts? table options
---  Keys:
---  - on_cleanup? function cleanup function that is called when the plugin system shuts down correctly; this function should not rely on any processes to still be alive
--- @return PluginInstanceHandle|nil #plugin instance handle or nil if an error occurred
neopult.api.register_plugin_instance = function(name, opts) end

-- Creates a store for communication between plugins. A store holds one value
-- of any type at a time. A store handle can be used to register subscriptions
-- in form of a callback. All callbacks will be called with the new value every
-- time the store value is updated.
--
-- NOTE: Tables are references in lua. This means that when a store holds a
-- table value which is mutated, all tables that the subscribers hold are also
-- mutated. This happens without the subscribers being notified about the
-- mutation. If you ever need to mutate a table value of a store, you should
-- set the value of the store again to notify the subscribes about the change.
--- @param initial_value? any initial value of the store
--- @return Store
neopult.api.create_store = function(initial_value) end

-- Runs the function at a later point in time. Currently this is in the event
-- loop of the plugin system, before processing new events. This makes sure,
-- that those tasks don't interfere with other events. This can be useful when
-- you need to call window manager specific functions inside window callbacks.
--- @param task function
neopult.api.run_later = function(task) end


-- Log functions
neopult.log = {}

-- Logs `msg` with log level 'debug'.
--- @param msg string message to log
neopult.log.debug = function(msg) end

-- Logs `msg` with log level 'info'.
--- @param msg string message to log
neopult.log.info = function(msg) end

-- Logs `msg` with log level 'warn'.
--- @param msg string message to log
neopult.log.warn = function(msg) end

-- Logs `msg` with log level 'error'.
--- @param msg string message to log
neopult.log.error = function(msg) end


-- Config values
--- @type { websocket_password?: string }
neopult.config = {}
