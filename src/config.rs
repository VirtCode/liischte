use std::{
    collections::HashMap,
    env,
    fs::{self},
    path::PathBuf,
    process::exit,
    sync::LazyLock,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use iced::{Color, color};
use log::{debug, error, info};
use lucide_icons::Icon;
use serde::{Deserialize, Deserializer};
use toml::Table;

use crate::{
    module::{
        audio::AUDIO_MODULE_IDENTIFIER, network::NETWORK_MODULE_IDENTIFIER,
        power::POWER_MODULE_IDENTIFIER,
    },
    ui::window::WindowLayer,
};

/// path where the config is read from
fn config_path() -> Result<PathBuf> {
    if let Ok(path) = env::var("LIISCHTE_CONFIG") {
        Ok(PathBuf::from(path))
    } else if let Ok(config) = env::var("XDG_CONFIG_HOME") {
        Ok(PathBuf::from(config).join("liischte.toml"))
    } else if let Ok(config) = env::var("HOME") {
        Ok(PathBuf::from(config).join(".config/liischte.toml"))
    } else {
        Err(anyhow!("$LIISCHTE_CONFIG, $XDG_CONFIG_HOME and $HOME are all not defined"))
    }
}

/// deserializes a color from a toml string
pub fn deserialize_color<'de, D>(deserializer: D) -> Result<Color, D::Error>
where
    D: Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;

    Color::parse(&string)
        .ok_or(serde::de::Error::unknown_variant(&string, &["#RRGGBB", "#RRGGBBAA"]))
}

/// deserializes an icon from a toml string
pub fn deserialize_icon<'de, D>(deserializer: D) -> Result<Icon, D::Error>
where
    D: Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;

    Icon::from_name(&string).ok_or(serde::de::Error::custom("not a valid lucide icon name"))
}

/// deserializes a duration from a toml integer as seconds
pub fn deserialize_duration_seconds<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    u64::deserialize(deserializer).map(Duration::from_secs)
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
    /// layer namespace to use (with `-osd` for the osd)
    pub namespace: String,
    /// layer to show bar on
    pub layer: WindowLayer,
    /// whether to show the bar on the left instead of the right
    pub right: bool,
    /// output to show the bar on (name, or description with a `desc:` prefix)
    /// `active` for the active monitor
    pub output: String,
    /// whether the ipc socket is enabled
    pub ipc: bool,

    /// looks of the bar
    pub looks: ConfigLooks,

    /// parameters for the osd
    pub osd: ConfigOsd,

    /// config for the main widgets
    pub hyprland: ConfigHyprland,
    pub clock: ConfigClock,

    /// which modules are enabled
    pub modules: Vec<String>,

    /// config for modules
    module: HashMap<String, Table>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            namespace: "liischte".to_string(),
            layer: WindowLayer::Top,
            right: false,
            output: "active".to_string(),
            ipc: true,
            looks: ConfigLooks::default(),
            osd: ConfigOsd::default(),
            hyprland: ConfigHyprland::default(),
            clock: ConfigClock::default(),
            modules: vec![
                POWER_MODULE_IDENTIFIER.to_string(),
                AUDIO_MODULE_IDENTIFIER.to_string(),
                NETWORK_MODULE_IDENTIFIER.to_string(),
            ],
            module: HashMap::default(),
        }
    }
}

impl Config {
    /// reads the config from the file system
    pub fn read() -> Result<Option<Self>> {
        let path = config_path()?;

        if !path.exists() {
            return Ok(None);
        }

        info!("reading config file from `{}`", path.to_string_lossy());

        Ok(Some(
            toml::from_str(&fs::read_to_string(path).context("failed to read config file")?)
                .context("cannot deserialize config file")?,
        ))
    }

    pub fn module<'de, T>(&self, name: &str) -> T
    where
        T: Deserialize<'de> + Default,
    {
        if let Some(config) = self.module.get(name) {
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
    /// main background color for opaque objects (like osd)
    #[serde(deserialize_with = "deserialize_color")]
    pub background: Color,
    /// border for opaque objects
    #[serde(deserialize_with = "deserialize_color")]
    pub border: Color,

    /// opacity of the background in two-tone icons
    pub tone_opacity: f32,

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
            background: color!(0x000000, 0.6),
            border: color!(0x555555),
            tone_opacity: 0.25,
            padding: 10,
            width: 40,
            font: "JetBrains Mono".to_string(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct ConfigOsd {
    /// is the osd enabled
    pub enabled: bool,

    /// layer the osd is rendered on
    pub layer: WindowLayer,

    /// how long to show the osd for an event in millis
    pub timeout: u64,

    /// time the osd hides when respawning in millis
    /// this is used such that the compositor has time to show an animation
    pub respawn_time: u64,
}

impl Default for ConfigOsd {
    fn default() -> Self {
        Self { enabled: true, layer: WindowLayer::Overlay, timeout: 4000, respawn_time: 200 }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct ConfigHyprland {
    /// enable hyprland workspace indicator
    pub enabled: bool,

    /// id of the monitor to show workspaces for
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
    /// whether to show the seconds indicator
    /// (minutes might be inaccurate if disabled)
    pub seconds: bool,
}

impl Default for ConfigClock {
    fn default() -> Self {
        Self { seconds: true }
    }
}
