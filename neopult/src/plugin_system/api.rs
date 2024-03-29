use crate::{
    plugin_system::{
        create_context_function, Action, Event, LogWithPrefix, LuaContext, Module,
        ModuleIdentifier, ModuleMessage, ModuleStatus, Notification, PluginInstance,
    },
    window_manager::{
        ManagedWid, Margin, MinGeometry, PrimaryDemotionAction, VirtualWindowCallbacks,
    },
};
use ::log::{debug, error};
use mlua::{AnyUserData, Function, Lua, RegistryKey, Table, UserData, UserDataMethods, Value};
use rand::distributions::{Alphanumeric, DistString};
use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::{mpsc, oneshot},
};

#[derive(Debug)]
struct PluginInstanceHandle {
    plugin_instance: Arc<PluginInstance>,
    ctx: Arc<LuaContext>,
}

impl PluginInstanceHandle {
    fn register_module<'lua>(
        &self,
        lua: &'lua Lua,
        (name, opts): (String, Value),
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

            let mut display_name = None;
            if let Value::Table(opts_table) = opts {
                if let Ok(display_name_arg) = opts_table.get::<_, String>("display_name") {
                    display_name = Some(display_name_arg)
                }
            }

            let module = Arc::new(Module::new(
                name,
                self.plugin_instance.name.clone(),
                display_name,
            ));
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
        let _enter_guard = self.ctx.main_runtime_handle.enter();

        let mut args = Vec::<String>::new();
        let mut envs = HashMap::<String, String>::new();
        let mut on_output_key = None;

        if let Value::Table(ref opts_table) = opts {
            if let Ok(on_output) = opts_table.get::<_, Function>("on_output") {
                on_output_key = Some(lua.create_registry_value(on_output)?);
            }
            if let Ok(args_table) = opts_table.get::<_, Table>("args") {
                args = args_table
                    .pairs::<Value, String>()
                    .flatten()
                    .map(|(_idx, arg)| arg)
                    .collect();
            }
            if let Ok(env_table) = opts_table.get::<_, Table>("envs") {
                envs = env_table.pairs::<String, String>().flatten().collect();
            }
        }

