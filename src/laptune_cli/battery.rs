use serde_json::json;
use std::io::{self, Write};

use super::{context, write_json};
use crate::tuxedo::battery;

#[derive(clap::Args)]
#[command(after_help = "Examples:\n  x-laptune battery\n  sudo x-laptune battery stationary")]
pub(super) struct Args {
    /// Omit to query the current and available profiles, design capacity, and current charge
    #[arg(value_name = "PROFILE", value_parser = ["high_capacity", "balanced", "stationary"])]
    profile: Option<String>,
}

pub(super) fn run(output: &mut impl Write, args: Args, as_json: bool) -> io::Result<()> {
    let Args { profile } = args;
    if let Some(profile) = profile {
        context(
            "Failed to set the charging profile",
            battery::set_profile(&profile),
        )?;
        let actual = context(
            "Failed to read back the charging profile",
            battery::read_profile(),
        )?;
        if actual != profile {
            return Err(io::Error::other(format!(
                "Charging profile read back as {actual} after requesting {profile}"
            )));
        }
        return if as_json {
            write_json(output, json!({ "profile": actual }))
        } else {
            writeln!(output, "{:<18} : {actual}", "Charging profile")
        };
    }

    let profile = battery::read_profile().ok();
    let available = battery::read_available_profiles().ok();
    let (design, charge, percentage) = context(
        "Failed to read battery capacity",
        battery::read_design_and_current_capacity(),
    )?;
    if as_json {
        write_json(
            output,
            json!({
                "profile": profile,
                "available_profiles": available.as_deref().map(|profiles| profiles.split_whitespace().collect::<Vec<_>>()),
                "design_capacity_uah": design,
                "charge_now_uah": charge,
                "charge_percent": percentage,
            }),
        )
    } else {
        writeln!(
            output,
            "{:<18} : {}",
            "Charging profile",
            profile.as_deref().unwrap_or("--")
        )?;
        writeln!(
            output,
            "{:<18} : {}",
            "Available profiles",
            available.as_deref().unwrap_or("--")
        )?;
        writeln!(
            output,
            "{:<18} : {:.3} mAh",
            "Design capacity",
            design as f64 / 1000.0
        )?;
        writeln!(
            output,
            "{:<18} : {:.3} mAh ({percentage}%)",
            "Current charge",
            charge as f64 / 1000.0
        )
    }
}
