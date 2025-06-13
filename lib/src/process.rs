use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use futures::StreamExt;
use log::{trace, warn};
use nix::{sys::signal::kill, unistd::Pid};
use tokio::{fs, time::Instant};
use tokio_stream::wrappers::ReadDirStream;

use crate::{StaticStream, StreamContext};

pub use nix::sys::signal::Signal as ProcessSignal;

/// information about one process
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// process id of the process
    pub pid: u64,
    /// name of the process (_not_ the executable)
    pub name: String,
    /// command line of the process, space separated
    pub cmdline: String,
}

/// reads all running processes from the procfs
pub async fn read_running_processes() -> Result<Vec<ProcessInfo>> {
    let devices = fs::read_dir("/proc").await.context("cannot access procfs, are you on linux?")?;

    Ok(ReadDirStream::new(devices)
        .filter_map(async |result| result.ok())
        .filter_map(async |f| f.file_name().into_string().ok().and_then(|s| s.parse::<u64>().ok()))
        .filter_map(async |pid| {
            let dir = PathBuf::from("/proc").join(pid.to_string());

            let Ok(name) = fs::read_to_string(dir.join("comm")).await else {
                warn!("failed to read `comm` attribute for process `{pid}`");
                return None;
            };

            let Ok(cmdline) = fs::read_to_string(dir.join("cmdline")).await else {
                warn!("failed to read `cmdline` attribute for process `{pid}`");
                return None;
            };

            Some(ProcessInfo {
                pid,
                name: name.trim().to_owned(),
                cmdline: cmdline.replace('\0', " ").trim().to_owned(),
            })
        })
        .collect()
        .await)
}

/// creates a stream which polls for actively running processes at the given
/// interval
pub fn listen_running_processes(polling: Duration) -> StaticStream<Vec<ProcessInfo>> {
    let mut interval = tokio::time::interval_at(Instant::now(), polling);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    futures::stream::unfold(interval, async |mut interval| {
        interval.tick().await;

        trace!("polling running process information");
        let Some(processes) = read_running_processes().await.stream_log("running processes stream")
        else {
            return None;
        };

        Some((processes, interval))
    })
    .boxed()
}

/// sends a signal to a process
pub fn send_signal(pid: u64, signal: ProcessSignal) -> Result<()> {
    kill(Pid::from_raw(pid as i32), signal)
        .with_context(|| format!("failed to send signal `{signal}` to process `{pid}`"))
}
