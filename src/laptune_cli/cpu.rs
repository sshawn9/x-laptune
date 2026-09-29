use clap::Subcommand;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    num::NonZeroU64,
};

use super::{context, write_json};
use crate::cpu::{
    performance::{self as cpu_performance, CpuPolicy},
    power_limits::{self, RaplPowerLimit},
};

#[derive(clap::Args)]
#[command(
    after_help = "With no subcommand, query frequency ranges, EPP, PL1, and PL2.\n\nExamples:\n  x-laptune cpu\n  x-laptune cpu epp\n  sudo x-laptune cpu max-frequency 3500\n  sudo x-laptune cpu max-frequency --reset\n  sudo x-laptune cpu pl1 90"
)]
pub(super) struct Args {
    #[command(subcommand)]
    command: Option<CpuCommand>,
}

#[derive(Subcommand)]
enum CpuCommand {
    /// Query, set (MHz), or reset the maximum frequency for all CPUs
    #[command(
        after_help = "Examples:\n  x-laptune cpu max-frequency\n  sudo x-laptune cpu max-frequency 3500\n  sudo x-laptune cpu max-frequency --reset"
    )]
    MaxFrequency {
        #[arg(value_name = "MHZ")]
        mhz: Option<NonZeroU64>,
        /// Restore each policy's hardware maximum, not a previous custom limit
        #[arg(long, conflicts_with = "mhz")]
        reset: bool,
    },
    /// Query current and available EPP values, or set EPP for all CPUs
    #[command(
        after_help = "Examples:\n  x-laptune cpu epp\n  sudo x-laptune cpu epp balance_power"
    )]
    Epp {
        /// Run 'cpu epp' without a value to list available preferences
        #[arg(value_name = "PREFERENCE")]
        preference: Option<String>,
    },
    /// Query or set the CPU long-term power limit (W)
    #[command(after_help = "Examples:\n  x-laptune cpu pl1\n  sudo x-laptune cpu pl1 90")]
    Pl1 {
        #[arg(value_name = "WATTS")]
        watts: Option<u64>,
    },
    /// Query or set the CPU short-term power limit (W)
    #[command(after_help = "Examples:\n  x-laptune cpu pl2\n  sudo x-laptune cpu pl2 110")]
    Pl2 {
        #[arg(value_name = "WATTS")]
        watts: Option<u64>,
    },
}

fn policy_json(policy: &CpuPolicy) -> Value {
    json!({
        "policy": policy.name,
        "cpus": policy.cpus,
        "hardware_min_khz": policy.hardware_min_khz,
        "hardware_max_khz": policy.hardware_max_khz,
        "min_frequency_khz": policy.current_min_khz,
        "max_frequency_khz": policy.current_max_khz,
        "epp": policy.energy_performance_preference,
        "available_epp": policy.available_energy_performance_preferences,
    })
}

fn limits_json(limits: &[RaplPowerLimit]) -> Value {
    json!(
        limits
            .iter()
            .map(|limit| json!({
                "package": limit.package,
                "index": limit.index,
                "name": limit.name,
                "limit_microwatts": limit.current_microwatts,
                "reported_max_microwatts": limit.max_microwatts,
                "time_window_microseconds": limit.time_window_microseconds,
            }))
            .collect::<Vec<_>>()
    )
}

fn print_limits(output: &mut impl Write, limits: &[RaplPowerLimit]) -> io::Result<()> {
    writeln!(output, "Power limits")?;
    writeln!(
        output,
        "{:<12}  {:<5}  {:>11}  {:>16}  {:>11}",
        "PACKAGE", "LIMIT", "CURRENT (W)", "REPORTED MAX (W)", "WINDOW (µs)"
    )?;
    for limit in limits {
        let label = match limit.name.as_str() {
            "long_term" => "PL1",
            "short_term" => "PL2",
            name => name,
        };
        let maximum = limit
            .max_microwatts
            .map(|value| format!("{:.3}", value as f64 / 1_000_000.0))
            .unwrap_or_else(|| "--".into());
        let window = limit
            .time_window_microseconds
            .map(|value| value.to_string())
            .unwrap_or_else(|| "--".into());
        writeln!(
            output,
            "{:<12}  {label:<5}  {:>11.3}  {maximum:>16}  {window:>11}",
            limit.package,
            limit.current_microwatts as f64 / 1_000_000.0
        )?;
    }
    Ok(())
}

fn show_power_limit(output: &mut impl Write, name: &str, as_json: bool) -> io::Result<()> {
    let limits: Vec<_> = context(
        "Failed to read CPU power limits",
        power_limits::read_cpu_power_limits(),
    )?
    .into_iter()
    .filter(|limit| limit.name == name)
    .collect();
    if limits.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No CPU {name} power limit found"),
        ));
    }
    if as_json {
        write_json(output, json!({ "power_limits": limits_json(&limits) }))
    } else {
        print_limits(output, &limits)
    }
}

