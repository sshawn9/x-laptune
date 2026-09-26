use serde_json::json;
use std::io::{self, Write};

use super::{context, write_json};
use crate::tuxedo::performance as oem;

#[derive(clap::Args)]
#[command(
    after_help = "Examples:\n  sudo x-laptune oem-mode\n  sudo x-laptune oem-mode enthusiast"
)]
pub(super) struct Args {
    #[arg(value_enum, value_name = "MODE")]
    mode: Option<oem::PerformanceMode>,
}

pub(super) fn run(output: &mut impl Write, args: Args, as_json: bool) -> io::Result<()> {
    let Args { mode } = args;
    if let Some(mode) = mode {
        context(
            "Failed to set the OEM performance mode",
            oem::set_mode(mode),
        )?;
    }
    let actual = context("Failed to read the OEM performance mode", oem::read_mode())?;
    if as_json {
        write_json(output, json!({ "mode": actual.to_string() }))
    } else {
        writeln!(output, "OEM performance mode: {actual}")
    }
}
