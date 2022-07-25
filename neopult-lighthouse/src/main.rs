use askama::Template;
use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Extension, Router,
};
use clap::Parser;
use env_logger::Env;
use log::{debug, error};
use std::{fs, io, net::SocketAddr, process, sync::Arc};
use tokio::{
    sync::RwLock,
    time::{self, Duration},
};

const IS_DEV: bool = cfg!(debug_assertions);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChannelInfo {
    number: i32,
}

/// Neopult channel overview page that guides you to your channel
#[derive(Parser, Debug)]
#[clap(name = "Neopult Lighthouse", author, version, about, long_about=None)]
struct Args {
    /// Will check for channel changes every `MS` milliseconds and rerender the page if necessary
    #[clap(short = 'i', long, value_name = "MS", default_value = "30000")]
    rerender_interval_ms: u64,

    /// Neopult home
    #[clap(short = 'h', long, value_name = "HOME", default_value = if IS_DEV { "neopult_home" } else { "/home/neopult" })]
    neopult_home: String,

    /// noVNC base URL; query parameters (?...) will be appended
    #[clap(short = 'u', long, value_name = "URL")]
    novnc_base_url: Option<String>,
}

#[derive(Debug)]
struct Config {
    rerender_interval_ms: Duration,
    neopult_home: String,
    novnc_base_url: String,
}

impl From<Args> for Config {
    fn from(args: Args) -> Self {
        Config {
            rerender_interval_ms: Duration::from_millis(args.rerender_interval_ms),
            neopult_home: args.neopult_home,
            novnc_base_url: args
                .novnc_base_url
                .unwrap_or("http://localhost:6080/vnc.html".to_string()),
        }
    }
}

#[derive(Template)]
#[template(path = "channel-overview.html")]
struct ChannelOverviewTemplate<'a> {
    channels: &'a [ChannelInfo],
    novnc_base_url: &'a str,
}

struct State {
    channel_overview_html: Arc<RwLock<String>>,
}

async fn rerender_loop(config: Config, mut channels: Vec<ChannelInfo>, state: Arc<State>) {
    // Leak config so it can be passed to the blocking task. This is no problem since this function
    // will run until program termination anyways, thus the config effettively has a static
    // lifetime.
    let config = Box::leak(Box::new(config));
    loop {
        time::sleep(config.rerender_interval_ms).await;
        debug!("checking for channel changes");
        let result = tokio::task::spawn_blocking(|| read_channels(config)).await;
        match result {
            Ok(Ok(new_channels)) => {
                if new_channels != channels {
                    debug!("channels changed -- rerendering");
                    match generate_channel_overview_html(config, &new_channels, &state).await {
                        Ok(_) => channels = new_channels,
                        Err(e) => error!("Failed to render channel overview template: {}", e),
                    }
                }
            }
            Ok(Err(e)) => {
                error!("Failed to read channels: {}", e);
            }
            Err(e) => {
                error!("Read channel task failed: {}", e);
            }
        }
    }
}

async fn generate_channel_overview_html(
    config: &Config,
    channels: &Vec<ChannelInfo>,
    state: &State,
) -> askama::Result<()> {
    // TODO: Add port (either None for 6080 + channel or Some(port) for the same port for all
    // channel, for running behind a reverse proxy)
    // TODO: Add websockify base path (will append channel number)
    // TODO: Add host
    let template = ChannelOverviewTemplate {
        channels,
        novnc_base_url: &config.novnc_base_url,
    };
    let channel_overview_html = template.render()?;
    *state.channel_overview_html.write().await = channel_overview_html;
    Ok(())
}

fn read_channels(config: &Config) -> io::Result<Vec<ChannelInfo>> {
    let channel_entries = fs::read_dir(&config.neopult_home)?;
    let channels = channel_entries
        .into_iter()
        .flatten()
        .flat_map(|channel| {
            let ft = channel.file_type().ok()?;
            if ft.is_dir() || ft.is_symlink() {
                match channel
                    .file_name()
                    .to_string_lossy()
                    .strip_prefix("channel-")
                {
                    Some(channel_name) => {
                        let channel_number = channel_name.parse().ok()?;
                        Some(ChannelInfo {
                            number: channel_number,
                        })
                    }
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect();
    Ok(channels)
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();

    let args = Args::parse();
    debug!("Got args: {:?}", args);
    let config = Config::from(args);
    debug!("Got config: {:?}", config);

    let state = Arc::new(State {
        channel_overview_html: Arc::new(RwLock::new(String::new())),
    });

    let channels = match read_channels(&config) {
        Ok(channels) => channels,
        Err(e) => {
            eprintln!("Failed to read channels {}", e);
            process::exit(1);
        }
    };

    if let Err(e) = generate_channel_overview_html(&config, &channels, &state).await {
        eprintln!("Failed to render channel overview template: {}", e);
        process::exit(1);
    }

    tokio::spawn(rerender_loop(config, channels, state.clone()));

    let app = Router::new()
        .route("/", get(channel_overview))
        .layer(Extension(state));

    let addr = SocketAddr::from(([127, 0, 0, 1], 4199));
    debug!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn channel_overview(Extension(state): Extension<Arc<State>>) -> impl IntoResponse {
    let html = state.channel_overview_html.read().await.clone();
    Html(html)
}