        let child_result = Command::new(&cmd)
            .args(&args)
            .envs(&envs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let pid;
        let mut child = match child_result {
            Err(e) => {
                self.plugin_instance.error(format!(
                    "couldn't spawn process {} with args {:?} and envs {:?}: {}",
                    cmd, args, envs, e
                ));
                return Ok(Value::Nil);
            }
            Ok(c) => {
                pid = c.id().unwrap();
                self.plugin_instance.debug(format!(
                    "spawned process {} with args {:?} and envs {:?} (PID {})",
                    cmd, args, envs, pid,
                ));
                c
            }
        };

        async fn read_lines(
            source: impl AsyncReadExt + Unpin,
            event_sender: Arc<mpsc::Sender<Event>>,
            process_name: String,
            plugin_instance: Arc<PluginInstance>,
            callback_key: Option<Arc<RegistryKey>>,
            pid: u32,
            kind: &str,
        ) {
            let mut lines = BufReader::new(source).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        plugin_instance.debug(format!(
                            "process {} (PID {}) {} line: {}",
                            process_name, pid, kind, line
                        ));
                        if let Some(key) = callback_key.as_ref() {
                            let event = Event::ProcessOutput {
                                line,
                                process_name: process_name.clone(),
                                plugin_instance: plugin_instance.clone(),
                                callback_key: key.clone(),
                            };
                            if event_sender.send(event).await.is_err() {
                                plugin_instance.warn(format!(
                                    "event receiver was dropped, couldn't send process output ({})",
                                    kind
                                ));
                                break;
                            };
                        }
                    }
                    Ok(None) => {
                        plugin_instance.debug(format!(
                            "{} of process {} (PID {}) closed",
                            kind, process_name, pid
                        ));
                        break;
                    }
                    Err(e) => {
                        plugin_instance.error(format!(
                            "error while reading {} of process {} (PID {}): {}",
                            kind, process_name, pid, e
                        ));
                    }
                }
            }
        }

        let callback_key = on_output_key.map(Arc::new);
        let child_stdout = child.stdout.take().unwrap();
        tokio::spawn(read_lines(
            child_stdout,
            self.ctx.event_sender.clone(),
            cmd.clone(),
            self.plugin_instance.clone(),
            callback_key.clone(),
            pid,
            "stdout",
        ));
        let child_stderr = child.stderr.take().unwrap();
        tokio::spawn(read_lines(
            child_stderr,
            self.ctx.event_sender.clone(),
            cmd.clone(),
            self.plugin_instance.clone(),
            callback_key,
            pid,
            "stderr",
        ));

        let child_stdin = child.stdin.take().unwrap();

        let pid_file_path = self.ctx.pid_dir_path.join(format!("{}.pid", pid));
        if let Err(e) = std::fs::File::create(&pid_file_path) {
            self.plugin_instance.error(format!(
                "couldn't create PID file {}: {}",
                pid_file_path.display(),
                e
            ));
        }

        let (kill_tx, kill_rx) = oneshot::channel();

        // Shutdown handler
        tokio::spawn({
            let plugin_shutdown_wait_sender = self
                .ctx
                .plugin_shutdown_wait_sender
                .upgrade()
                .unwrap()
                .as_ref()
                .clone();
            let cmd = cmd.clone();
            async move {
                tokio::select!(
                    _ = kill_rx => {
                        match child.kill().await {
                            Ok(_) => {
                                let _ = child.wait().await;
                            }
                            Err(e) => {
                                error!("tried to kill process {} (PID {}) which is not running: {}", cmd, pid, e);
                            }
                        }
                    },
                    _ = child.wait() => {},
                );
                if let Err(e) = tokio::fs::remove_file(&pid_file_path).await {
                    error!(
                        "couldn't remove PID file {}: {}",
                        pid_file_path.display(),
                        e
                    );
                }
                drop(plugin_shutdown_wait_sender);
            }
        });

        let process_handle = ProcessHandle {
            cmd,
            pid,
            ctx: self.ctx.clone(),
            child_stdin,
            kill_sender: Some(kill_tx),
            plugin_instance: self.plugin_instance.clone(),
        };

        lua.pack(process_handle)
    }

    fn get_min_geometry_from_value(
        &self,
        lua: &Lua,
        min_geometry_val: Value,
    ) -> mlua::Result<MinGeometry> {
        let mut min_geometry = Default::default();
        match min_geometry_val {
            Value::String(min_geometry_str) => match min_geometry_str.to_string_lossy().parse() {
                Ok(parsed) => min_geometry = parsed,
                Err(e) => {
                    self.plugin_instance.warn(format!(
                        "invalid geometry string for window (using default): {}",
                        e
                    ));
                }
            },
            Value::Function(min_geometry_fn) => {
                let key = lua.create_registry_value(min_geometry_fn)?;
                min_geometry = MinGeometry::Dynamic {
                    callback_key: Arc::new(key),
                }
            }
            Value::Nil => {}
            _ => {
                error!("unexpected value for min_geometry");
            }
        };
        Ok(min_geometry)
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
        let mut ignore_managed = false;

        if let Value::Table(opts_table) = opts {
            if let Ok(timeout) = opts_table.get::<_, u64>("timeout_ms") {
                timeout_ms = timeout;
            }
            if let Ok(min_geometry_val) = opts_table.get::<_, Value>("min_geometry") {
                min_geometry = self.get_min_geometry_from_value(lua, min_geometry_val)?;
            }
            if let Ok(ignore_managed_arg) = opts_table.get::<_, bool>("ignore_managed") {
                ignore_managed = ignore_managed_arg;
            }
        }

        self.plugin_instance.debug(format!(
            "Using min geometry for window with class {}: {:?}",
            class, min_geometry
        ));

        let mut window_manager = match self.ctx.write_window_manager() {
            Some(wm) => wm,
            None => return Ok(Value::Nil),
        };

        let timeout_end = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < timeout_end {
            match window_manager.get_window_by_class(&class, ignore_managed) {
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

    fn create_virtual_window<'lua>(
        &self,
        lua: &'lua Lua,
        (name, opts): (String, Table),
    ) -> mlua::Result<Value<'lua>> {
        self.plugin_instance
            .debug(format!("Creating virtual window with name {}", name));

        let set_geometry_key = match opts.get::<_, Function>("set_geometry") {
            Ok(cb) => lua.create_registry_value(cb)?,
            Err(_) => {
                self.plugin_instance.error(format!(
                    "error when creating virtual window with name {} -- set_geometry callback isn't present or is no function",
                    name
                ));
                return Ok(Value::Nil);
            }
        };

        let map_key = match opts.get::<_, Function>("map") {
            Ok(cb) => lua.create_registry_value(cb)?,
            Err(_) => {
                self.plugin_instance.error(format!(
                    "error when creating virtual window with name {} -- map callback isn't present or is no function",
                    name
                ));
                return Ok(Value::Nil);
            }
        };

        let unmap_key = match opts.get::<_, Function>("unmap") {
            Ok(cb) => lua.create_registry_value(cb)?,
            Err(_) => {
                self.plugin_instance.error(format!(
                    "error when creating virtual window with name {} -- unmap callback isn't present or is no function",
                    name
                ));
                return Ok(Value::Nil);
            }
        };

        let mut min_geometry = MinGeometry::default();
        if let Ok(min_geometry_val) = opts.get::<_, Value>("min_geometry") {
            min_geometry = self.get_min_geometry_from_value(lua, min_geometry_val)?;
        }

        let mut primary_demotion_action = PrimaryDemotionAction::default();
        if let Ok(primary_demotion_action_str) = opts.get::<_, String>("primary_demotion_action") {
            match primary_demotion_action_str.parse() {
                Ok(parsed) => primary_demotion_action = parsed,
                Err(e) => {
                    self.plugin_instance.warn(format!(
                        "could not parse primary_demotion_action when creating \
                        window with name {} (using default): {}",
                        name, e
                    ));
                }
            }
        }

        let callbacks = VirtualWindowCallbacks {
            set_geometry_key,
            map_key,
            unmap_key,
        };

        let mut wm = match self.ctx.write_window_manager() {
            Some(wm) => wm,
            None => return Ok(Value::Nil),
        };
        match wm.manage_virtual_window(
            lua,
            name.clone(),
            callbacks,
            min_geometry,
            primary_demotion_action,
        ) {
            Ok(id) => {
                let window_handle = WindowHandle {
                    id,
                    ctx: self.ctx.clone(),
                    plugin_instance: self.plugin_instance.clone(),
                };
                lua.pack(window_handle)
            }
            Err(e) => {
                self.plugin_instance.error(format!(
                    "couldn't create virtual window with name {}: {}",
                    name, e
                ));
                Ok(Value::Nil)
            }
        }
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

        methods.add_method("create_virtual_window", |lua, this, (name, opts)| {
            this.create_virtual_window(lua, (name, opts))
        });
    }
}

