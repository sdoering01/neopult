use ::log::{debug, error, info, warn};
use anyhow::Context;
use mlua::{FromLuaMulti, Function, Lua, RegistryKey, Table, ToLuaMulti, Value};
use std::sync::Arc;
use std::sync::RwLock;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

mod api;
mod log;

const SEPARATOR: &str = "::";

#[derive(Debug)]
struct LuaContext {
    plugin_instances: RwLock<Vec<Arc<PluginInstance>>>,
    event_sender: Arc<mpsc::Sender<Event>>,
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
}

#[derive(Debug)]
pub struct PluginInstance {
    name: String,
    modules: RwLock<Vec<Arc<Module>>>,
}

impl PluginInstance {
    fn new(name: String) -> Self {
        Self {
            name,
            modules: RwLock::new(Vec::new()),
        }
    }
}

impl LogWithPrefix for PluginInstance {
    fn prefix_msg(&self, msg: String) -> String {
        format!("[{}] {}", self.name, msg)
    }
}

#[derive(Debug)]
struct Module {
    name: String,
    plugin_instance_name: String,
    actions: RwLock<Vec<Action>>,
}

impl Module {
    fn new(name: String, plugin_instance_name: String) -> Self {
        Self {
            name,
            plugin_instance_name,
            actions: RwLock::new(Vec::new()),
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
    key: RegistryKey,
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

fn call_action(lua: &Lua, ctx: &LuaContext, action_identifier: &str) -> anyhow::Result<()> {
    let tokens = action_identifier.split(SEPARATOR).collect::<Vec<_>>();
    if tokens.len() != 3 {
        anyhow::bail!("malformed action identifier: \"{}\"", action_identifier);
    }

    let plugin_instances = ctx.plugin_instances.read().unwrap();
    let plugin_instance = match plugin_instances.iter().find(|p| p.name == tokens[0]) {
        None => anyhow::bail!("no plugin instance with name {}", tokens[0]),
        Some(p) => p,
    };

    let modules = plugin_instance.modules.read().unwrap();
    let module = match modules.iter().find(|m| m.name == tokens[1]) {
        None => anyhow::bail!("no module with name {}", tokens[1]),
        Some(m) => m,
    };

    let actions = module.actions.read().unwrap();
    let action = match actions.iter().find(|a| a.name == tokens[2]) {
        None => anyhow::bail!("no action with name {}", tokens[2]),
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
    F: 'static + Fn(&'callback Lua, A, Arc<LuaContext>) -> mlua::Result<R>,
{
    lua.create_function(move |lua, lua_args| func(lua, lua_args, ctx.clone()))
}

fn inject_plugin_api(lua: &Lua, ctx: Arc<LuaContext>) -> anyhow::Result<()> {
    let neopult = lua.create_table()?;

    log::inject_log_functions(lua, &neopult).context("error when injecting log functions")?;
    api::inject_api_functions(lua, &neopult, ctx).context("error when injecting api functions")?;

    lua.globals().set("neopult", neopult)?;
    Ok(())
}

pub fn start(
    event_sender: mpsc::Sender<Event>,
    event_receiver: mpsc::Receiver<Event>,
) -> anyhow::Result<()> {
    let lua = Lua::new();

    let ctx = Arc::new(LuaContext {
        plugin_instances: RwLock::new(Vec::new()),
        event_sender: Arc::new(event_sender),
    });

    let globals = lua.globals();

    // Look for lua modules in the specified paths first
    let package_table = globals.get::<_, Table>("package")?;
    let mut lua_path: String = package_table.get("path")?;
    lua_path.insert_str(0, "./plugins/?.lua;./plugins/?/init.lua;");
    package_table.set("path", lua_path)?;

    inject_plugin_api(&lua, ctx.clone()).context("error when injecting plugin api")?;

    info!("loading plugins");

    lua.load(r#"require("init")"#)
        .set_name("init.lua")?
        .exec()
        .context("error when loading plugins")?;

    info!("plugins loaded");

    event_loop(&lua, ctx, event_receiver);

    Ok(())
}

fn event_loop(lua: &Lua, ctx: Arc<LuaContext>, mut event_receiver: mpsc::Receiver<Event>) {
    info!("starting event loop");

    while let Some(event) = event_receiver.blocking_recv() {
        match event {
            Event::CliCommand {
                command,
                reply_sender,
            } => {
                if command == "list" {
                    let actions = list_actions(&ctx);
                    let reply = actions.join("\n");
                    let _ = reply_sender.send(reply);
                } else if command.starts_with("call ") {
                    match call_action(&lua, &ctx, &command[5..]) {
                        Ok(_) => {
                            let _ = reply_sender.send("action called successfully".to_string());
                        }
                        Err(e) => {
                            let _ = reply_sender.send(format!("error when calling action: {}", e));
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
            } => match lua.registry_value::<Function>(&callback_key) {
                Ok(callback) => {
                    if let Err(e) = callback.call::<_, Value>(line) {
                        plugin_instance.error(format!(
                            "error when handling callback for process {}: {}",
                            process_name, e
                        ));
                    }
                }
                Err(_) => {}
            },
        }
    }
}
