use anyhow::Result;
use env_logger::Env;
use plugin_system::Event;
use std::io::{self, BufRead};
use std::thread;
use tokio::sync::{mpsc, oneshot};

mod plugin_system;

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();

    let (sender, receiver) = mpsc::channel(16);
    let plugin_system_handle = thread::spawn({
        let sender = sender.clone();
        move || plugin_system::start(sender, receiver)
    });

    let stdin = io::stdin();
    let lock = stdin.lock();
    for line in lock.lines().flatten() {
        let (tx, rx) = oneshot::channel();
        let _ = sender.blocking_send(Event::CliCommand {
            command: line,
            reply_sender: tx,
        });
        match rx.blocking_recv() {
            Ok(reply) => println!("{}", reply),
            Err(_) => println!("no reply"),
        }
    }

    drop(sender);

    plugin_system_handle.join().unwrap()?;
    Ok(())
}
