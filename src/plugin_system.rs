use mlua::{Lua, Table};

pub fn start() -> anyhow::Result<()> {
    let lua = Lua::new();

    let globals = lua.globals();

    // Look for lua modules in the specified paths first
    let package_table = globals.get::<_, Table>("package")?;
    let mut lua_path: String = package_table.get("path")?;
    lua_path.insert_str(0, "./plugins/?.lua;./plugins/?/init.lua;");
    package_table.set("path", lua_path)?;

    lua.load(r#"require("init")"#)
        .set_name("init.lua")?
        .exec()?;

    Ok(())
}
