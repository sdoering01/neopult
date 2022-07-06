-- This file describes the Neopult Lua API. The functionality is written in
-- Rust and injected into the Lua runtime before the plugins are loaded.
--
-- The provided type annotations work well with the Lua language server
-- (https://github.com/sumneko/lua-language-server).

--- @class PluginInstanceHandle
PluginInstanceHandle = {}

-- Registers a module with the given `name`. The name must be unique across all
-- modules of the plugin instance.
-- Returns nil if an error occurs (e.g. name is already taken).
--- @param name string name of the module
--- @return ModuleHandle|nil #module handle or nil if an error occurred
function PluginInstanceHandle:register_module(name) end

-- Runs the provided `cmd`. Returns a process handle or nil if an error
-- occurrs.
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
--- @return WindowHandle|nil #window handle or nil if an error occured
function PluginInstanceHandle:claim_window(class, opts) end

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
--- @param callback function function to be executed when the action is called
function ModuleHandle:register_action(name, callback) end

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


--- @class WindowHandle
WindowHandle = {}



--- @diagnostic disable-next-line
neopult = {}


-- API functions
neopult.api = {}

-- Registers a plugin instance with the given `name`. The name must be unique
-- across all plugin instances.
-- Returns nil if an error occurs (e.g. name is already taken).
--- @param name string name of the plugin instance
--- @return PluginInstanceHandle|nil #plugin instance handle or nil if an error occurred
neopult.api.register_plugin_instance = function(name) end


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
