use anyhow::{Context, Result};
use futures::StreamExt;
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;

///! Implementation of power information using events from udev and the
///! power_supply sysfs
///! https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-class-power
///! https://www.kernel.org/doc/Documentation/power/power_supply_class.rst

/// status of an ac adapter
struct Ac {
    /// is the ac adapter providing power
    status: bool,
}

/// status of a battery
struct Battery {
    /// capacity the batter has in Wh
    capacity: f32,
    /// current charge of the battery from 0 to 1
    charge: f32,
    /// status of the battery
    status: BatteryStatus,
}

/// different statuses a battery can have
enum BatteryStatus {
    Unknown,
    Charging,
    Discharging,
    NotCharging,
    Full,
}

pub async fn init(whitelist: Option<Vec<String>>) -> Result<()> {
    let devices = fs::read_dir("/sys/class/power_supply")
        .await
        .context("`power_supply` sysfs is required for battery information")?;

    let devices = ReadDirStream::new(devices)
        .filter_map(async |result| result.ok().and_then(|a| a.file_name().to_str()))
        .filter(async |name| {
            name == &"AC"
                || (name.starts_with("BAT")
                    && whitelist
                        .as_ref()
                        .map(|allowed| allowed.iter().any(|w| w == name))
                        .unwrap_or(true))
        })
        .collect::<Vec<_>>()
        .await;

    Ok(())
}
