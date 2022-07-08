use log::{debug, error};
use std::{env, path::PathBuf};

#[derive(Debug)]
pub struct Config {
    pub channel: u8,
    pub home: PathBuf,
}

const CHANNEL_ENV_KEY: &str = "NEOPULT_CHANNEL";
const CHANNEL_DEFAULT: u8 = 0;
const CHANNEL_MAX: u8 = 99;
// In debug mode we do not want to overwrite HOME or cargo won't work. In production, neopult will
// run under its own user so it is fine to inherit the HOME.
const HOME_ENV_KEY: &str = if cfg!(debug_assertions) { "NEOPULT_HOME" } else { "HOME" };

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

    debug!("expecting home in environment variable {}", HOME_ENV_KEY);
    let home = match env::var(HOME_ENV_KEY) {
        Ok(home) => PathBuf::from(home),
        Err(_) => {
            anyhow::bail!("the {} environment variable has to be set", HOME_ENV_KEY);
        }
    };
    debug!("using home {:?}", home);
    if !home.exists() {
        anyhow::bail!("home directory does not exist");
    }

    let config = Config { channel, home };
    Ok(config)
}
