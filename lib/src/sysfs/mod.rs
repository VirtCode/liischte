use std::path::PathBuf;

use anyhow::{Context, Result};
use futures::StreamExt;
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;

/// implementation of power information using events from udev and the
/// power_supply sysfs
/// https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-class-power
/// https://www.kernel.org/doc/Documentation/power/power_supply_class.rst
#[cfg(feature = "power")]
pub mod power;

/// implementation of backlight information using events from udev and the
/// backlight sysfs
/// https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-class-backlight
#[cfg(feature = "backlight")]
pub mod backlight;

/// represents a device in the sysfs
#[derive(Clone)]
pub struct Device {
    path: PathBuf,

    /// name of the device (the directory name)
    pub name: String,
}

impl Device {
    /// list all devices available in a given sysfs class
    async fn read_devices(class: &str) -> Result<Vec<Self>> {
        let devices = fs::read_dir(PathBuf::from("/sys/class").join(class))
            .await
            .context("`backlight` sysfs is required for backlight information")?;

        Ok(ReadDirStream::new(devices)
            .filter_map(async |result| result.ok())
            .filter_map(async |entry| {
                let path = entry.path();

                let Some(name) = path.file_name() else {
                    return None;
                };

                Some(Self { name: name.to_string_lossy().to_string(), path })
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
