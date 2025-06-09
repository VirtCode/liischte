use futures::stream::BoxStream;
use log::warn;

/// implementation of hyprland workspace information and basic actions using the
/// hyprland ipc
#[cfg(feature = "hyprland")]
pub mod hyprland;

/// implementation of network connectivity information using the network manager
/// dbus interface
#[cfg(feature = "modemmanager")]
pub mod modemmanager;
/// implementation of modem status information using the modem manager dbus
/// interface
#[cfg(feature = "networkmanager")]
pub mod networkmanager;

/// implementation of pipewire information and basic actions using libpipewire
/// https://docs.pipewire.org/page_api.html
#[cfg(feature = "pipewire")]
pub mod pipewire;

/// implementation of power information using events from udev and the
/// power_supply sysfs
/// https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-class-power
/// https://www.kernel.org/doc/Documentation/power/power_supply_class.rst
#[cfg(feature = "power")]
pub mod power;

mod util;

/// a boxed stream with a static lifetime
pub type StaticStream<T> = BoxStream<'static, T>;

/// an extension trait to log and pretend nothing happend if we encounter errors
/// in a stream
pub trait StreamContext<T, E> {
    fn stream_log(self, name: &str) -> Option<T>;
    fn stream_context(self, stream: &str, context: &str) -> Option<T>;
}

impl<T, E: std::fmt::Display> StreamContext<T, E> for Result<T, E> {
    /// just log as the given stream name
    fn stream_log(self, stream: &str) -> Option<T> {
        match self {
            Ok(r) => Some(r),
            Err(e) => {
                warn!("failure in stream `{stream}`: {e:#}");
                None
            }
        }
    }

    /// just log as the given stream name with some additional context
    fn stream_context(self, stream: &str, context: &str) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(e) => {
                warn!("failure in stream `{stream}`: {context} ({e:#})");
                None
            }
        }
    }
}
