use log::{error, warn};
use mlua::{Lua, Table, Value};

pub(super) struct LuaConfig {
    pub websocket_password: String,
}

impl Default for LuaConfig {
    fn default() -> Self {
        LuaConfig {
            websocket_password: "admin".to_string(),
        }
    }
}

pub(super) fn inject_config_table(lua: &Lua, neopult: &Table) -> mlua::Result<()> {
    let config_table = lua.create_table()?;
    neopult.set("config", config_table)
}

pub(super) fn get_config(lua: &Lua) -> mlua::Result<LuaConfig> {
    let mut lua_config = LuaConfig::default();

    let config_table = lua
        .globals()
        .get::<_, Table>("neopult")?
        .get::<_, Table>("config")?;

    for pair in config_table.pairs::<String, Value>() {
        match pair {
            Ok((key, value)) => match key.as_str() {
                "websocket_password" => match value {
                    Value::String(password) => {
                        lua_config.websocket_password = password.to_string_lossy().to_string();
                    }
                    _ => {
                        error!("websocket_password has to be a string string");
                    }
                },
                _ => {
                    warn!("unknown config key: {}", key);
                }
            },
            Err(_) => {
                warn!("got non-string config key");
            }
        }
    }

    Ok(lua_config)
}
