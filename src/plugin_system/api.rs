use crate::plugin_system::{
    create_context_function, Action, Event, LogWithPrefix, LuaContext, Module, PluginInstance,
};
use crate::window_manager::{ManagedWid, MinGeometry};
use ::log::{debug, error};
use mlua::{Function, Lua, Table, UserData, UserDataMethods, Value};
use std::io::{prelude::*, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct PluginInstanceHandle {
    plugin_instance: Arc<PluginInstance>,
    ctx: Arc<LuaContext>,
}

impl PluginInstanceHandle {
    fn register_module<'lua>(
        &self,
        lua: &'lua Lua,
        (name, _args): (String, Value),
    ) -> mlua::Result<Value<'lua>> {
        let mut modules = self.plugin_instance.modules.write().unwrap();

        if modules.iter().any(|m| m.name == name) {
            self.plugin_instance.error(format!(
                "tried registering module with duplicate name {}",
                name
            ));
            Ok(Value::Nil)
        } else {
            self.plugin_instance
                .debug(format!("registering module {}", name));
            let module = Arc::new(Module::new(name, self.plugin_instance.name.clone()));
            let module_handle = ModuleHandle {
                module: module.clone(),
                ctx: self.ctx.clone(),
            };
            modules.push(module);
            let val = lua.pack(module_handle)?;
            Ok(val)
        }
    }

    fn spawn_process<'lua>(
        &self,
        lua: &'lua Lua,
        (cmd, opts): (String, Value),
    ) -> mlua::Result<Value<'lua>> {
        let mut args = Vec::<String>::new();
        if let Value::Table(ref opts_table) = opts {
            if let Ok(args_table) = opts_table.get::<_, Table>("args") {
                args = args_table
                    .pairs::<Value, String>()
                    .flatten()
                    .map(|(_idx, arg)| arg)
                    .collect();
            }
        }

        let (output_reader, output_writer) = os_pipe::pipe()?;
        let output_writer_clone = output_writer.try_clone()?;

        let child_result = Command::new(cmd.clone())
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(output_writer)
            .stderr(output_writer_clone)
            .spawn();

        let child = match child_result {
            Err(e) => {
                self.plugin_instance.error(format!(
                    "couldn't spawn process {} with args {:?}: {}",
                    cmd, args, e
                ));
                return Ok(Value::Nil);
            }
            Ok(c) => {
                self.plugin_instance.debug(format!(
                    "spawned process {} with args {:?} (PID {})",
                    cmd,
                    args,
                    c.id()
                ));
                c
            }
        };

        let mut on_output_key = None;

        if let Value::Table(opts_table) = opts {
            if let Ok(on_output) = opts_table.get::<_, Function>("on_output") {
                on_output_key = Some(lua.create_registry_value(on_output)?);
            }
        }

        if let Some(key) = on_output_key {
            let event_sender = self.ctx.event_sender.clone();
            let process_name = cmd.clone();
            let plugin_instance = self.plugin_instance.clone();
            let callback_key = Arc::new(key);
            let pid = child.id();
            thread::spawn(move || {
                let reader = BufReader::new(output_reader);
                for line_result in reader.lines() {
                    match line_result {
                        Ok(line) => {
                            let event = Event::ProcessOutput {
                                line,
                                process_name: process_name.clone(),
                                plugin_instance: plugin_instance.clone(),
                                callback_key: callback_key.clone(),
                            };
                            if event_sender.blocking_send(event).is_err() {
                                plugin_instance.warn(
                                    "event receiver was dropped, couldn't send process output"
                                        .to_string(),
                                );
                                break;
                            };
                        }
                        Err(e) => {
                            plugin_instance.error(format!(
                                "error while reading stdout of process {} (PID {}): {}",
                                process_name, pid, e
                            ));
                        }
                    }
                }
                plugin_instance.debug(format!(
                    "stdout and stderr for process {} (PID {}) closed",
                    process_name, pid
                ));
            });
        }

        let process_handle = ProcessHandle {
            cmd,
            child,
            plugin_instance: self.plugin_instance.clone(),
        };

        lua.pack(process_handle)
    }

    fn claim_window<'lua>(
        &self,
        lua: &'lua Lua,
        (class, opts): (String, Value),
    ) -> mlua::Result<Value<'lua>> {
        self.plugin_instance
            .debug(format!("Claiming window with class {}", class));

        let poll_interval_ms = 50;
        let mut timeout_ms = 250;
        let mut min_geometry = MinGeometry::default();

        if let Value::Table(opts_table) = opts {
            if let Ok(timeout) = opts_table.get::<_, u64>("timeout_ms") {
                timeout_ms = timeout;
            }
            if let Ok(min_geometry_str) = opts_table.get::<_, String>("min_geometry") {
                match min_geometry_str.parse() {
                    Ok(parsed) => min_geometry = parsed,
                    Err(e) => {
                        self.plugin_instance.warn(format!(
                            "invalid geometry string for window with class {} (using default): {}",
                            class, e
                        ));
                    }
                };
            }
        }

        self.plugin_instance.debug(format!(
            "Using min geometry for window with class {}: {:?}",
            class, min_geometry
        ));

        let mut window_manager = self.ctx.window_manager.write().unwrap();

        let timeout_end = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < timeout_end {
            match window_manager.get_window_by_class(&class) {
                Ok(Some(window)) => {
                    self.plugin_instance.debug(format!(
                        "Got window with class {}; letting the window manager manage it",
                        class
                    ));
                    match window_manager.manage_x_window(lua, window, min_geometry.clone()) {
                        Ok(id) => {
                            let window_handle = WindowHandle {
                                id,
                                ctx: self.ctx.clone(),
                                plugin_instance: self.plugin_instance.clone(),
                            };
                            return lua.pack(window_handle);
                        }
                        Err(e) => {
                            self.plugin_instance.error(format!(
                                "Couldn't manage window with class {}: {}",
                                class, e
                            ));
                        }
                    }
                }
                Ok(None) => {
                    let sleep_time = std::cmp::min(
                        Duration::from_millis(poll_interval_ms),
                        timeout_end - Instant::now(),
                    );
                    if !sleep_time.is_zero() {
                        thread::sleep(sleep_time);
                    }
                }
                Err(e) => {
                    self.plugin_instance
                        .error(format!("Error getting window with class {}: {}", class, e));
                }
            }
        }

        self.plugin_instance.warn(format!(
            "Couldn't claim window with class {} (timeout)",
            class
        ));
        Ok(Value::Nil)
    }
}

