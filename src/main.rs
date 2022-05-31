use anyhow::Result;

mod plugin_system;

fn main() -> Result<()> {
    plugin_system::start()
}
