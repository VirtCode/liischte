use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use futures::StreamExt;
use log::trace;
use tokio::{fs, time::Instant};
use tokio_stream::wrappers::ReadDirStream;
use udev::MonitorBuilder;

use crate::{StaticStream, StreamErrorLog};

use super::util::udev::AsyncMonitorSocket;

///! Implementation of power information using events from udev and the
///! power_supply sysfs
///! https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-class-power
///! https://www.kernel.org/doc/Documentation/power/power_supply_class.rst

/// a device in the `power_supply` sysfs
#[derive(Clone)]
pub struct PowerDevice {
    path: PathBuf,
    pub name: String,
}

/// reads all power devices currently available from the sysfs
pub async fn read_devices() -> Result<Vec<PowerDevice>> {
    let devices = fs::read_dir("/sys/class/power_supply")
        .await
        .context("`power_supply` sysfs is required for power information")?;

    Ok(ReadDirStream::new(devices)
        .filter_map(async |result| result.ok())
        .filter_map(async |entry| {
            let path = entry.path();
            let Some(name) = path.file_name() else {
                return None;
            };

            Some(PowerDevice { name: name.to_string_lossy().to_string(), path })
        })
        .collect::<Vec<_>>()
        .await)
}

/// reads a sysfs device attribute
async fn read_device_attribute(device: &PowerDevice, attribute: &str) -> Result<i64> {
    fs::read_to_string(device.path.join(attribute))
        .await
        .with_context(|| format!("failed to read `{attribute}` file of device `{}`", device.name))
        .and_then(|s| {
            s.trim().parse::<i64>().with_context(|| {
                format!("could not parse `{attribute}` for device `{}`", device.name)
            })
        })
}

/// reads the online state from an ac adapter given as a power device
pub async fn read_ac_online(ac: &PowerDevice) -> Result<bool> {
    read_device_attribute(ac, "online").await.map(|v| v == 1)
}

/// reads the capacity in Wh, meaning the energy it can store from a battery
/// given as a power device
pub async fn read_battery_capacity(bat: &PowerDevice) -> Result<f64> {
    read_device_attribute(bat, "energy_full").await.map(|energy| energy as f64 / 1e6f64)
}

/// reads the charge as a percentage from a battery given as a power device
pub async fn read_battery_charge(bat: &PowerDevice) -> Result<f64> {
    read_device_attribute(bat, "capacity").await.map(|energy| energy as f64 / 100f64)
}

/// creates a stream which listens to udev events for the given ac adapter
/// device and then reads the online state from the sysfs
pub fn listen_ac_online(ac: PowerDevice) -> Result<StaticStream<bool>> {
    let socket =
        MonitorBuilder::new()?.match_subsystem_devtype("power_supply", "power_supply")?.listen()?;

    // yeah, cooked
    let device = Box::leak(Box::new(ac));

    let stream = AsyncMonitorSocket::new(socket)?
        .filter_map(async |r| {
            if r.context("received invalid udev event")
                .stream_log("ac online stream")?
                .sysname()
                .to_string_lossy()
                == *device.name
            {
                Some(())
            } else {
                None
            }
        })
        .then(async |_| read_ac_online(device).await)
        .filter_map(async |r| r.stream_log("ac online stream"))
        .boxed();

    Ok(stream)
}

/// creates a stream which polls the battery charge which is read now and then
/// from the sysfs
pub fn listen_battery_charge(bat: PowerDevice, polling: Duration) -> StaticStream<f64> {
    let mut interval = tokio::time::interval_at(Instant::now(), polling);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let bat = Box::leak(Box::new(bat));

    futures::stream::unfold((interval, -1f64), async |(mut interval, last)| {
        let mut next = last;

        while next == last {
            interval.tick().await;

            trace!("polling battery charge for device `{}`", bat.name);
            if let Some(charge) = read_battery_charge(bat).await.stream_log("battery charge stream")
            {
                next = charge;
            };
        }

        Some((next, (interval, next)))
    })
    .boxed()
}