impl UserData for PluginInstanceHandle {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("debug", |_lua, this, msg: String| {
            this.plugin_instance.debug(msg);
            Ok(())
        });
        methods.add_method("info", |_lua, this, msg: String| {
            this.plugin_instance.info(msg);
            Ok(())
        });
        methods.add_method("warn", |_lua, this, msg: String| {
            this.plugin_instance.warn(msg);
            Ok(())
        });
        methods.add_method("error", |_lua, this, msg: String| {
            this.plugin_instance.error(msg);
            Ok(())
        });

        methods.add_method("register_module", |lua, this, (name, args)| {
            this.register_module(lua, (name, args))
        });

        methods.add_method("spawn_process", |lua, this, (cmd, opts)| {
            this.spawn_process(lua, (cmd, opts))
        });

        methods.add_method("claim_window", |lua, this, (class, opts)| {
            this.claim_window(lua, (class, opts))
        });
    }
}

struct ModuleHandle {
    module: Arc<Module>,
    ctx: Arc<LuaContext>,
}

impl ModuleHandle {
    fn register_action(&self, lua: &Lua, (name, callback): (String, Function)) -> mlua::Result<()> {
        let mut actions = self.module.actions.write().unwrap();
        if actions.iter().any(|a| a.name == name) {
            self.module.error(format!(
                "tried registering action with duplicate name {}",
                name
            ));
        } else {
            self.module.debug(format!("registering action {}", name));
            let key = lua.create_registry_value(callback)?;
            let action = Action { name, key };
            actions.push(action);
        }
        Ok(())
    }
}

impl UserData for ModuleHandle {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("debug", |_lua, this, msg: String| {
            this.module.debug(msg);
            Ok(())
        });
        methods.add_method("info", |_lua, this, msg: String| {
            this.module.info(msg);
            Ok(())
        });
        methods.add_method("warn", |_lua, this, msg: String| {
            this.module.warn(msg);
            Ok(())
        });
        methods.add_method("error", |_lua, this, msg: String| {
            this.module.error(msg);
            Ok(())
        });

        methods.add_method("register_action", |lua, this, (name, callback)| {
            this.register_action(lua, (name, callback))
        });
    }
}

struct ProcessHandle {
    child: Child,
    cmd: String,
    plugin_instance: Arc<PluginInstance>,
}

impl ProcessHandle {
    fn write(&self, _lua: &Lua, buf: String) -> mlua::Result<()> {
        self.child
            .stdin
            .as_ref()
            .unwrap()
            .write_all(buf.as_bytes())?;
        Ok(())
    }

    fn writeln(&self, lua: &Lua, line: String) -> mlua::Result<()> {
        self.write(lua, line + "\n")
    }

