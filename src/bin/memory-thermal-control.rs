#![forbid(unsafe_code)]

use clap::Parser;
use std::{io, num::NonZeroU64, process::ExitCode, time::Duration};

#[derive(Parser)]
#[command(
    version,
    about = "Control CPU throttling on all cores based on memory temperature"
)]
struct Args {
    /// Sampling interval in milliseconds
    #[arg(long, default_value = "2000")]
    interval_ms: NonZeroU64,
}

fn main() -> ExitCode {
    match x_laptune::memory_overheat::thermal_control::run_control_loop(Duration::from_millis(
        Args::parse().interval_ms.get(),
    )) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("memory-thermal-control: {error}");
            ExitCode::FAILURE
        }
    }
}
