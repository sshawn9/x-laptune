use serde_json::json;
use std::{
    io::{self, Write},
    num::{NonZeroU64, NonZeroUsize},
    thread,
    time::Duration,
};

use super::{context, write_json};
use crate::memory_overheat::temperature;

#[derive(clap::Args)]
#[command(
    after_help = "Examples:\n  x-laptune memory-temp\n  x-laptune memory-temp --watch --interval-ms 1000\n  x-laptune memory-temp --watch --count 5 --json"
)]
pub(super) struct Args {
    /// Read continuously; press Ctrl+C to stop
    #[arg(long)]
    watch: bool,
    /// Sampling interval in milliseconds
    #[arg(long, default_value = "100", value_name = "MS")]
    interval_ms: NonZeroU64,
    /// Exit after this many samples in watch mode
    #[arg(long, requires = "watch", value_name = "N")]
    count: Option<NonZeroUsize>,
}

pub(super) fn run(output: &mut impl Write, args: Args, as_json: bool) -> io::Result<()> {
    let Args {
        watch,
        interval_ms,
        count,
    } = args;
    let sensors = context(
        "Failed to find memory temperature sensors",
        temperature::discover_sensors(),
    )?;
    let mut samples = 0;
    loop {
        let readings = temperature::read_temperatures(&sensors);
        if as_json {
            write_json(output, json!({ "temperatures_millicelsius": readings }))?;
        } else {
            let values: Vec<_> = readings
                .iter()
                .map(|(sensor, value)| {
                    let value = value
                        .map(|value| format!("{:.2} °C", f64::from(value) / 1000.0))
                        .unwrap_or_else(|| "-- °C".into());
                    format!("{sensor}: {value}")
                })
                .collect();
            writeln!(output, "{}", values.join(" | "))?;
        }
        output.flush()?;
        samples += 1;
        if !watch || count.is_some_and(|count| samples >= count.get()) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(interval_ms.get()));
    }
}
