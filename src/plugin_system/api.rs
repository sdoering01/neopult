use crate::plugin_system::{
    create_context_function, Action, LuaContext, Module, PluginInstance, SEPARATOR,
};
use ::log::{debug, error, info, warn};
use mlua::{Function, Lua, Table, UserData, UserDataMethods, Value};
use std::sync::Arc;

trait LoggableHandle {
    fn prefix_msg(&self, msg: String) -> String;

    fn debug(&self, msg: String) -> mlua::Result<()> {
        debug!("{}", self.prefix_msg(msg));
        Ok(())
    }

    fn info(&self, msg: String) -> mlua::Result<()> {
        info!("{}", self.prefix_msg(msg));
        Ok(())
    }

    fn warn(&self, msg: String) -> mlua::Result<()> {
        warn!("{}", self.prefix_msg(msg));
        Ok(())
    }

    fn error(&self, msg: String) -> mlua::Result<()> {
        error!("{}", self.prefix_msg(msg));
        Ok(())
    }
}

#[derive(Debug)]
struct PluginInstanceHandle {
    plugin_instance: Arc<PluginInstance>,
    ctx: Arc<LuaContext>,
}

impl LoggableHandle for PluginInstanceHandle {
    fn prefix_msg(&self, msg: String) -> String {
        format!("[{}] {}", self.plugin_instance.name, msg)
    }
}

impl UserData for PluginInstanceHandle {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("debug", |_lua, this, msg: String| this.debug(msg));
        methods.add_method("info", |_lua, this, msg: String| this.info(msg));
        methods.add_method("warn", |_lua, this, msg: String| this.warn(msg));
        methods.add_method("error", |_lua, this, msg: String| this.error(msg));

        methods.add_method(
            "register_module",
            |lua, this, (name, _args): (String, Value)| {
                let mut modules = this.plugin_instance.modules.write().unwrap();

                if modules.iter().any(|m| m.name == name) {
                    this.error(format!(
                        "tried registering module with duplicate name {}",
                        name
                    ))?;
                    Ok(Value::Nil)
                } else {
                    this.debug(format!("registering module {}", name))?;
                    let module = Arc::new(Module::new(name, this.plugin_instance.name.clone()));
                    let module_handle = ModuleHandle {
                        module: module.clone(),
                        ctx: this.ctx.clone(),
                    };
                    modules.push(module);
                    let val = lua.pack(module_handle)?;
                    Ok(val)
                }
            },
        );
    }
}

struct ModuleHandle {
    module: Arc<Module>,
    ctx: Arc<LuaContext>,
}

impl LoggableHandle for ModuleHandle {
    fn prefix_msg(&self, msg: String) -> String {
        format!(
            "[{}{}{}] {}",
            self.module.plugin_instance_name, SEPARATOR, self.module.name, msg
        )
    }
}

impl UserData for ModuleHandle {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("debug", |_lua, this, msg: String| this.debug(msg));
        methods.add_method("info", |_lua, this, msg: String| this.info(msg));
        methods.add_method("warn", |_lua, this, msg: String| this.warn(msg));
        methods.add_method("error", |_lua, this, msg: String| this.error(msg));

        methods.add_method(
            "register_action",
            |lua, this, (name, callback): (String, Function)| {
                let mut actions = this.module.actions.write().unwrap();
                if actions.iter().any(|a| a.name == name) {
                    this.error(format!(
                        "tried registering action with duplicate name {}",
                        name
                    ))?;
                } else {
                    this.debug(format!("registering action {}", name))?;
                    let key = lua.create_registry_value(callback)?;
                    let action = Action { name, key };
                    actions.push(action);
                }
                Ok(())
            },
        );
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
