use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use futures::StreamExt;
use log::trace;
use tokio::{fs, time::Instant};
use tokio_stream::wrappers::ReadDirStream;
use udev::MonitorBuilder;

use crate::{StaticStream, StreamContext};

use super::util::udev::AsyncMonitorSocket;

/// a device in the `power_supply` sysfs
#[derive(Clone)]
pub struct PowerDevice {
    path: PathBuf,

    /// the type of the device
    pub kind: PowerDeviceKind,
    /// the name of the device
    pub name: String,
}

/// represents the type of a device read from the sysfs
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PowerDeviceKind {
    /// this device is a mains power supply
    Mains,
    /// this device is a battery
    Battery,
    /// we don't bother with Ups, Wireless and other shit
    Unknown,
}

impl PowerDeviceKind {
    pub fn parse(string: &str) -> Self {
        match string.trim() {
            "Mains" => Self::Mains,
            "Battery" => Self::Battery,
            _ => Self::Unknown,
        }
    }
}

impl PowerDevice {
    /// reads all power devices currently available from the sysfs
    pub async fn read_all() -> Result<Vec<Self>> {
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

                let mut this = PowerDevice {
                    name: name.to_string_lossy().to_string(),
                    kind: PowerDeviceKind::Unknown,
                    path,
                };

                if let Ok(kind) = this.read_device_attribute_string("type").await {
                    this.kind = PowerDeviceKind::parse(&kind)
                }

                Some(this)
            })
            .collect::<Vec<_>>()
            .await)
    }

    /// reads a sysfs device attribute as a string
    async fn read_device_attribute_string(&self, attribute: &str) -> Result<String> {
        fs::read_to_string(self.path.join(attribute))
            .await
            .with_context(|| format!("failed to read `{attribute}` file of device `{}`", self.name))
    }

    /// reads a sysfs device attribute as a an integer
    async fn read_device_attribute_int(&self, attribute: &str) -> Result<i64> {
        self.read_device_attribute_string(attribute).await.and_then(|s| {
            s.trim().parse::<i64>().with_context(|| {
                format!("could not parse `{attribute}` for device `{}`", self.name)
            })
        })
    }
}

/// a device in the `power_supply` sysfs which is a mains power device
/// this should only ever be constructed if `type` is `Mains`
#[derive(Clone)]
pub struct MainsPowerDevice(pub PowerDevice);

impl MainsPowerDevice {
    /// reads the online state
    pub async fn read_online(&self) -> Result<bool> {
        self.0.read_device_attribute_int("online").await.map(|v| v == 1)
    }

    /// creates a stream which listens to udev events for the given ac adapter
    /// device and then reads the online state from the sysfs
    pub fn listen_online(self) -> Result<StaticStream<bool>> {
        let socket = MonitorBuilder::new()?
            .match_subsystem_devtype("power_supply", "power_supply")?
            .listen()?;

        // yeah, cooked
        let this = Box::leak(Box::new(self));

        let stream = AsyncMonitorSocket::new(socket)?
            .filter_map(async |r| {
                if r.context("received invalid udev event")
                    .stream_log("ac online stream")?
                    .sysname()
                    .to_string_lossy()
                    == *this.0.name
                {
                    Some(())
                } else {
                    None
                }
            })
            .then(async |_| this.read_online().await)
            .filter_map(async |r| r.stream_log("ac online stream"))
            .boxed();

        Ok(stream)
    }
}

/// a device in the `power_supply` sysfs which is a battery power device
/// this should only ever be constructed if `type` is `Battery`
#[derive(Clone)]
pub struct BatteryPowerDevice(pub PowerDevice);

impl BatteryPowerDevice {
    /// reads the capacity in Wh, meaning the energy it can store
    pub async fn read_capacity(&self) -> Result<f64> {
        self.0.read_device_attribute_int("energy_full").await.map(|energy| energy as f64 / 1e6f64)
    }

    /// reads the charge as a percentage (0-1)
    pub async fn read_charge(&self) -> Result<f64> {
        self.0.read_device_attribute_int("capacity").await.map(|energy| energy as f64 / 100f64)
    }

    /// creates a stream which polls the battery charge which is read now and
    /// then from the sysfs
    pub fn listen_charge(self, polling: Duration) -> StaticStream<f64> {
        let mut interval = tokio::time::interval_at(Instant::now(), polling);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let bat = Box::leak(Box::new(self));

        futures::stream::unfold((interval, -1f64), async |(mut interval, last)| {
            let mut next = last;

            while next == last {
                interval.tick().await;

                trace!("polling battery charge for device `{}`", bat.0.name);
                if let Some(charge) = bat.read_charge().await.stream_log("battery charge stream") {
                    next = charge;
                };
            }

            Some((next, (interval, next)))
        })
        .boxed()
    }
}