    fn kill(&mut self) -> mlua::Result<()> {
        if let Err(e) = self.child.kill() {
            self.plugin_instance.warn(format!(
                "Tried to to kill process {} (PID {}) which is not running: {}",
                self.cmd,
                self.child.id(),
                e
            ));
        }
        Ok(())
    }
}

impl UserData for ProcessHandle {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("write", |lua, this, buf| this.write(lua, buf));
        methods.add_method("writeln", |lua, this, line| this.writeln(lua, line));
        methods.add_method_mut("kill", |_lua, this, ()| this.kill());
    }
}

struct WindowHandle {
    id: ManagedWid,
    ctx: Arc<LuaContext>,
    plugin_instance: Arc<PluginInstance>,
}

impl WindowHandle {
    fn max(&self, lua: &Lua, size: Value) -> mlua::Result<()> {
        self.plugin_instance.debug(format!(
            "setting mode of window with managed wid {} to max",
            self.id
        ));

        let width;
        let height;

        match size {
            Value::Table(size_table) => {
                width = match size_table.get::<_, u16>(1) {
                    Ok(w) => w,
                    Err(e) => {
                        self.plugin_instance
                            .error(format!("couldn't get width: {}", e));
                        return Ok(());
                    }
                };

                height = match size_table.get::<_, u16>(2) {
                    Ok(h) => h,
                    Err(e) => {
                        self.plugin_instance
                            .error(format!("couldn't get height: {}", e));
                        return Ok(());
                    }
                };
            }
            _ => {
                self.plugin_instance
                    .error("first argument of max isn't a table".to_string());
                return Ok(());
            }
        }

        let mut wm = self.ctx.window_manager.write().unwrap();
        if let Err(e) = wm.max_window(lua, self.id, (width, height)) {
            self.plugin_instance
                .error(format!("error setting window mode to max: {}", e));
        }

        Ok(())
    }

    fn min(&self, lua: &Lua) -> mlua::Result<()> {
        self.plugin_instance.debug(format!(
            "setting mode of window with managed wid {} to min",
            self.id
        ));
        let mut wm = self.ctx.window_manager.write().unwrap();
        if let Err(e) = wm.min_window(lua, self.id) {
            self.plugin_instance
                .error(format!("error setting window mode to min: {}", e));
        }
        Ok(())
    }

    fn hide(&self, lua: &Lua) -> mlua::Result<()> {
        self.plugin_instance
            .debug(format!("hiding window with managed wid {}", self.id));
        let mut wm = self.ctx.window_manager.write().unwrap();
        if let Err(e) = wm.hide_window(lua, self.id) {
            self.plugin_instance
                .error(format!("error hiding window: {}", e));
        }
        Ok(())
    }

    fn unclaim(&self, lua: &Lua) -> mlua::Result<()> {
        self.plugin_instance
            .debug(format!("unclaiming window with managed wid {}", self.id));
        let mut wm = self.ctx.window_manager.write().unwrap();
        if let Err(e) = wm.release_window(lua, self.id) {
            self.plugin_instance
                .error(format!("error unclaiming window: {}", e));
        }
        Ok(())
    }
}

impl UserData for WindowHandle {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("max", |lua, this, size| this.max(lua, size));
        methods.add_method("min", |lua, this, ()| this.min(lua));
        methods.add_method("hide", |lua, this, ()| this.hide(lua));
        methods.add_method("unclaim", |lua, this, ()| this.unclaim(lua));
    }
}

fn register_plugin_instance<'lua>(
    lua: &'lua Lua,
    (name, _args): (String, Value),
    ctx: Arc<LuaContext>,
) -> mlua::Result<Value<'lua>> {
    let mut plugin_instances = ctx.plugin_instances.write().unwrap();
    if plugin_instances.iter().any(|p| p.name == name) {
        error!(
            "tried registering plugin instance with duplicate name {}",
            name
        );
        Ok(Value::Nil)
    } else {
        debug!("registering plugin instance {}", name);
        let plugin_instance = Arc::new(PluginInstance::new(name));
        let plugin_instance_handle = PluginInstanceHandle {
            plugin_instance: plugin_instance.clone(),
            ctx: ctx.clone(),
        };
        plugin_instances.push(plugin_instance);
        lua.pack(plugin_instance_handle)
    }
}

pub(super) fn inject_api_functions(
    lua: &Lua,
    neopult: &Table,
    ctx: Arc<LuaContext>,
) -> mlua::Result<()> {
    let api = lua.create_table()?;

    api.set(
        "register_plugin_instance",
        create_context_function(lua, ctx, register_plugin_instance)?,
    )?;

    neopult.set("api", api)?;

    Ok(())
}
