use clap::Subcommand;
use serde_json::json;
use std::io::{self, Write};

use super::{context, write_json};
use crate::tuxedo::fan::{self, policy};

#[derive(clap::Args)]
#[command(
    after_help = "Examples:\n  sudo x-laptune fan\n  sudo x-laptune fan auto\n  sudo x-laptune fan full\n  x-laptune fan policy list\n  x-laptune fan policy show baseline\n  sudo x-laptune fan policy apply baseline\n  sudo x-laptune fan custom\n  sudo x-laptune fan custom --reset-config"
)]
pub(super) struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Use the firmware's built-in curves, preserving shared EC control
    Auto,
    /// Request full-speed fan mode
    Full,
    /// Read and apply /etc/x-laptune/fan-policy.json
    Custom {
        /// Overwrite the config with its initial baseline and exit without applying it
        #[arg(long)]
        reset_config: bool,
    },
    /// Query the custom curve, list policies, or apply a policy
    Policy {
        #[command(subcommand)]
        command: Option<PolicyCommand>,
    },
}

#[derive(Subcommand)]
enum PolicyCommand {
    /// List built-in policies and the editable system policy
    List,
    /// Read a policy definition without accessing hardware
    Show {
        #[arg(value_name = "NAME_OR_FILE")]
        policy: String,
    },
    /// Apply a built-in policy or JSON file; the EC continues regulating the fans
    Apply {
        #[arg(value_name = "NAME_OR_FILE")]
        policy: String,
    },
}

pub(super) fn run(output: &mut impl Write, args: Args, as_json: bool) -> io::Result<()> {
    match args.command {
        Some(Command::Auto) => context("Failed to change the fan mode", fan::set_auto_mode())?,
        Some(Command::Full) => context("Failed to change the fan mode", fan::set_full_mode())?,
        Some(Command::Custom { reset_config }) => {
            if reset_config {
                context(
                    "Failed to reset the custom fan configuration",
                    policy::reset_custom_config(),
                )?;
                return if as_json {
                    write_json(
                        output,
                        json!({ "config": policy::CUSTOM_POLICY_PATH, "reset": true }),
                    )
                } else {
                    writeln!(
                        output,
                        "Reset custom fan configuration: {}",
                        policy::CUSTOM_POLICY_PATH
                    )
                };
            }
            return run_policy(
                output,
                Some(PolicyCommand::Apply {
                    policy: "custom".into(),
                }),
                as_json,
            );
        }
        Some(Command::Policy { command }) => return run_policy(output, command, as_json),
        None => {}
    }
    print_status(output, as_json)
}

fn print_status(output: &mut impl Write, as_json: bool) -> io::Result<()> {
    let (state, current) = context(
        "Failed to read the fan control state and policy",
        fan::read_control_state_and_policy(),
    )?;
    let speeds = context("Failed to read fan speeds", fan::read_fan_speeds())?;
    let name = match current.as_ref() {
        Some(current) => policy::builtin_name(current).unwrap_or("custom"),
        None => "firmware",
    };
    if as_json {
        write_json(
            output,
            json!({
                "automatic": state.automatic,
                "full_speed": state.full_speed,
                "manual_control": state.manual_control,
                "custom_curve": state.custom_curve,
                "separate_fans": state.separate_fans,
                "policy": name,
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
            ("Custom curve", state.custom_curve),
            ("Separate fans", state.separate_fans),
        ] {
            writeln!(
                output,
                "{label:<18} : {}",
                if enabled { "on" } else { "off" }
            )?;
        }
        writeln!(output, "{:<18} : {name}", "Curve policy")?;
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
                    .unwrap_or_else(|| "--".into()),
            )?;
        }
        writeln!(
            output,
            "-- means unknown. Fan numbers are not mapped to left/right positions."
        )
    }
}

fn run_policy(
    output: &mut impl Write,
    command: Option<PolicyCommand>,
    as_json: bool,
) -> io::Result<()> {
    match command {
        Some(PolicyCommand::List) => {
            let names: Vec<_> = policy::BUILTIN_POLICIES
                .iter()
                .map(|(name, _)| *name)
                .chain(["custom"])
                .collect();
            if as_json {
                write_json(output, json!({ "policies": names }))
            } else {
                writeln!(output, "POLICY")?;
                for name in names {
                    writeln!(output, "{name}")?;
                }
                Ok(())
            }
        }
        Some(PolicyCommand::Show { policy: name }) => {
            let definition = context("Failed to load the fan policy", policy::load(&name))?;
            print_policy(output, &name, &definition, as_json)
        }
        Some(PolicyCommand::Apply { policy: name }) => {
            let definition = context("Failed to load the fan policy", policy::load(&name))?;
            context("Failed to apply the fan policy", policy::apply(&definition))?;
            print_status(output, as_json)
        }
        None => {
            let (_, current) = context(
                "Failed to read the fan control state and policy",
                fan::read_control_state_and_policy(),
            )?;
            let Some(current) = current else {
                return if as_json {
                    write_json(
                        output,
                        json!({ "policy": "firmware", "custom_curve": false }),
                    )
                } else {
                    writeln!(
                        output,
                        "Curve policy       : firmware\nNo custom fan table is enabled."
                    )
                };
            };
            print_policy(
                output,
                policy::builtin_name(&current).unwrap_or("custom"),
                &current,
                as_json,
            )
        }
    }
}

fn print_policy(
    output: &mut impl Write,
    name: &str,
    definition: &policy::Policy,
    as_json: bool,
) -> io::Result<()> {
    if as_json {
        // Keep definitions directly loadable as a policy JSON file.
        return write_json(output, json!(definition));
    }
    writeln!(output, "Curve policy       : {name}")?;
    for (sensor, curve) in [("CPU", &definition.cpu), ("GPU", &definition.gpu)] {
        writeln!(output, "\n{sensor} curve")?;
        writeln!(
            output,
            "{:>5}  {:>14}  {:>14}  {:>7}",
            "STAGE", "RISE ABOVE (C)", "FALL BELOW (C)", "PWM (%)"
        )?;
        for stage in 0..16 {
            writeln!(
                output,
                "{stage:>5}  {:>14}  {:>14}  {:>7.1}",
                curve.rise_above_c[stage], curve.fall_below_c[stage], curve.pwm_percent[stage],
            )?;
        }
    }
    Ok(())
}
