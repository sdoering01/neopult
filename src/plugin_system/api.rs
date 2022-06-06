use crate::plugin_system::{
    create_context_function, Action, Event, LogWithPrefix, LuaContext, Module, PluginInstance,
};
use crate::window_manager::ManagedWid;
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
        let child_result = Command::new(cmd.clone())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn();

        let mut child = match child_result {
            Err(e) => {
                self.plugin_instance
                    .error(format!("couldn't spawn process {}: {}", cmd, e));
                return Ok(Value::Nil);
            }
            Ok(c) => {
                self.plugin_instance
                    .debug(format!("spawned process {}", cmd));
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
            let stdout = child.stdout.take().unwrap();
            let event_sender = self.ctx.event_sender.clone();
            let process_name = cmd;
            let plugin_instance = self.plugin_instance.clone();
            let callback_key = Arc::new(key);
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
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
                                "error while reading stdout of process {}: {}",
                                process_name, e
                            ));
                        }
                    }
                }
            });
        }

        let process_handle = ProcessHandle { child };

        lua.pack(process_handle)
    }

    fn claim_window<'lua>(
        &self,
        lua: &'lua Lua,
        (name, opts): (String, Value),
    ) -> mlua::Result<Value<'lua>> {
        self.plugin_instance
            .debug(format!("Claiming window with name {}", name));

        let poll_interval_ms = 50;
        let mut timeout_ms = 250;

        if let Value::Table(opts_table) = opts {
            if let Ok(timeout) = opts_table.get::<_, u64>("timeout_ms") {
                timeout_ms = timeout;
            }
        }

        let mut window_manager = self.ctx.window_manager.write().unwrap();

        let timeout_end = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < timeout_end {
            match window_manager.get_window_by_name(&name) {
                Ok(Some(window)) => {
                    self.plugin_instance.debug(format!(
                        "Got window with name {}; letting the window manager manage it",
                        name
                    ));
                    match window_manager.manage_x_window(window) {
                        Ok(id) => {
                            let window_handle = WindowHandle { id };
                            return lua.pack(window_handle);
                        }
                        Err(e) => {
                            self.plugin_instance
                                .error(format!("Couldn't manage window with name {}: {}", name, e));
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
                        .error(format!("Error getting window with name {}: {}", name, e));
                }
            }
        }

        self.plugin_instance
            .warn(format!("Couldn't claim window with name {}", name));
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

        methods.add_method("claim_window", |lua, this, (name, opts)| {
            this.claim_window(lua, (name, opts))
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
}

impl UserData for ProcessHandle {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("write", |lua, this, buf| this.write(lua, buf));
        methods.add_method("writeln", |lua, this, line| this.writeln(lua, line));
    }
}

struct WindowHandle {
    id: ManagedWid,
}

impl UserData for WindowHandle {}

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
