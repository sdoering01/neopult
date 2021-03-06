use log::{debug, error};
use std::{env, path::PathBuf};

pub const GLOBAL_DATA_DIR: &str = "/usr/local/share/neopult";

pub const WEB_ROOT: &str = if cfg!(debug_assertions) {
    "neopult/web"
} else {
    "/usr/local/share/neopult/web"
};

const CHANNEL_ENV_KEY: &str = "NEOPULT_CHANNEL";
const CHANNEL_DEFAULT: u8 = 0;
const CHANNEL_MAX: u8 = 99;
// In debug mode we do not want to overwrite HOME or cargo won't work. In production, neopult will
// run under its own user so it is fine to inherit the HOME.
const NEOPULT_HOME_ENV_KEY: &str = if cfg!(debug_assertions) {
    "NEOPULT_HOME"
} else {
    "HOME"
};

#[derive(Debug)]
pub struct EnvConfig {
    pub channel: u8,
    pub neopult_home: PathBuf,
    pub channel_home: PathBuf,
}

#[derive(Debug)]
pub struct Config {
    pub channel: u8,
    pub neopult_home: PathBuf,
    pub channel_home: PathBuf,
    pub websocket_password: String,
}

pub fn get_env_config() -> anyhow::Result<EnvConfig> {
    let channel_option = match env::var(CHANNEL_ENV_KEY) {
        Ok(channel_str) => {
            debug!(
                "got {} environment variable with value {}",
                CHANNEL_ENV_KEY, channel_str
            );
            match channel_str.parse() {
                Ok(channel) => {
                    if channel <= CHANNEL_MAX {
                        Some(channel)
                    } else {
                        error!("channel must be at most {}  -- using default", CHANNEL_MAX);
                        None
                    }
                }
                Err(e) => {
                    error!("could not parse channel: {} -- using default", e);
                    None
                }
            }
        }
        Err(_) => None,
    };
    let channel = channel_option.unwrap_or(CHANNEL_DEFAULT);
    debug!("using channel {}", channel);

    debug!(
        "expecting neopult home in environment variable {}",
        NEOPULT_HOME_ENV_KEY
    );
    let neopult_home = match env::var(NEOPULT_HOME_ENV_KEY) {
        Ok(home) => PathBuf::from(home),
        Err(_) => {
            anyhow::bail!(
                "the {} environment variable has to be set to the neopult home",
                NEOPULT_HOME_ENV_KEY
            );
        }
    };
    debug!("using neopult home {:?}", neopult_home);
    if !neopult_home.exists() {
        anyhow::bail!("neopult home directory does not exist");
    }

    // Eliminate discrepancies between debug and release build
    if cfg!(debug_assertions) {
        env::set_var("HOME", &neopult_home);
    }

    let channel_home = neopult_home.join(format!("channel-{}", channel));
    debug!("using channel home {:?}", channel_home);
    if !channel_home.exists() {
        anyhow::bail!("channel home directory does not exist");
    }

    let config = EnvConfig {
        channel,
        neopult_home,
        channel_home,
    };
    Ok(config)
}
