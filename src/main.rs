use anyhow::Result;
use env_logger::Env;
use std::process;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot};

mod plugin_system;
mod window_manager;

use plugin_system::{Event, Notification};
use window_manager::WindowManager;

async fn terminal_client(
    plugin_event_tx: mpsc::Sender<Event>,
    plugin_notification_tx: broadcast::Sender<Notification>,
) {
    let stdin = io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut plugin_notification_rx = plugin_notification_tx.subscribe();

    loop {
        tokio::select!(
            line_result = lines.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        let (tx, rx) = oneshot::channel();
                        let _ = plugin_event_tx
                            .send(Event::CliCommand {
                                command: line,
                                reply_sender: tx,
                            })
                            .await;
                        match rx.await {
                            Ok(reply) => println!("{}", reply),
                            Err(_) => println!("no reply"),
                        }
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("error reading line from stdin: {}", e);
                    }
                }
            },
            notification_result = plugin_notification_rx.recv() => {
                match notification_result {
                    Ok(notification) => {
                        match notification {
                            Notification::ModuleStautsUpdate { module_identifier, new_status } =>
                                println!("new module status for {}: '{}'", module_identifier, new_status),
                        }
                    }
                    Err(e) => {
                        eprintln!("error when receiving notification: {}", e);
                    }
                }
            },
        );
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();

    let (plugin_event_tx, plugin_event_rx) = mpsc::channel(64);
    let (plugin_notification_tx, _) = broadcast::channel(64);

    let wm = match WindowManager::init() {
        Ok(wm) => wm,
        Err(e) => {
            eprintln!("Error when initializing the window manager: {}", e);
            process::exit(1);
        }
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut plugin_system_handle = runtime.spawn_blocking({
        let event_tx = plugin_event_tx.clone();
        let notification_tx = plugin_notification_tx.clone();
        move || plugin_system::start(event_tx, plugin_event_rx, notification_tx, wm)
    });

    let mut terminal_client_handle =
        runtime.spawn(terminal_client(plugin_event_tx, plugin_notification_tx));

    runtime.block_on(async {
        tokio::select!(
            join_result = &mut plugin_system_handle => {
                eprint!("error: plugin system exited ");
                match join_result {
                    Ok(Ok(_)) => eprintln!("without error"),
                    Ok(Err(e)) => eprintln!("with system error: {:?}", e),
                    Err(e) => eprintln!("with join error: {}", e),
                };
                terminal_client_handle.abort();
            },
            _ = &mut terminal_client_handle => {
                println!("terminal client exited");
                plugin_system_handle.abort();
            },
        );
        Ok(())
    })
}
