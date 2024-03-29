use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, get_service},
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
use tower_http::services::ServeDir;

const IS_DEV: bool = cfg!(debug_assertions);

const STATIC_ROOT: &str = if IS_DEV {
    "neopult-lighthouse/static"
} else {
    "/usr/local/share/neopult/lighthouse/static"
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChannelInfo {
    number: u8,
    novnc_url: String,
    neopult_url: String,
}

/// Neopult channel overview page that guides you to your channel
#[derive(Parser, Debug)]
#[clap(name = "Neopult Lighthouse", author, version, about, long_about=None)]
struct Args {
    /// Will check for channel changes every `MS` milliseconds and rerender the page if necessary
    #[clap(short = 'i', long, value_name = "MS", default_value = "30000")]
    rerender_interval_ms: u64,

    /// Port on which lighthouse should run.
    #[clap(short = 'p', long, value_name = "PORT", default_value = "4199")]
    port: u16,

    /// Show channels even when they are hidden via a `lighthouse_hide` file.
    #[clap(long)]
    show_hidden_channels: bool,

    /// Neopult home
    #[clap(short = 'n', long, value_name = "HOME", default_value = if IS_DEV { "neopult_home" } else { "/home/neopult" })]
    neopult_home: String,

    /// Template for the neopult interface url. {{CHANNEL}} will be substitued with the channel
    /// number. {{PORT}} will be substituted with the port of the channel (4200 + channel).
    #[clap(
        short = 't',
        long,
        value_name = "URL_TEMPLATE",
        default_value = "http://localhost:{{PORT}}"
    )]
    neopult_url_template: String,

    /// Should point the vnc.html of novnc; query parameters (?...) will be appended
    #[clap(
        short = 'u',
        long,
        value_name = "URL",
        default_value = "http://localhost:6080/vnc.html"
    )]
    novnc_base_url: String,

    /// Host on which websockify can be reached by the noVNC client. If not given, noVNC will use
    /// the same host, on which it is hosted.
    #[clap(short = 'w', long, value_name = "HOST")]
    websockify_host: Option<String>,

    /// If given, the channel number will be appended and the result will be used as the path to
    /// reach websockify on the websockify-host.
    #[clap(short = 'b', long, value_name = "BASE_PATH")]
    websockify_base_path: Option<String>,

    /// Port on which websockify can be reached by the noVNC client. If given, this port will be
    /// used for all channels. This can be useful when running websockify behind a reverse proxy.
    /// Defaults to 6080 + channel_number.
    #[clap(short = 's', long, value_name = "PORT")]
    websockify_port: Option<u16>,
}

#[derive(Debug)]
struct Config {
    rerender_interval_ms: Duration,
    port: u16,
    show_hidden_channels: bool,
    neopult_home: String,
    neopult_url_template: String,
    novnc_base_url: String,
    websockify_host: Option<String>,
    websockify_base_path: Option<String>,
    websockify_port: Option<u16>,
}

