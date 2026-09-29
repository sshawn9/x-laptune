#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use serde_json::Value;
use std::{
    io::{self, Write},
    process::ExitCode,
};

mod battery;
mod cpu;
mod fan;
mod memory_temp;
mod oem_mode;

#[derive(Parser)]
#[command(
    name = "x-laptune",
    version,
    about = "Monitor laptop hardware and adjust charging, fan, and performance settings",
    after_help = "With no subcommand, run all queries in sequence.\n\nExamples:\n  x-laptune battery\n  sudo x-laptune fan auto\n  sudo x-laptune oem-mode enthusiast\n  x-laptune cpu --json\n  sudo x-laptune cpu pl1 90\n  x-laptune memory-temp --watch --interval-ms 1000"
)]
struct Args {
    /// Output JSON, one object per query or sample
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Query battery information or set the charging profile
    Battery(battery::Args),
    /// Query fans, switch modes, or apply an EC fan policy
    Fan(fan::Args),
    /// Query or set the OEM performance mode
    OemMode(oem_mode::Args),
    /// Query CPU settings or set the frequency limit, EPP, PL1, or PL2
    Cpu(cpu::Args),
    /// Read memory temperatures, optionally sampling continuously
    MemoryTemp(memory_temp::Args),
}

fn context<T>(action: &str, result: io::Result<T>) -> io::Result<T> {
    result.map_err(|error| io::Error::new(error.kind(), format!("{action}: {error}")))
}

fn write_json(output: &mut impl Write, value: Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, &value)?;
    writeln!(output)
}

pub fn run() -> ExitCode {
    let args = Args::parse();
    let commands = match args.command {
        Some(command) => vec![command],
        None => ["battery", "fan", "oem-mode", "cpu", "memory-temp"]
            .into_iter()
            // Reuse the Clap defaults for each subcommand.
            .map(|name| Args::parse_from(["x-laptune", name]).command.unwrap())
            .collect(),
    };
    let mut output = io::stdout().lock();
    let mut status = ExitCode::SUCCESS;
    for (index, command) in commands.into_iter().enumerate() {
        let (name, title) = match &command {
            Command::Battery(_) => ("battery", "Battery"),
            Command::Fan(_) => ("fan", "Fans"),
            Command::OemMode(_) => ("oem-mode", "OEM Performance"),
            Command::Cpu(_) => ("cpu", "CPU"),
            Command::MemoryTemp(_) => ("memory-temp", "Memory Temperature"),
        };
        let result = (|| {
            if !args.json {
                if index > 0 {
                    writeln!(output)?;
                }
                writeln!(output, "[{title}]")?;
                output.flush()?;
            }
            match command {
                Command::Battery(command) => battery::run(&mut output, command, args.json),
                Command::Fan(command) => fan::run(&mut output, command, args.json),
                Command::OemMode(command) => oem_mode::run(&mut output, command, args.json),
                Command::Cpu(command) => cpu::run(&mut output, command, args.json),
                Command::MemoryTemp(command) => memory_temp::run(&mut output, command, args.json),
            }?;
            output.flush()
        })();

        match result {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => return ExitCode::SUCCESS,
            Err(error) => {
                let mut stderr = io::stderr().lock();
                let _ = writeln!(stderr, "x-laptune {name}: {error}");
                if error.kind() == io::ErrorKind::PermissionDenied {
                    let _ = writeln!(stderr, "Permission denied. Run this command with sudo.");
                }
                status = ExitCode::FAILURE;
            }
        }
    }
    status
}
