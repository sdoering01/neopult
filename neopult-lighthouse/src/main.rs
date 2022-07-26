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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChannelInfo {
    number: u8,
    novnc_url: String,
}

/// Neopult channel overview page that guides you to your channel
#[derive(Parser, Debug)]
#[clap(name = "Neopult Lighthouse", author, version, about, long_about=None)]
struct Args {
    /// Will check for channel changes every `MS` milliseconds and rerender the page if necessary
    #[clap(short = 'i', long, value_name = "MS", default_value = "30000")]
    rerender_interval_ms: u64,

    /// Neopult home
    #[clap(short = 'n', long, value_name = "HOME", default_value = if IS_DEV { "neopult_home" } else { "/home/neopult" })]
    neopult_home: String,

    /// Should point the vnc.html of novnc; query parameters (?...) will be appended
    #[clap(short = 'u', long, value_name = "URL")]
    novnc_base_url: Option<String>,

    /// Host on which websockify can be reached by the noVNC client. If not given, noVNC will use
    /// the same host, on which it is hosted.
    #[clap(short = 'w', long, value_name = "HOST")]
    websockify_host: Option<String>,

    /// If given, the channel number will be appended and the result will be used as the path to
    /// reach websockify on the websockify-host.
    #[clap(short = 'b', long, value_name = "BASE_PATH")]
    websockify_base_path: Option<String>,

    /// Port on which websockify can be reached by the noVNC client. If given, this port will be
    /// used for all channels. This can be useful when running websockify behind a reverse proxxy.
    /// Defaults to 6080 + channel_number.
    #[clap(short = 'p', long, value_name = "PORT")]
    websockify_port: Option<u16>,
}

#[derive(Debug)]
struct Config {
    rerender_interval_ms: Duration,
    neopult_home: String,
    novnc_base_url: String,
    websockify_host: Option<String>,
    websockify_base_path: Option<String>,
    websockify_port: Option<u16>,
}

impl From<Args> for Config {
    fn from(args: Args) -> Self {
        Config {
            rerender_interval_ms: Duration::from_millis(args.rerender_interval_ms),
            neopult_home: args.neopult_home,
            novnc_base_url: args
                .novnc_base_url
                .unwrap_or_else(|| "http://localhost:6080/vnc.html".to_string()),
            websockify_host: args.websockify_host,
            websockify_base_path: args.websockify_base_path,
            websockify_port: args.websockify_port,
        }
    }
}

#[derive(Template)]
#[template(path = "channel-overview.html")]
struct ChannelOverviewTemplate<'a> {
    channels: &'a [ChannelInfo],
}

struct State {
    channel_overview_html: Arc<RwLock<String>>,
}

async fn rerender_loop(config: Config, mut channels: Vec<u8>, state: Arc<State>) {
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
    channels: &[u8],
    state: &State,
) -> askama::Result<()> {
    let channel_info = channels
        .iter()
        .map(|&channel| {
            let websockify_port = config.websockify_port.unwrap_or(6080 + (channel as u16));
            let janus_room = 1000 + (channel as u16);
            let mut novnc_url = format!(
                "{}?view_only=1&reconnect=1&bell=0&resize=scale&port={}&room={}",
                config.novnc_base_url, websockify_port, janus_room
            );
            if let Some(ref websockify_host) = config.websockify_host {
                novnc_url = format!("{}&host={}", novnc_url, websockify_host);
            }
            if let Some(ref websockify_base_path) = config.websockify_base_path {
                novnc_url = format!("{}&path={}{}", novnc_url, websockify_base_path, channel);
            }
            ChannelInfo {
                number: channel,
                novnc_url,
            }
        })
        .collect::<Vec<_>>();
    let template = ChannelOverviewTemplate {
        channels: &channel_info,
    };
    let channel_overview_html = template.render()?;
    *state.channel_overview_html.write().await = channel_overview_html;
    Ok(())
}

fn read_channels(config: &Config) -> io::Result<Vec<u8>> {
    let channel_entries = fs::read_dir(&config.neopult_home)?;
    let mut channels = channel_entries
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
                        Some(channel_number)
                    }
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    channels.sort_unstable();
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
            eprintln!("Failed to read channels: {}", e);
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
