use std::{
    collections::HashMap,
    env,
    fs::{self},
    path::PathBuf,
    process::exit,
    sync::LazyLock,
};

use anyhow::{Context, Result};
use iced::{Color, color};
use log::{debug, error, info};
use serde::{Deserialize, Deserializer};
use toml::Table;

use crate::status::{
    audio::AUDIO_STATUS_IDENTIFIER, network::NETWORK_STATUS_IDENTIFIER,
    power::POWER_STATUS_IDENTIFIER,
};

/// deserializes a color from a toml string
fn deserialize_color<'de, D>(deserializer: D) -> Result<Color, D::Error>
where
    D: Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;

    Color::parse(&string)
        .ok_or(serde::de::Error::unknown_variant(&string, &["#RRGGBB", "#RRGGBBAA"]))
}

pub static CONFIG: LazyLock<Config> = LazyLock::new(|| {
    debug!("starting configuration read");

    match Config::read() {
        Ok(Some(config)) => config,
        Ok(None) => Config::default(),
        Err(e) => {
            error!("{e:?}");
            exit(1);
        }
    }
});

#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    /// layer namespace to use
    pub namespace: String,
    /// whether to show the bar on the left instead of the right
    pub right: bool,

    /// looks of the bar
    pub looks: ConfigLooks,

    /// config for the main widgets
    pub hyprland: ConfigHyprland,
    pub clock: ConfigClock,

    /// which status are enabled
    pub statuses: Vec<String>,

    /// config for statuses
    status: HashMap<String, Table>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            namespace: "liischte".to_string(),
            right: false,
            looks: ConfigLooks::default(),
            hyprland: ConfigHyprland::default(),
            clock: ConfigClock::default(),
            statuses: vec![
                POWER_STATUS_IDENTIFIER.to_string(),
                AUDIO_STATUS_IDENTIFIER.to_string(),
                NETWORK_STATUS_IDENTIFIER.to_string(),
            ],
            status: HashMap::default(),
        }
    }
}

impl Config {
    /// reads the config from the file system
    pub fn read() -> Result<Option<Self>> {
        let path = env::var("XDG_CONFIG_HOME")
            .map(|config| PathBuf::from(config).join("liischte.toml"))
            .or_else(|_| {
                env::var("HOME").map(|home| PathBuf::from(home).join(".config/liischte.toml"))
            })
            .context("$XDG_CONFIG_HOME and $HOME are both not defined, can't read config")?;

        if !path.exists() {
            return Ok(None);
        }

        info!("reading config file from {}", path.to_string_lossy());

        Ok(Some(
            toml::from_str(&fs::read_to_string(path).context("failed to read config file")?)
                .context("cannot deserialize config file")?,
        ))
    }

    pub fn status<'de, T>(&self, name: &str) -> T
    where
        T: Deserialize<'de> + Default,
    {
        if let Some(config) = self.status.get(name) {
            Table::try_into(config.clone())
                .with_context(|| format!("cannot deserialize status config for `{name}`"))
                .map_err(|e| {
                    error!("{e:?}");
                    exit(1)
                })
                .expect("should have exit")
        } else {
            T::default()
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct ConfigLooks {
    /// main foreground color
    #[serde(deserialize_with = "deserialize_color")]
    pub foreground: Color,
    /// semi-transparent color used for separators etc.
    #[serde(deserialize_with = "deserialize_color")]
    pub semi: Color,

    /// font to use for text on the bar
    pub font: String,

    /// padding of the bar to the side
    pub padding: u32,
    /// width of the bar
    pub width: u32,
}

impl Default for ConfigLooks {
    fn default() -> Self {
        Self {
            foreground: color!(0xFFFFFF),
            semi: color!(0xFFFFFF, 0.6),
            padding: 10,
            width: 32,
            font: "JetBrains Mono".to_string(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct ConfigHyprland {
    /// enable hyprland workspace indicator
    pub enabled: bool,

    /// monitor to show workspaces for
    pub monitor: u64,
    /// whether to show fullscreen status in bar
    pub fullscreen: bool,

    /// size of the indicators
    pub size: f32,
    /// thickness of the indicator border
    pub border: f32,
    /// radius of the indicators
    pub rounding: f32,
}

impl Default for ConfigHyprland {
    fn default() -> Self {
        Self {
            enabled: true,
            monitor: 0,
            fullscreen: true,
            size: 17f32,
            border: 1.5f32,
            rounding: 6f32,
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct ConfigClock {
    pub seconds: bool,
}

impl Default for ConfigClock {
    fn default() -> Self {
        Self { seconds: true }
    }
}
