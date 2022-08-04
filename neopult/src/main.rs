use anyhow::Result;
use env_logger::Env;
use std::{process, sync::Arc};
use tokio::{
    io::{self, AsyncBufReadExt, BufReader},
    signal,
    sync::{broadcast, mpsc, oneshot},
};

mod config;
mod plugin_system;
mod server;
mod window_manager;

use plugin_system::{Event, Notification, PluginSystem};
use window_manager::WindowManager;

#[derive(Debug, Clone)]
pub struct ShutdownChannels {
    pub shutdown_sender: broadcast::Sender<()>,
    pub shutdown_wait_sender: mpsc::Sender<()>,
}

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
                                println!("new module status for {}: '{:?}'", module_identifier, new_status),
                            Notification::ModuleMessageUpdate { module_identifier, new_message: Some(msg) } =>
                                println!("new module message for {}: '{}'", module_identifier, msg),
                            Notification::ModuleMessageUpdate { module_identifier, new_message: None } =>
                                println!("cleared message for module {}", module_identifier),
                            Notification::ModuleActiveActionsUpdate { module_identifier, new_active_actions } =>
                                println!("new active actions for {}: '{:?}'", module_identifier, new_active_actions),
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

    let env_config = config::get_env_config()?;

    let (plugin_event_tx, plugin_event_rx) = mpsc::channel(64);
    let (plugin_notification_tx, _) = broadcast::channel(64);

    let (shutdown_wait_tx, mut shutdown_wait_rx) = mpsc::channel::<()>(1);
    let (shutdown_tx, _) = broadcast::channel(1);

    let shutdown_channels = ShutdownChannels {
        shutdown_sender: shutdown_tx,
        shutdown_wait_sender: shutdown_wait_tx,
    };

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
        // This looks a bit ugly, but for the io driver of tokio to work we need to be inside of
        // the runtime.block_on() call. But we can't perform the init directly inside of the
        // block_on call since the plugin system starts a new runtime for plugin calls, which is
        // not possible in the async context of a runtime. That's why this is done in a separate
        // blocking task.
        let (config_tx, config_rx) = oneshot::channel();
        let mut plugin_system_handle = runtime.spawn_blocking({
            let runtime_handle = runtime.handle().to_owned();
            let shutdown_channels = shutdown_channels.clone();
            let plugin_event_tx = plugin_event_tx.clone();
            let plugin_notification_tx = plugin_notification_tx.clone();

            move || {
                let plugin_system = match PluginSystem::init(
                    runtime_handle,
                    env_config,
                    shutdown_channels.clone(),
                    plugin_event_tx.clone(),
                    plugin_event_rx,
                    plugin_notification_tx.clone(),
                    wm,
                ) {
                    Ok(plugin_system) => plugin_system,
                    Err(e) => {
                        eprintln!("Error when initializing the plugin system: {:?}", e);
                        process::exit(1);
                    }
                };

                let config = Arc::new(plugin_system.get_config().expect("couldn't read config from lua"));
                config_tx.send(config).unwrap();

                plugin_system.event_loop()
            }
        });

        let config = config_rx.await?;

        let mut server_handle = tokio::spawn(server::start(
            config,
            plugin_event_tx.clone(),
            plugin_notification_tx.clone(),
        ));
        let terminal_client_handle =
            tokio::spawn(async {
                terminal_client(plugin_event_tx, plugin_notification_tx).await;
                println!("terminal client exited");
            });

        // This must happen before waiting for shutdown or recv() will sleep forever
        drop(shutdown_channels.shutdown_wait_sender);

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
            _ = signal::ctrl_c() => {
                println!("got ctrl-c, shutting down gracefully (press ctrl-c again to force shutdown)");
                let _ = shutdown_channels.shutdown_sender.send(());
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
