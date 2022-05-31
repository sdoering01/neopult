use mlua::{Lua, Table};

pub(super) fn debug(_lua: &Lua, msg: String) -> mlua::Result<()> {
    println!("LUA DBG -- {}", msg);
    Ok(())
}

pub(super) fn info(_lua: &Lua, msg: String) -> mlua::Result<()> {
    println!("LUA INF -- {}", msg);
    Ok(())
}

pub(super) fn warning(_lua: &Lua, msg: String) -> mlua::Result<()> {
    println!("LUA WRN -- {}", msg);
    Ok(())
}

pub(super) fn error(_lua: &Lua, msg: String) -> mlua::Result<()> {
    println!("LUA ERR -- {}", msg);
    Ok(())
}

pub(super) fn inject_log_functions(lua: &Lua, neopult: &Table) -> mlua::Result<()> {
    let log = lua.create_table()?;

    log.set("debug", lua.create_function(debug)?)?;
    log.set("info", lua.create_function(info)?)?;
    log.set("warning", lua.create_function(warning)?)?;
    log.set("error", lua.create_function(error)?)?;

    neopult.set("log", log)?;

    Ok(())
}
