use crate::{
    config::{Config, EnvConfig, GLOBAL_DATA_DIR},
    window_manager::WindowManager,
    ShutdownChannels,
};
use ::log::{debug, error, info, warn};
use anyhow::Context;
use mlua::{FromLuaMulti, Function, Lua, RegistryKey, Table, ToLuaMulti, Value};
use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    fmt::{self, Display, Formatter},
    fs::{self, ReadDir},
    io,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock, Weak},
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, mpsc, oneshot};

mod api;
mod config;
mod log;

const SEPARATOR: &str = "::";
const OLD_PROCESS_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_millis(2500);
const OLD_PROCESS_SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug)]
struct LuaContext {
    env_config: Arc<EnvConfig>,
    main_runtime_handle: tokio::runtime::Handle,
    plugin_runtime: tokio::runtime::Runtime,
    plugin_instances: RwLock<Vec<Arc<PluginInstance>>>,
    event_sender: Arc<mpsc::Sender<Event>>,
    notification_sender: Arc<broadcast::Sender<Notification>>,
    window_manager: RwLock<WindowManager>,
    shutdown_sender: broadcast::Sender<()>,
    plugin_shutdown_wait_sender: Weak<mpsc::Sender<()>>,
    run_later_tasks: Mutex<VecDeque<RegistryKey>>,
    pid_dir_path: PathBuf,
}

trait LogWithPrefix {
    fn prefix_msg(&self, msg: String) -> String;

    fn debug(&self, msg: String) {
        debug!("{}", self.prefix_msg(msg));
    }

    fn info(&self, msg: String) {
        info!("{}", self.prefix_msg(msg));
    }

    fn warn(&self, msg: String) {
        warn!("{}", self.prefix_msg(msg));
    }