struct ModuleHandle {
    module: Arc<Module>,
    ctx: Arc<LuaContext>,
}

impl ModuleHandle {
    fn register_action(
        &self,
        lua: &Lua,
        (name, callback, opts): (String, Function, Value),
    ) -> mlua::Result<()> {
        let mut actions = self.module.actions.write().unwrap();
        if actions.iter().any(|a| a.name == name) {
            self.module.error(format!(
                "tried registering action with duplicate name {}",
                name
            ));
        } else {
            self.module.debug(format!("registering action {}", name));

            let mut display_name = None;
            if let Value::Table(opts_table) = opts {
                if let Ok(display_name_arg) = opts_table.get::<_, String>("display_name") {
                    display_name = Some(display_name_arg);
                }
            }

            let key = lua.create_registry_value(callback)?;
            let action = Action {
                name,
                display_name,
                key,
            };
            actions.push(action);
        }
        Ok(())
    }

    fn set_status(&self, status: Option<ModuleStatus>) -> mlua::Result<()> {
        self.module
            .debug(format!("setting module status to '{:?}'", status));
        let mut module_status = self.module.status.write().unwrap();
        *module_status = status.clone();

        let _ = self
            .ctx
            .notification_sender
            .send(Notification::ModuleStatusUpdate {
                module_identifier: ModuleIdentifier {
                    plugin_instance: self.module.plugin_instance_name.clone(),
                    module: self.module.name.clone(),
                },
                new_status: status,
            });

        Ok(())
    }

