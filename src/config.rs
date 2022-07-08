use log::{debug, error};
use std::{env, path::PathBuf};

#[derive(Debug)]
pub struct Config {
    pub channel: u8,
    pub neopult_home: PathBuf,
    pub default_channel_home: PathBuf,
    /// Will be the specific channel home (if it exists) or else the default channel home.
    pub channel_home: PathBuf,
}

pub const GLOBAL_CONFIG_DIR: &str = "/etc/neopult";

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

pub fn get_config() -> anyhow::Result<Config> {
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

    let default_channel_home = neopult_home.join("channel-default");
    debug!("using default channel home {:?}", default_channel_home);
    if !default_channel_home.exists() {
        anyhow::bail!("default channel home directory does not exist");
    }

    let specific_channel_home = neopult_home.join(format!("channel-{}", channel));
    debug!("using specific channel home {:?}", specific_channel_home);
    let channel_home = if specific_channel_home.exists() {
        debug!("specific channel home directory exists");
        specific_channel_home
    } else {
        debug!("specific channel home directory does not exist -- falling back to default channel home");
        default_channel_home.clone()
    };

    let config = Config {
        channel,
        neopult_home,
        default_channel_home,
        channel_home,
    };
    Ok(config)
}