pub(super) fn run(output: &mut impl Write, args: Args, as_json: bool) -> io::Result<()> {
    let Args { command } = args;
    match &command {
        Some(CpuCommand::MaxFrequency { reset: true, .. }) => {
            context(
                "Failed to reset the CPU maximum frequency",
                cpu_performance::reset_max_frequency(),
            )?;
        }
        Some(CpuCommand::MaxFrequency { mhz: Some(mhz), .. }) => {
            context(
                "Failed to set the CPU maximum frequency",
                cpu_performance::set_max_frequency_mhz(mhz.get()),
            )?;
        }
        Some(CpuCommand::Epp {
            preference: Some(preference),
        }) => {
            context(
                "Failed to set CPU EPP",
                cpu_performance::set_energy_performance_preference(preference),
            )?;
        }
        Some(CpuCommand::Pl1 { watts }) => {
            if let Some(watts) = watts {
                context("Failed to set CPU PL1", power_limits::set_pl1_watts(*watts))?;
            }
            return show_power_limit(output, "long_term", as_json);
        }
        Some(CpuCommand::Pl2 { watts }) => {
            if let Some(watts) = watts {
                context("Failed to set CPU PL2", power_limits::set_pl2_watts(*watts))?;
            }
            return show_power_limit(output, "short_term", as_json);
        }
        _ => {}
    }

    let policies = context(
        "Failed to read CPU frequency settings",
        cpu_performance::read_policies(),
    )?;
    match command {
        Some(CpuCommand::MaxFrequency { .. }) => {
            if as_json {
                write_json(
                    output,
                    json!({ "policies": policies.iter().map(|policy| json!({
                    "policy": policy.name,
                    "cpus": policy.cpus,
                    "max_frequency_khz": policy.current_max_khz,
                    "hardware_max_khz": policy.hardware_max_khz,
                })).collect::<Vec<_>>() }),
                )
            } else {
                writeln!(
                    output,
                    "{:<10}  {:<8}  {:>14}  {:>14}",
                    "POLICY", "CPUs", "MAX (MHz)", "HW MAX (MHz)"
                )?;
                for policy in &policies {
                    writeln!(
                        output,
                        "{:<10}  {:<8}  {:>14.3}  {:>14.3}",
                        policy.name,
                        policy.cpus,
                        policy.current_max_khz as f64 / 1000.0,
                        policy.hardware_max_khz as f64 / 1000.0
                    )?;
                }
                Ok(())
            }
        }
        Some(CpuCommand::Epp { .. }) => {
            if as_json {
                write_json(
                    output,
                    json!({ "policies": policies.iter().map(|policy| json!({
                    "policy": policy.name,
                    "cpus": policy.cpus,
                    "epp": policy.energy_performance_preference,
                    "available_epp": policy.available_energy_performance_preferences,
                })).collect::<Vec<_>>() }),
                )
            } else {
                writeln!(
                    output,
                    "{:<10}  {:<8}  {:<20}  AVAILABLE",
                    "POLICY", "CPUs", "EPP"
                )?;
                for policy in &policies {
                    writeln!(
                        output,
                        "{:<10}  {:<8}  {:<20}  {}",
                        policy.name,
                        policy.cpus,
                        policy
                            .energy_performance_preference
                            .as_deref()
                            .unwrap_or("--"),
                        policy.available_energy_performance_preferences.join(" ")
                    )?;
                }
                Ok(())
            }
        }
        None => {
            let limits = context(
                "Failed to read CPU power limits",
                power_limits::read_cpu_power_limits(),
            )?;
            if as_json {
                write_json(
                    output,
                    json!({
                        "policies": policies.iter().map(policy_json).collect::<Vec<_>>(),
                        "power_limits": limits_json(&limits),
                    }),
                )
            } else {
                writeln!(output, "Frequency and EPP")?;
                writeln!(
                    output,
                    "{:<10}  {:<8}  {:>19}  {:>19}  EPP",
                    "POLICY", "CPUs", "HARDWARE (MHz)", "LIMITS (MHz)"
                )?;
                for policy in &policies {
                    let hardware = format!(
                        "{:.3}–{:.3}",
                        policy.hardware_min_khz as f64 / 1000.0,
                        policy.hardware_max_khz as f64 / 1000.0
                    );
                    let range = format!(
                        "{:.3}–{:.3}",
                        policy.current_min_khz as f64 / 1000.0,
                        policy.current_max_khz as f64 / 1000.0
                    );
                    writeln!(
                        output,
                        "{:<10}  {:<8}  {:>19}  {:>19}  {}",
                        policy.name,
                        policy.cpus,
                        hardware,
                        range,
                        policy
                            .energy_performance_preference
                            .as_deref()
                            .unwrap_or("--")
                    )?;
                }
                writeln!(output)?;
                print_limits(output, &limits)
            }
        }
        Some(CpuCommand::Pl1 { .. } | CpuCommand::Pl2 { .. }) => unreachable!(),
    }
}
