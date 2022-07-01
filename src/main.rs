use anyhow::Result;
use env_logger::Env;
use std::process;
use tokio::{
    io::{self, AsyncBufReadExt, BufReader},
    signal,
    sync::{broadcast, mpsc, oneshot},
};

mod plugin_system;
mod server;
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
                        let json = serde_json::to_string(&notification).expect("serialization");
                        match notification {
                            Notification::ModuleStatusUpdate { module_identifier, new_status } =>
                                println!("new module status for {}: '{}'", module_identifier, new_status),
                            Notification::ModuleMessageUpdate { module_identifier, new_message: Some(msg) } =>
                                println!("new module message for {}: '{}'", module_identifier, msg),
                            Notification::ModuleMessageUpdate { module_identifier, new_message: None } =>
                                println!("cleared message for module {}", module_identifier),
                        }
                        println!("  json: {}", json);
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

    let (shutdown_wait_tx, mut shutdown_wait_rx) = mpsc::channel::<()>(1);
    let (shutdown_tx, _) = broadcast::channel(1);

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

    runtime.block_on(async {
        let mut plugin_system_handle = runtime.spawn_blocking({
            let event_tx = plugin_event_tx.clone();
            let notification_tx = plugin_notification_tx.clone();
            let shutdown_wait_tx = shutdown_wait_tx.clone();
            let shutdown_tx = shutdown_tx.clone();
            move || {
                plugin_system::start(
                    shutdown_tx,
                    shutdown_wait_tx,
                    event_tx,
                    plugin_event_rx,
                    notification_tx,
                    wm,
                )
            }
        });

        let mut server_handle = tokio::spawn(server::start(
            plugin_event_tx.clone(),
            plugin_notification_tx.clone(),
        ));
        let mut terminal_client_handle =
            tokio::spawn(terminal_client(plugin_event_tx, plugin_notification_tx));

        // This must happen before waiting for shutdown or recv() will sleep forever
        drop(shutdown_wait_tx);

        tokio::select!(
            join_result = &mut plugin_system_handle => {
                eprint!("error: plugin system exited ");
                match join_result {
                    Ok(Ok(_)) => eprintln!("without error"),
                    Ok(Err(e)) => eprintln!("with system error: {:?}", e),
                    Err(e) => eprintln!("with join error: {}", e),
                };
                server_handle.abort();
                terminal_client_handle.abort();
            },
            join_result = &mut server_handle => {
                eprint!("error: server exited ");
                match join_result {
                    Ok(Ok(_)) => eprintln!("without error"),
                    Ok(Err(e)) => eprintln!("with error: {:?}", e),
                    Err(e) => eprintln!("with join error: {}", e),
                }
                plugin_system_handle.abort();
                terminal_client_handle.abort();
            },
            _ = &mut terminal_client_handle => {
                println!("terminal client exited");
                plugin_system_handle.abort();
                server_handle.abort();
            },
            _ = signal::ctrl_c() => {
                println!("got ctrl-c, shutting down gracefully (press ctrl-c again to force shutdown)");
                let _ = shutdown_tx.send(());
                tokio::select!(
                    _ = shutdown_wait_rx.recv() => {}
                    _ = signal::ctrl_c() => {}
                );
                std::process::exit(0);
            }
        );
        Ok(())
    })
}
