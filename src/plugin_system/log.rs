use log::{debug, error, info, warn};
use mlua::{Lua, Table};

fn debug(_lua: &Lua, msg: String) -> mlua::Result<()> {
    debug!("{}", msg);
    Ok(())
}

fn info(_lua: &Lua, msg: String) -> mlua::Result<()> {
    info!("{}", msg);
    Ok(())
}

fn warn(_lua: &Lua, msg: String) -> mlua::Result<()> {
    warn!("{}", msg);
    Ok(())
}

fn error(_lua: &Lua, msg: String) -> mlua::Result<()> {
    error!("{}", msg);
    Ok(())
}

pub(super) fn inject_log_functions(lua: &Lua, neopult: &Table) -> mlua::Result<()> {
    let log = lua.create_table()?;

    log.set("debug", lua.create_function(debug)?)?;
    log.set("info", lua.create_function(info)?)?;
    log.set("warn", lua.create_function(warn)?)?;
    log.set("error", lua.create_function(error)?)?;

    neopult.set("log", log)?;

    Ok(())
}