    fn get_status<'lua>(&self, lua: &'lua Lua) -> mlua::Result<Value<'lua>> {
        let module_status = self.module.status.read().unwrap();
        match module_status.as_ref() {
            Some(status) => Ok(Value::String(lua.create_string(status)?)),
            None => Ok(Value::Nil),
        }
    }

    fn set_message(&self, message: Option<ModuleMessage>) -> mlua::Result<()> {
        self.module
            .debug(format!("setting module message to '{:?}'", message));
        let mut module_message = self.module.message.write().unwrap();
        *module_message = message.clone();

        let _ = self
            .ctx
            .notification_sender
            .send(Notification::ModuleMessageUpdate {
                module_identifier: ModuleIdentifier {
                    plugin_instance: self.module.plugin_instance_name.clone(),
                    module: self.module.name.clone(),
                },
                new_message: message,
            });

        Ok(())
    }

    fn set_active_actions(&self, actions: Vec<String>) -> mlua::Result<()> {
        self.module
            .debug(format!("setting active actions to '{:?}'", actions));
        let mut active_actions = self.module.active_actions.write().unwrap();
        active_actions.clear();
        active_actions.extend(actions);

        let _ = self
            .ctx
            .notification_sender
            .send(Notification::ModuleActiveActionsUpdate {
                module_identifier: ModuleIdentifier {
                    plugin_instance: self.module.plugin_instance_name.clone(),
                    module: self.module.name.clone(),
                },
                new_active_actions: active_actions.clone(),
            });

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

        methods.add_method("register_action", |lua, this, args| {
            this.register_action(lua, args)
        });

        methods.add_method("set_status", |_lua, this, status| this.set_status(status));

        methods.add_method("get_status", |lua, this, ()| this.get_status(lua));

        methods.add_method("set_message", |_lua, this, message| {
            this.set_message(message)
        });

        methods.add_method("set_active_actions", |_lua, this, actions| {
            this.set_active_actions(actions)
        });
    }
}

struct ProcessHandle {
    child_stdin: ChildStdin,
    kill_sender: Option<oneshot::Sender<()>>,
    ctx: Arc<LuaContext>,
    cmd: String,
    pid: u32,
    plugin_instance: Arc<PluginInstance>,
}

impl ProcessHandle {
    fn write(&mut self, _lua: &Lua, buf: String) -> mlua::Result<()> {
        self.ctx.plugin_runtime.block_on(async {
            // Fails when process is not running anymore
            self.child_stdin.write_all(buf.as_bytes()).await
        })?;
        Ok(())
    }

    fn writeln(&mut self, lua: &Lua, line: String) -> mlua::Result<()> {
        self.write(lua, line + "\n")
    }

    fn kill(&mut self) -> mlua::Result<()> {
        self.plugin_instance
            .debug(format!("killing process {} (PID {})", self.cmd, self.pid));
        match self.kill_sender.take() {
            Some(kill_tx) => {
                if kill_tx.send(()).is_err() {
                    self.plugin_instance.warn(format!(
                        "tried to kill process {} (PID {}) which is not running",
                        self.cmd, self.pid
                    ));
                }
            }
            None => {
                self.plugin_instance.warn(format!(
                    "tried to kill process {} (PID {}) which was already killed explicitly",
                    self.cmd, self.pid
                ));
            }
        }
        Ok(())
    }
}

impl UserData for ProcessHandle {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("write", |lua, this, buf| this.write(lua, buf));
        methods.add_method_mut("writeln", |lua, this, line| this.writeln(lua, line));
        methods.add_method_mut("kill", |_lua, this, ()| this.kill());
    }
}

struct WindowHandle {
    id: ManagedWid,
    ctx: Arc<LuaContext>,
    plugin_instance: Arc<PluginInstance>,
}