impl From<Args> for Config {
    fn from(args: Args) -> Self {
        Config {
            rerender_interval_ms: Duration::from_millis(args.rerender_interval_ms),
            port: args.port,
            show_hidden_channels: args.show_hidden_channels,
            neopult_home: args.neopult_home,
            neopult_url_template: args.neopult_url_template,
            novnc_base_url: args.novnc_base_url,
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
                    match generate_channel_overview_html(config, &new_channels) {
                        Ok(html) => {
                            channels = new_channels;
                            *state.channel_overview_html.write().await = html;
                        }
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

fn generate_channel_overview_html(config: &Config, channels: &[u8]) -> askama::Result<String> {
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

            let mut neopult_url = config.neopult_url_template.clone();
            neopult_url = neopult_url.replace("{{PORT}}", &(4200 + (channel as u16)).to_string());
            neopult_url = neopult_url.replace("{{CHANNEL}}", &channel.to_string());

            ChannelInfo {
                number: channel,
                novnc_url,
                neopult_url,
            }
        })
        .collect::<Vec<_>>();
    let template = ChannelOverviewTemplate {
        channels: &channel_info,
    };
    template.render()
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
                        if !config.show_hidden_channels
                            && channel.path().join("lighthouse_hide").is_file()
                        {
                            None
                        } else {
                            let channel_number = channel_name.parse().ok()?;
                            Some(channel_number)
                        }
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

    let channels = match read_channels(&config) {
        Ok(channels) => channels,
        Err(e) => {
            eprintln!("Failed to read channels: {}", e);
            process::exit(1);
        }
    };

    let html = match generate_channel_overview_html(&config, &channels) {
        Ok(html) => html,
        Err(e) => {
            eprintln!("Failed to render channel overview template: {}", e);
            process::exit(1);
        }
    };

    let state = Arc::new(State {
        channel_overview_html: Arc::new(RwLock::new(html)),
    });

    let port = config.port;

    tokio::spawn(rerender_loop(config, channels, state.clone()));

    let app = Router::new()
        .route("/", get(channel_overview))
        .nest(
            "/static",
            get_service(ServeDir::new(STATIC_ROOT)).handle_error(handle_error),
        )
        .layer(Extension(state));

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    debug!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn handle_error(err: io::Error) -> impl IntoResponse {
    error!("static serve dir error: {}", err);
    (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong...")
}

async fn channel_overview(Extension(state): Extension<Arc<State>>) -> impl IntoResponse {
    let html = state.channel_overview_html.read().await.clone();
    Html(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_test_config() -> Config {
        Config {
            neopult_home: "irrelevant".to_string(),
            port: 4199,
            show_hidden_channels: false,
            neopult_url_template: "https://neopult.my-domain.com/{{CHANNEL}}".to_string(),
            novnc_base_url: "https://my-domain.com".to_string(),
            rerender_interval_ms: Duration::from_millis(30000),
            websockify_base_path: None,
            websockify_port: None,
            websockify_host: None,
        }
    }

    #[test]
    fn test_generate_channel_overview_html() {
        let config = default_test_config();
        let channels = [1, 3, 4];
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(html.contains("Channel 1"));
        assert!(!html.contains("Channel 2"));
        assert!(html.contains("Channel 3"));
        assert!(html.contains("Channel 4"));
        assert!(html.contains(r#"href="https://my-domain.com?"#));
    }

    #[test]
    fn test_neopult_url_template_flag() {
        let channels = [6, 22, 37];

        let args = Args::parse_from([
            "neopult-lighthouse",
            "--neopult-url-template",
            "https://neopult.my-domain.com",
        ]);
        let config = Config::from(args);
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(html.contains(r#"href="https://neopult.my-domain.com""#));

        let args = Args::parse_from([
            "neopult-lighthouse",
            "--neopult-url-template",
            "https://neopult.my-domain.com:{{PORT}}",
        ]);
        let config = Config::from(args);
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(html.contains(r#"href="https://neopult.my-domain.com:4206""#));
        assert!(html.contains(r#"href="https://neopult.my-domain.com:4222""#));
        assert!(html.contains(r#"href="https://neopult.my-domain.com:4237""#));

        let args = Args::parse_from([
            "neopult-lighthouse",
            "--neopult-url-template",
            "https://my-domain.com/neopult/{{CHANNEL}}/admin",
        ]);
        let config = Config::from(args);
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(html.contains(r#"href="https://my-domain.com/neopult/6/admin""#));
        assert!(html.contains(r#"href="https://my-domain.com/neopult/22/admin""#));
        assert!(html.contains(r#"href="https://my-domain.com/neopult/37/admin""#));
    }

    #[test]
    fn test_websockify_host_flag() {
        let channels = [3, 7, 8, 12];

        let args = Args::parse_from(["neopult-lighthouse"]);
        let config = Config::from(args);
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(!html.contains("&amp;host="));

        let args = Args::parse_from(["neopult-lighthouse", "--websockify-host", "my-domain.com"]);
        let config = Config::from(args);
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(html.contains("&amp;host=my-domain.com"));
    }

    #[test]
    fn test_websockify_base_path_flag() {
        let channels = [5, 18, 37];

        let args = Args::parse_from(["neopult-lighthouse"]);
        let config = Config::from(args);
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(!html.contains("&amp;path="));

        let args = Args::parse_from(["neopult-lighthouse", "--websockify-base-path", "/channel/"]);
        let config = Config::from(args);
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(html.contains("&amp;path=/channel/5"));
        assert!(html.contains("&amp;path=/channel/18"));
        assert!(html.contains("&amp;path=/channel/37"));
    }

    #[test]
    fn test_websockify_port_flag() {
        let channels = [5, 8, 13];

        let args = Args::parse_from(["neopult-lighthouse"]);
        let config = Config::from(args);
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(html.contains("&amp;port=6085"));
        assert!(html.contains("&amp;port=6088"));
        assert!(html.contains("&amp;port=6093"));

        let args = Args::parse_from(["neopult-lighthouse", "--websockify-port", "443"]);
        let config = Config::from(args);
        let html = generate_channel_overview_html(&config, &channels).unwrap();
        assert!(html.contains("&amp;port=443"));
        assert!(!html.contains("&amp;port=6085"));
        assert!(!html.contains("&amp;port=6088"));
        assert!(!html.contains("&amp;port=6093"));
    }
}