    fn error(&self, msg: String) {
        error!("{}", self.prefix_msg(msg));
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfo {
    plugin_instances: Vec<PluginInstanceInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginInstanceInfo {
    name: String,
    modules: Vec<ModuleInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleInfo {
    name: String,
    display_name: Option<String>,
    actions: Vec<ActionInfo>,
    active_actions: HashSet<String>,
    status: Option<ModuleStatus>,
    message: Option<ModuleMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionInfo {
    name: String,
    display_name: Option<String>,
}

#[derive(Debug)]
pub enum ClientCommand {
    CallAction {
        identifier: ActionIdentifier,
        error_sender: oneshot::Sender<anyhow::Result<()>>,
    },
}

#[derive(Debug)]
pub enum Event {
    ProcessOutput {
        line: String,
        process_name: String,
        plugin_instance: Arc<PluginInstance>,
        callback_key: Arc<RegistryKey>,
    },
    CliCommand {
        command: String,
        reply_sender: oneshot::Sender<String>,
    },
    FetchSystemInfo {
        reply_sender: oneshot::Sender<SystemInfo>,
    },
    ClientCommand(ClientCommand),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Notification {
    ModuleStatusUpdate {
        #[serde(flatten)]
        module_identifier: ModuleIdentifier,
        new_status: Option<ModuleStatus>,
    },
    ModuleMessageUpdate {
        #[serde(flatten)]
        module_identifier: ModuleIdentifier,
        new_message: Option<ModuleMessage>,
    },
    ModuleActiveActionsUpdate {
        #[serde(flatten)]
        module_identifier: ModuleIdentifier,
        new_active_actions: HashSet<String>,
    },
}

#[derive(Debug)]
pub struct PluginInstance {
    name: String,
    modules: RwLock<Vec<Arc<Module>>>,
    on_cleanup: Option<RegistryKey>,
}

impl PluginInstance {
    fn new(name: String, on_cleanup: Option<RegistryKey>) -> Self {
        Self {
            name,
            modules: RwLock::new(Vec::new()),
            on_cleanup,
        }
    }
}

impl LogWithPrefix for PluginInstance {
    fn prefix_msg(&self, msg: String) -> String {
        format!("[{}] {}", self.name, msg)
    }
}

type ModuleStatus = String;
type ModuleMessage = String;

#[derive(Debug)]
struct Module {
    name: String,
    display_name: Option<String>,
    plugin_instance_name: String,
    actions: RwLock<Vec<Action>>,
    active_actions: RwLock<HashSet<String>>,
    status: RwLock<Option<ModuleStatus>>,
    message: RwLock<Option<ModuleMessage>>,
}

impl Module {
    fn new(name: String, plugin_instance_name: String, display_name: Option<String>) -> Self {
        Self {
            name,
            display_name,
            plugin_instance_name,
            actions: RwLock::new(Vec::new()),
            active_actions: RwLock::new(HashSet::new()),
            status: RwLock::new(None),
            message: RwLock::new(None),
        }
    }
}

impl LogWithPrefix for Module {
    fn prefix_msg(&self, msg: String) -> String {
        format!(
            "[{}{}{}] {}",
            self.plugin_instance_name, SEPARATOR, self.name, msg
        )
    }
}

#[derive(Debug)]
struct Action {
    name: String,
    display_name: Option<String>,
    key: RegistryKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleIdentifier {
    pub plugin_instance: String,
    pub module: String,
}

impl Display for ModuleIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.plugin_instance, SEPARATOR, self.module)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionIdentifier {
    pub plugin_instance: String,
    pub module: String,
    pub action: String,
}

impl Display for ActionIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{}{}{}",
            self.plugin_instance, SEPARATOR, self.module, SEPARATOR, self.action
        )
    }
}

fn list_actions(ctx: &LuaContext) -> Vec<String> {
    let mut action_identifiers = vec![];
    for plugin_instance in ctx.plugin_instances.read().unwrap().iter() {
        for module in plugin_instance.modules.read().unwrap().iter() {
            for action in module.actions.read().unwrap().iter() {
                let action_identifier = format!(
                    "{}{}{}{}{}",
                    plugin_instance.name, SEPARATOR, module.name, SEPARATOR, action.name
                );
                action_identifiers.push(action_identifier);
            }
        }
    }
    action_identifiers
}

fn list_statuses(ctx: &LuaContext) -> Vec<String> {
    let mut status_lines = vec![];
    for plugin_instance in ctx.plugin_instances.read().unwrap().iter() {
        for module in plugin_instance.modules.read().unwrap().iter() {
            let status = module.status.read().unwrap();
            let status_line = format!(
                "{}{}{} -- {}",
                plugin_instance.name,
                SEPARATOR,
                module.name,
                status.as_ref().unwrap_or(&"unknown".to_string())
            );
            status_lines.push(status_line)
        }
    }
    status_lines
}

fn call_action_string(lua: &Lua, ctx: &LuaContext, action_string: &str) -> anyhow::Result<()> {
    let tokens = action_string.split(SEPARATOR).collect::<Vec<_>>();
    if tokens.len() != 3 {
        anyhow::bail!("malformed action identifier: \"{}\"", action_string);
    }
    let identifier = ActionIdentifier {
        plugin_instance: tokens[0].to_string(),
        module: tokens[1].to_string(),
        action: tokens[2].to_string(),
    };

    call_action(lua, ctx, identifier)
}

fn call_action(lua: &Lua, ctx: &LuaContext, identifier: ActionIdentifier) -> anyhow::Result<()> {
    let plugin_instances = ctx.plugin_instances.read().unwrap();
    let plugin_instance = match plugin_instances
        .iter()
        .find(|p| p.name == identifier.plugin_instance)
    {
        None => anyhow::bail!(
            "no plugin instance with name {}",
            identifier.plugin_instance
        ),
        Some(p) => p,
    };

    let modules = plugin_instance.modules.read().unwrap();
    let module = match modules.iter().find(|m| m.name == identifier.module) {
        None => anyhow::bail!("no module with name {}", identifier.module),
        Some(m) => m,
    };

    let actions = module.actions.read().unwrap();
    let action = match actions.iter().find(|a| a.name == identifier.action) {
        None => anyhow::bail!("no action with name {}", identifier.action),
        Some(a) => a,
    };

    let callback = lua
        .registry_value::<Function>(&action.key)
        .context("action key has no corresponding callback in lua registry")?;

    callback
        .call::<_, ()>(())
        .context("action callback failed")?;

    Ok(())
}

fn system_info(ctx: &LuaContext) -> SystemInfo {
    let plugin_instances = ctx
        .plugin_instances
        .read()
        .unwrap()
        .iter()
        .map(|plugin_instance| {
            let name = plugin_instance.name.clone();
            let modules = plugin_instance
                .modules
                .read()
                .unwrap()
                .iter()
                .map(|module| {
                    let name = module.name.clone();
                    let display_name = module.display_name.clone();
                    let actions = module
                        .actions
                        .read()
                        .unwrap()
                        .iter()
                        .map(|action| ActionInfo {
                            name: action.name.clone(),
                            display_name: action.display_name.clone(),
                        })
                        .collect();
                    let active_actions = module.active_actions.read().unwrap().clone();
                    let status = module.status.read().unwrap().clone();
                    let message = module.message.read().unwrap().clone();

                    ModuleInfo {
                        name,
                        display_name,
                        actions,
                        active_actions,
                        status,
                        message,
                    }
                })
                .collect();

            PluginInstanceInfo { name, modules }
        })
        .collect();

    SystemInfo { plugin_instances }
}

/// Wraps the `mlua::create_function` call and passes the `ctx` as the third argument to `func`.
fn create_context_function<'lua, 'callback, A, R, F>(
    lua: &'lua Lua,
    ctx: Arc<LuaContext>,
    func: F,
) -> mlua::Result<Function<'lua>>
where
    'lua: 'callback,
    A: FromLuaMulti<'callback>,
    R: ToLuaMulti<'callback>,
    // `mlua::create_function` uses the MaybeSend trait of the maybe_sync crate to impose the Sync
    // trait on the function only when the `async` feature of mlua is activated.
    F: 'static + Fn(&'callback Lua, A, Arc<LuaContext>) -> mlua::Result<R> + Send,
{
    lua.create_function(move |lua, lua_args| func(lua, lua_args, ctx.clone()))
}

fn inject_plugin_api(lua: &Lua, neopult: &Table, ctx: Arc<LuaContext>) -> anyhow::Result<()> {
    log::inject_log_functions(lua, neopult).context("error when injecting log functions")?;
    api::inject_api_functions(lua, neopult, ctx).context("error when injecting api functions")?;
    Ok(())
}

fn clean_old_processes(pid_items: ReadDir) {
    // The performance isn't ideal, because processes are killed one after another, with each
    // process having a grace period to shut down after a SIGINT. But ideally needing to clean old
    // processes shouldn't happen at all.
    for item in pid_items.flatten() {
        let path = item.path();
        let filename_os = item.file_name();
        let filename = filename_os.to_string_lossy();
        if filename.ends_with(".pid") {
            match filename.trim_end_matches(".pid").parse::<i32>() {
                Ok(pid) if pid > 0 => {
                    debug!("found old process with pid {}", pid);
                    let pid = Pid::from_raw(pid);
                    if signal::kill(pid, None).is_err() {
                        debug!("old process with pid {} is already dead", pid);
                    } else {
                        debug!("old process with pid {} is still alive", pid);
                        kill_old_process(pid);
                    }

                    if let Err(e) = fs::remove_file(path) {
                        warn!("error removing old process pid file: {}", e);
                    }
                }
                Ok(pid) => {
                    warn!("found old process with invalid pid {}, somebody might have messed with the pid directory", pid);
                }
                Err(e) => {
                    error!("error parsing pid filename: {}", e);
                }
            }
        }
    }
}

fn kill_old_process(pid: Pid) {
    if let Err(e) = signal::kill(pid, Signal::SIGINT) {
        warn!(
            "error sending SIGINT to old process with pid {}: {}",
            pid, e
        );
        return;
    }
    let start_time = Instant::now();
    while start_time.elapsed() < OLD_PROCESS_SHUTDOWN_GRACE_PERIOD {
        if signal::kill(pid, None).is_err() {
            return;
        }
        thread::sleep(OLD_PROCESS_SHUTDOWN_POLL_INTERVAL);
    }
    debug!(
        "old process with pid {} is still alive after grace period -- killing with SIGKILL",
        pid
    );
    if let Err(e) = signal::kill(pid, Signal::SIGKILL) {
        warn!(
            "error sending SIGKILL to old process with pid {}: {}",
            pid, e
        );
    }
}

#[derive(Debug)]
pub struct PluginSystem {
    lua: Lua,
    ctx: Arc<LuaContext>,
    event_receiver: mpsc::Receiver<Event>,
    shutdown_wait_sender: mpsc::Sender<()>,
    plugin_shutdown_wait_receiver: mpsc::Receiver<()>,
    plugin_shutdown_wait_sender: Arc<mpsc::Sender<()>>,
}

impl PluginSystem {
    pub fn init(
        main_runtime_handle: tokio::runtime::Handle,
        env_config: EnvConfig,
        shutdown_channels: ShutdownChannels,
        event_tx: mpsc::Sender<Event>,
        event_rx: mpsc::Receiver<Event>,
        notification_tx: broadcast::Sender<Notification>,
        window_manager: WindowManager,
    ) -> anyhow::Result<PluginSystem> {
        let lua = Lua::new();

        let (plugin_shutdown_wait_sender, plugin_shutdown_wait_receiver) = mpsc::channel::<()>(1);
        let plugin_shutdown_wait_sender = Arc::new(plugin_shutdown_wait_sender);

        let runtime = tokio::runtime::Builder::new_current_thread().build()?;

        {
            let globals = lua.globals();

            // Look for lua modules in the specified paths first
            let package_table = globals.get::<_, Table>("package")?;
            let lua_path: String = package_table.get("path")?;
            let channel_path = env_config.channel_home.display().to_string();
            let mut neopult_lua_path = String::new();
            for path in [&channel_path, GLOBAL_DATA_DIR] {
                neopult_lua_path = format!(
                    "{}{}/?.lua;{}/plugins/?.lua;{}/plugins/?/init.lua;",
                    neopult_lua_path, path, path, path
                );
            }
            package_table.set("path", neopult_lua_path + &lua_path)?;
        }

        let pid_dir_path = PathBuf::from(format!("/tmp/neopult-channel-{}", env_config.channel));

        match fs::read_dir(&pid_dir_path) {
            Ok(items) => {
                clean_old_processes(items);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if let Err(e) = fs::create_dir_all(&pid_dir_path) {
                    error!(
                        "couldn't create PID directory {}: {}",
                        pid_dir_path.display(),
                        e
                    );
                }
            }
            Err(e) => {
                error!(
                    "couldn't read PID directory {}: {}",
                    pid_dir_path.display(),
                    e
                );
            }
        }

        let ctx = Arc::new(LuaContext {
            env_config: Arc::new(env_config),
            main_runtime_handle,
            plugin_runtime: runtime,
            plugin_instances: RwLock::new(Vec::new()),
            event_sender: Arc::new(event_tx),
            window_manager: RwLock::new(window_manager),
            notification_sender: Arc::new(notification_tx),
            shutdown_sender: shutdown_channels.shutdown_sender,
            // The context must not own the plugin shutdown wait sender because we won't be able to drop
            // every context reference on shutdown.
            plugin_shutdown_wait_sender: Arc::downgrade(&plugin_shutdown_wait_sender),
            run_later_tasks: Mutex::new(VecDeque::new()),
            pid_dir_path,
        });

        let neopult = lua.create_table()?;
        inject_plugin_api(&lua, &neopult, ctx.clone())
            .context("error when injecting plugin api")?;
        config::inject_config_table(&lua, &neopult).context("error when injecting config table")?;
        lua.globals().set("neopult", neopult)?;

        info!("loading plugins");

        lua.load(r#"require("init")"#)
            .set_name("init.lua")?
            .exec()
            .context("error when loading plugins")?;

        info!("plugins loaded");

        let plugin_system = PluginSystem {
            lua,
            ctx,
            event_receiver: event_rx,
            shutdown_wait_sender: shutdown_channels.shutdown_wait_sender,
            plugin_shutdown_wait_receiver,
            plugin_shutdown_wait_sender,
        };
        Ok(plugin_system)
    }

    pub fn get_config(&self) -> anyhow::Result<Config> {
        let lua_config = config::get_config(&self.lua)?;

        let config = Config {
            channel: self.ctx.env_config.channel,
            neopult_home: self.ctx.env_config.neopult_home.clone(),
            channel_home: self.ctx.env_config.channel_home.clone(),
            websocket_password: lua_config.websocket_password,
        };

        Ok(config)
    }

    pub fn event_loop(self) -> io::Result<()> {
        let lua = self.lua;
        let ctx = self.ctx;
        let mut event_receiver = self.event_receiver;
        let shutdown_wait_sender = self.shutdown_wait_sender;
        let mut plugin_shutdown_wait_receiver = self.plugin_shutdown_wait_receiver;
        let plugin_shutdown_wait_sender = self.plugin_shutdown_wait_sender;

        let mut shutdown_receiver = ctx.shutdown_sender.subscribe();

        info!("starting event loop");

        let mut event_loop_counter = 0;
        loop {
            // Inner block is necessary to drop the mutex guard, so that `run_later` can be called
            // from inside `run_later` tasks.
            while let Some(func_key) = {
                let mut run_later_tasks = ctx.run_later_tasks.lock().unwrap();
                run_later_tasks.pop_front()
            } {
                if let Ok(func) = lua.registry_value::<Function>(&func_key) {
                    if let Err(e) = func.call::<_, Value>(()) {
                        error!("error when calling run_later function: {:?}", e);
                    }
                }
                let _ = lua.remove_registry_value(func_key);
            }

            let event_option = ctx.plugin_runtime.block_on({
                async {
                    tokio::select!(
                        event_option = event_receiver.recv() => event_option,
                        _ = shutdown_receiver.recv() => {
                            None
                        }
                    )
                }
            });

            // Handling the event must happen outside of the async runtime, so that non-async rust
            // functions that are called from lua can call `block_on` on the runtime.
            match event_option {
                Some(event) => handle_event(&lua, &ctx, event),
                None => break,
            };

            event_loop_counter += 1;
            if event_loop_counter > 10 {
                event_loop_counter = 0;
                lua.expire_registry_values()
            }
        }

        info!("event loop finished");
        debug!("running plugin instance cleanup callbacks");

        ctx.plugin_instances
            .read()
            .unwrap()
            .iter()
            .for_each(|plugin_instance| {
                if let Some(ref callback_key) = plugin_instance.on_cleanup {
                    match lua.registry_value::<Function>(callback_key) {
                        Ok(callback) => {
                            if let Err(e) = callback.call::<_, Value>(()) {
                                plugin_instance
                                    .error(format!("error when calling cleanup callback: {:?}", e));
                            }
                        }
                        Err(e) => {
                            plugin_instance
                                .error(format!("error when retreiving cleanup callback. {:?}", e));
                        }
                    }
                }
            });

        ctx.plugin_runtime.block_on(async {
            // Drop sender at the latest possible time so Arc upgrades are possible
            drop(plugin_shutdown_wait_sender);
            debug!("Waiting for plugin system shutdown");
            let _ = plugin_shutdown_wait_receiver.recv().await;
            debug!("Plugin system shut down");
        });

        drop(shutdown_wait_sender);

        Ok(())
    }
}

fn handle_event(lua: &Lua, ctx: &LuaContext, event: Event) {
    match event {
        Event::CliCommand {
            command,
            reply_sender,
        } => {
            if command == "actions" {
                let actions = list_actions(ctx);
                let reply = actions.join("\n");
                let _ = reply_sender.send(reply);
            } else if command == "statuses" {
                let statuses = list_statuses(ctx);
                let reply = statuses.join("\n");
                let _ = reply_sender.send(reply);
            } else if let Some(identifier) = command.strip_prefix("call ") {
                match call_action_string(lua, ctx, identifier) {
                    Ok(_) => {
                        let _ = reply_sender.send("action called successfully".to_string());
                    }
                    Err(e) => {
                        let _ = reply_sender.send(format!("error when calling action: {:?}", e));
                    }
                }
            } else {
                let _ = reply_sender.send(format!("unknown command: {}", command));
            }
        }
        Event::ProcessOutput {
            line,
            process_name,
            plugin_instance,
            callback_key,
        } => {
            if let Ok(callback) = lua.registry_value::<Function>(&callback_key) {
                if let Err(e) = callback.call::<_, Value>(line) {
                    plugin_instance.error(format!(
                        "error when handling callback for process {}: {:?}",
                        process_name, e
                    ));
                }
            }
        }
        Event::FetchSystemInfo { reply_sender } => {
            let system_info = system_info(ctx);
            if reply_sender.send(system_info).is_err() {
                warn!("fetch system info: reply receiver was closed");
            }
        }
        Event::ClientCommand(cmd) => match cmd {
            ClientCommand::CallAction {
                identifier,
                error_sender,
            } => {
                let call_result = call_action(lua, ctx, identifier);
                let _ = error_sender.send(call_result);
            }
        },
    }
}
