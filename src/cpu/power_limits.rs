use std::{
    fs, io,
    path::{Path, PathBuf},
};

const POWER_CAP_ROOT: &str = "/sys/class/powercap";
const INTEL_RAPL_PREFIX: &str = "intel-rapl:";

#[derive(Debug)]
pub struct RaplPowerLimit {
    pub package: String,
    pub index: usize,
    pub name: String,
    pub current_microwatts: u64,
    pub max_microwatts: Option<u64>,
    pub time_window_microseconds: Option<u64>,
}

fn read_number(path: &Path) -> io::Result<u64> {
    fs::read_to_string(path)?
        .trim()
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn package_zones() -> io::Result<Vec<PathBuf>> {
    let mut zones = Vec::new();

    for entry in fs::read_dir(POWER_CAP_ROOT)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        let Some(id) = name.strip_prefix(INTEL_RAPL_PREFIX) else {
            continue;
        };
        if id.is_empty() || id.contains(':') || !id.chars().all(|digit| digit.is_ascii_digit()) {
            continue;
        }

        let package = fs::read_to_string(path.join("name"))?;
        if package.trim().starts_with("package-") {
            zones.push(path);
        }
    }

    zones.sort();
    if zones.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No Intel RAPL CPU packages found",
        ));
    }
    Ok(zones)
}

fn optional_number(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Read PL1 and PL2 values exposed by each Intel RAPL CPU package.
pub fn read_cpu_power_limits() -> io::Result<Vec<RaplPowerLimit>> {
    let mut limits = Vec::new();

    for zone in package_zones()? {
        let package = fs::read_to_string(zone.join("name"))?.trim().to_owned();

        for index in 0..2 {
            let name_path = zone.join(format!("constraint_{index}_name"));
            if !name_path.exists() {
                continue;
            }

            let name = fs::read_to_string(&name_path)?.trim().to_owned();
            let current_microwatts =
                read_number(&zone.join(format!("constraint_{index}_power_limit_uw")))?;

            limits.push(RaplPowerLimit {
                package: package.clone(),
                index,
                name,
                current_microwatts,
                max_microwatts: optional_number(
                    &zone.join(format!("constraint_{index}_max_power_uw")),
                ),
                time_window_microseconds: optional_number(
                    &zone.join(format!("constraint_{index}_time_window_us")),
                ),
            });
        }
    }

    Ok(limits)
}

fn set_cpu_power_limit(index: usize, expected_name: &str, watts: u64) -> io::Result<u64> {
    let zones = package_zones()?;
    if zones.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "Found {} CPU packages; cannot select a package to modify",
                zones.len()
            ),
        ));
    }

    let zone = &zones[0];
    let name = fs::read_to_string(zone.join(format!("constraint_{index}_name")))?;
    if name.trim() != expected_name {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "constraint_{index} is named {:?}; expected {expected_name}",
                name.trim()
            ),
        ));
    }

    let requested_microwatts = watts
        .checked_mul(1_000_000)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Power value is too large"))?;
    let limit_path = zone.join(format!("constraint_{index}_power_limit_uw"));
    let previous_microwatts = read_number(&limit_path)?;
    fs::write(&limit_path, format!("{requested_microwatts}\n"))?;

    let failure = match read_number(&limit_path) {
        Ok(actual) if actual == requested_microwatts => return Ok(actual),
        Ok(actual) => io::Error::other(format!(
            "Requested {requested_microwatts} µW, read back {actual} µW"
        )),
        Err(error) => io::Error::new(
            error.kind(),
            format!("Failed to read back the requested {requested_microwatts} µW: {error}"),
        ),
    };

    let restored = fs::write(&limit_path, format!("{previous_microwatts}\n"))
        .and_then(|()| read_number(&limit_path));
    let restore_status = match restored {
        Ok(actual) if actual == previous_microwatts => {
            format!("restored the previous value: {previous_microwatts} µW")
        }
        Ok(actual) => format!(
            "failed to restore the previous {previous_microwatts} µW: read back {actual} µW"
        ),
        Err(error) => {
            format!("could not restore and verify the previous {previous_microwatts} µW: {error}")
        }
    };
    Err(io::Error::new(
        failure.kind(),
        format!("{failure}; {restore_status}"),
    ))
}

/// Set package PL1 in watts and return the verified readback in microwatts.
pub fn set_pl1_watts(watts: u64) -> io::Result<u64> {
    set_cpu_power_limit(0, "long_term", watts)
}

/// Set package PL2 in watts and return the verified readback in microwatts.
pub fn set_pl2_watts(watts: u64) -> io::Result<u64> {
    set_cpu_power_limit(1, "short_term", watts)
}
