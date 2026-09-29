use clap::ValueEnum;
use serde_json::json;
use std::io::{self, Write};

use super::{context, write_json};
use crate::tuxedo::fan;

#[derive(clap::Args)]
#[command(
    after_help = "Examples:\n  sudo x-laptune fan\n  sudo x-laptune fan auto\n  sudo x-laptune fan full"
)]
pub(super) struct Args {
    #[arg(value_enum, value_name = "MODE")]
    mode: Option<FanMode>,
}

#[derive(Clone, Copy, ValueEnum)]
enum FanMode {
    /// Restore automatic fan control
    Auto,
    /// Request full-speed fan mode
    Full,
}

pub(super) fn run(output: &mut impl Write, args: Args, as_json: bool) -> io::Result<()> {
    let Args { mode } = args;
    if let Some(mode) = mode {
        context(
            "Failed to change the fan mode",
            match mode {
                FanMode::Auto => fan::set_auto_mode(),
                FanMode::Full => fan::set_full_mode(),
            },
        )?;
    }
    let state = context(
        "Failed to read the fan control state",
        fan::read_control_state(),
    )?;
    let speeds = context("Failed to read fan speeds", fan::read_fan_speeds())?;
    if as_json {
        write_json(
            output,
            json!({
                "automatic": state.automatic,
                "full_speed": state.full_speed,
                "manual_control": state.manual_control,
                "fans": speeds.iter().enumerate().map(|(index, speed)| json!({
                    "fan": index + 1,
                    "current_rpm": speed.current_rpm,
                    "min_rpm": speed.min_rpm,
                    "max_rpm": speed.max_rpm,
                })).collect::<Vec<_>>(),
            }),
        )
    } else {
        for (label, enabled) in [
            ("Automatic control", state.automatic),
            ("Full-speed mode", state.full_speed),
            ("Manual control", state.manual_control),
        ] {
            writeln!(
                output,
                "{label:<18} : {}",
                if enabled { "on" } else { "off" }
            )?;
        }
        writeln!(
            output,
            "\n{:>3}  {:>8}  {:>8}  {:>8}",
            "FAN", "RPM", "MIN RPM", "MAX RPM"
        )?;
        for (index, speed) in speeds.iter().enumerate() {
            writeln!(
                output,
                "{:>3}  {:>8}  {:>8}  {:>8}",
                index + 1,
                speed.current_rpm,
                speed
                    .min_rpm
                    .map(|rpm| rpm.to_string())
                    .unwrap_or_else(|| "--".into()),
                speed
                    .max_rpm
                    .map(|rpm| rpm.to_string())
                    .unwrap_or_else(|| "--".into())
            )?;
        }
        writeln!(
            output,
            "-- means unknown. Fan numbers are not mapped to left/right positions."
        )
    }
}
