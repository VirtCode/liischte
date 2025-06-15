use anyhow::Result;
use futures::StreamExt;
use udev::MonitorBuilder;

use crate::{StaticStream, StreamContext, util::udev::AsyncMonitorSocket};

use super::Device;

#[derive(Clone)]
pub struct BacklightDevice {
    pub device: Device,

    /// maximum brightness of the device
    max: u32,
}

impl BacklightDevice {
    /// reads all backlight devices currently available from the sysfs
    pub async fn read_all() -> Result<Vec<Self>> {
        Ok(futures::future::join_all(Device::read_devices("backlight").await?.into_iter().map(
            |this| async {
                if let Ok(max) = this.read_device_attribute_int("max_brightness").await {
                    Some(Self { device: this, max: max as u32 })
                } else {
                    None
                }
            },
        ))
        .await
        .into_iter()
        .filter_map(|o| o)
        .collect())
    }

    /// reads the current brightness from the device
    pub async fn read_brightness(&self) -> Result<f64> {
        self.device
            .read_device_attribute_int("brightness")
            .await
            .map(|b| b as f64 / self.max as f64)
    }

    /// creates a stream which listens to udev events for the given backlight
    /// and then reads the brightness state from the sysfs
    pub fn listen_brightness(self) -> Result<StaticStream<f64>> {
        let socket = MonitorBuilder::new()?.match_subsystem("backlight")?.listen()?;

        let this = Box::leak(Box::new(self));

        const STREAM: &str = "backlight brightness";
        let stream = AsyncMonitorSocket::new(socket)?
            .filter_map(async |r| {
                if r.stream_context(STREAM, "received invalid udev event")?
                    .sysname()
                    .to_string_lossy()
                    == *this.device.name
                {
                    Some(())
                } else {
                    None
                }
            })
            .then(async |_| this.read_brightness().await)
            .filter_map(async |r| r.stream_log(STREAM))
            .boxed();

        Ok(stream)
    }
}
