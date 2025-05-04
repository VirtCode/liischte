use anyhow::{Context, Result};
use futures::StreamExt;
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;

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
    Ok(())
}
