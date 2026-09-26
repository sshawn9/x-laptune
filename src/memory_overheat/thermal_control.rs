use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use super::{intel_powerclamp, temperature};

pub fn run_control_loop(interval: Duration) -> io::Result<()> {
    let sensors = temperature::discover_sensors()?;
    let device = intel_powerclamp::find_cooling_device()?;
    let stopped = Arc::new(AtomicBool::new(false));
    let signal_stopped = Arc::clone(&stopped);
    let control_thread = thread::current();
    ctrlc::set_handler(move || {
        signal_stopped.store(true, Ordering::Relaxed);
        control_thread.unpark();
    })
    .map_err(io::Error::other)?;

    let result = (|| {
        let mut last_target = None;
        while !stopped.load(Ordering::Relaxed) {
            last_target = Some(update_throttling(&sensors, &device, last_target)?);
            thread::park_timeout(interval);
        }
        Ok(())
    })();

    let reset = intel_powerclamp::set_target_percent(&device, 0)
        .map(|_| ())
        .inspect_err(|error| {
            let _ = writeln!(io::stderr(), "Failed to disable CPU throttling: {error}");
        });
    result.and(reset)
}

fn update_throttling(
    sensors: &[PathBuf],
    device: &Path,
    last_target: Option<u8>,
) -> io::Result<u8> {
    let highest = temperature::read_temperatures(sensors)
        .into_values()
        .flatten()
        .max();
    let target = if highest.is_some_and(|temperature| temperature >= 90_000) {
        75
    } else {
        0
    };
    if Some(target) != last_target {
        intel_powerclamp::set_target_percent(device, target)?;
    }
    Ok(target)
}
