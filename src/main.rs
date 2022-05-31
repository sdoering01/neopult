use anyhow::Result;
use std::io::{self, BufRead};
use tokio::sync::{oneshot, mpsc};
use std::thread;

mod plugin_system;

fn main() -> Result<()> {
    let (sender, receiver) = mpsc::channel(16);
    let plugin_system_handle = thread::spawn(move || plugin_system::start(receiver));

    let stdin = io::stdin();
    let lock = stdin.lock();
    for line in lock.lines().flatten() {
        let (tx, rx) = oneshot::channel();
        let _ = sender.blocking_send((line, tx));
        match rx.blocking_recv() {
            Ok(reply) => println!("{}", reply),
            Err(_) => println!("no reply"),
        }
    }

    drop(sender);

    plugin_system_handle.join().unwrap()?;
    Ok(())
}