impl WindowHandle {
    fn max(&self, lua: &Lua, (size_table, opts): (Table, Value)) -> mlua::Result<()> {
        self.plugin_instance.debug(format!(
            "setting mode of window with managed wid {} to max",
            self.id
        ));

        let width = match size_table.get::<_, u16>(1) {
            Ok(w) => w,
            Err(e) => {
                self.plugin_instance
                    .error(format!("couldn't get width: {}", e));
                return Ok(());
            }
        };

        let height = match size_table.get::<_, u16>(2) {
            Ok(h) => h,
            Err(e) => {
                self.plugin_instance
                    .error(format!("couldn't get height: {}", e));
                return Ok(());
            }
        };

        let mut margin = Margin::default();

        if let Value::Table(opts_table) = opts {
            if let Ok(margin_table) = opts_table.get::<_, Table>("margin") {
                if let Ok(top) = margin_table.get::<_, u16>("top") {
                    margin.top = top;
                }
                if let Ok(right) = margin_table.get::<_, u16>("right") {
                    margin.right = right;
                }
                if let Ok(bottom) = margin_table.get::<_, u16>("bottom") {
                    margin.bottom = bottom;
                }
                if let Ok(left) = margin_table.get::<_, u16>("left") {
                    margin.left = left;
                }
            }
        }

        let mut wm = match self.ctx.write_window_manager() {
            Some(wm) => wm,
            None => return Ok(()),
        };
        if let Err(e) = wm.max_window(lua, self.id, (width, height), margin) {
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
        let mut wm = match self.ctx.write_window_manager() {
            Some(wm) => wm,
            None => return Ok(()),
        };
        if let Err(e) = wm.min_window(lua, self.id) {
            self.plugin_instance
                .error(format!("error setting window mode to min: {}", e));
        }
        Ok(())
    }

    fn hide(&self, lua: &Lua) -> mlua::Result<()> {
        self.plugin_instance
            .debug(format!("hiding window with managed wid {}", self.id));
        let mut wm = match self.ctx.write_window_manager() {
            Some(wm) => wm,
            None => return Ok(()),
        };
        if let Err(e) = wm.hide_window(lua, self.id) {
            self.plugin_instance
                .error(format!("error hiding window: {}", e));
        }
        Ok(())
    }

    fn unclaim(&self, lua: &Lua) -> mlua::Result<()> {
        self.plugin_instance
            .debug(format!("unclaiming window with managed wid {}", self.id));
        let mut wm = match self.ctx.write_window_manager() {
            Some(wm) => wm,
            None => return Ok(()),
        };
        if let Err(e) = wm.release_window(lua, self.id) {
            self.plugin_instance
                .error(format!("error unclaiming window: {}", e));
        }
        Ok(())
    }

    fn is_primary_window(&self) -> mlua::Result<bool> {
        let wm = match self.ctx.read_window_manager() {
            Some(wm) => wm,
            None => return Ok(false),
        };
        Ok(wm.is_primary_window(self.id))
    }
}

impl UserData for WindowHandle {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("max", |lua, this, args| this.max(lua, args));
        methods.add_method("min", |lua, this, ()| this.min(lua));
        methods.add_method("hide", |lua, this, ()| this.hide(lua));
        methods.add_method("unclaim", |lua, this, ()| this.unclaim(lua));
        methods.add_method("is_primary_window", |_lua, this, ()| {
            this.is_primary_window()
        });
    }
}

#[derive(Debug)]
struct Store {
    subscriber_callbacks: HashSet<Arc<RegistryKey>>,
}

impl Store {
    fn new() -> Self {
        Self {
            subscriber_callbacks: HashSet::new(),
        }
    }
}

impl UserData for Store {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_function("get", |_lua, store: AnyUserData| {
            store.get_user_value::<Value>()
        });

        methods.add_function("set", |lua, (store, value): (AnyUserData, Value)| {
            store.set_user_value(value.clone())?;
            let mut callbacks = vec![];
            for cb_key in &store.borrow::<Store>()?.subscriber_callbacks {
                if let Ok(cb) = lua.registry_value::<Function>(cb_key) {
                    callbacks.push(cb);
                };
            }
            // Call callbacks outside of store borrow, so that callbacks can call unscubscribe
            for cb in callbacks {
                if let Err(e) = cb.call::<_, Value>(value.clone()) {
                    error!("error in store subscriber callback: {:?}", e);
                }
            }
            Ok(())
        });

        methods.add_function("subscribe", |lua, (store, cb): (AnyUserData, Function)| {
            let cb_key = Arc::new(lua.create_registry_value(cb)?);
            store
                .borrow_mut::<Store>()?
                .subscriber_callbacks
                .insert(cb_key.clone());
            Ok(StoreSubscription::new(cb_key))
        });

        methods.add_function(
            "unsubscribe",
            |_lua, (store, subscription): (AnyUserData, StoreSubscription)| {
                store
                    .borrow_mut::<Store>()?
                    .subscriber_callbacks
                    .remove(&subscription.callback_key);
                Ok(())
            },
        );
    }
}

#[derive(Debug, Clone)]
struct StoreSubscription {
    callback_key: Arc<RegistryKey>,
}

impl StoreSubscription {
    fn new(callback_key: Arc<RegistryKey>) -> Self {
        Self { callback_key }
    }
}

impl UserData for StoreSubscription {}

fn register_plugin_instance<'lua>(
    lua: &'lua Lua,
    (name, opts): (String, Value),
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
        let mut cleanup_key = None;

        if let Value::Table(opts_table) = opts {
            if let Ok(cb) = opts_table.get::<_, Function>("on_cleanup") {
                cleanup_key = Some(lua.create_registry_value(cb)?);
            }
        }

        let plugin_instance = Arc::new(PluginInstance::new(name, cleanup_key));
        let plugin_instance_handle = PluginInstanceHandle {
            plugin_instance: plugin_instance.clone(),
            ctx: ctx.clone(),
        };
        plugin_instances.push(plugin_instance);
        lua.pack(plugin_instance_handle)
    }
}

fn generate_token(num_chars: u8) -> mlua::Result<String> {
    let token = Alphanumeric.sample_string(&mut rand::thread_rng(), num_chars as usize);
    Ok(token)
}

fn get_channel(_lua: &Lua, _: Value, ctx: Arc<LuaContext>) -> mlua::Result<u8> {
    Ok(ctx.env_config.channel)
}

fn get_channel_home(_lua: &Lua, _: Value, ctx: Arc<LuaContext>) -> mlua::Result<String> {
    Ok(ctx.env_config.channel_home.display().to_string())
}

fn create_store<'lua>(lua: &'lua Lua, value: Value<'lua>) -> mlua::Result<AnyUserData<'lua>> {
    let store: mlua::AnyUserData = lua.create_userdata(Store::new())?;
    store.set_user_value(value)?;
    Ok(store)
}

fn reposition_windows(lua: &Lua, _: Value, ctx: Arc<LuaContext>) -> mlua::Result<()> {
    let mut wm = match ctx.write_window_manager() {
        Some(wm) => wm,
        None => return Ok(()),
    };
    if let Err(e) = wm.reposition_windows(lua) {
        error!("error when repositioning windows: {}", e);
    }
    Ok(())
}

fn run_later(lua: &Lua, func: Function, ctx: Arc<LuaContext>) -> mlua::Result<()> {
    let func_key = lua.create_registry_value(func)?;
    ctx.run_later_tasks.lock().unwrap().push_back(func_key);
    Ok(())
}

fn escape_html(unescaped: String) -> mlua::Result<String> {
    let escaped = unescaped
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("\'", "&#039;");
    Ok(escaped)
}

pub(super) fn inject_api_functions(
    lua: &Lua,
    neopult: &Table,
    ctx: Arc<LuaContext>,
) -> mlua::Result<()> {
    let api = lua.create_table()?;

    api.set(
        "register_plugin_instance",
        create_context_function(lua, ctx.clone(), register_plugin_instance)?,
    )?;
    api.set(
        "generate_token",
        lua.create_function(|_lua, num_chars| generate_token(num_chars))?,
    )?;
    api.set(
        "get_channel",
        create_context_function(lua, ctx.clone(), get_channel)?,
    )?;
    api.set(
        "get_channel_home",
        create_context_function(lua, ctx.clone(), get_channel_home)?,
    )?;
    api.set("create_store", lua.create_function(create_store)?)?;
    api.set(
        "reposition_windows",
        create_context_function(lua, ctx.clone(), reposition_windows)?,
    )?;
    api.set("run_later", create_context_function(lua, ctx, run_later)?)?;
    api.set(
        "escape_html",
        lua.create_function(|_lua, unescaped| escape_html(unescaped))?,
    )?;

    neopult.set("api", api)?;

    Ok(())
}
