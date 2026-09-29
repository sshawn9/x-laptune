use std::{
    fs, io,
    path::{Path, PathBuf},
};

const CPUFREQ_ROOT: &str = "/sys/devices/system/cpu/cpufreq";

#[derive(Debug)]
pub struct CpuPolicy {
    pub name: String,
    pub cpus: String,
    pub hardware_min_khz: u64,
    pub hardware_max_khz: u64,
    pub current_min_khz: u64,
    pub current_max_khz: u64,
    pub energy_performance_preference: Option<String>,
    pub available_energy_performance_preferences: Vec<String>,
}

fn read_number(policy: &Path, attribute: &str) -> io::Result<u64> {
    fs::read_to_string(policy.join(attribute))?
        .trim()
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn policy_paths() -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(CPUFREQ_ROOT)? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.strip_prefix("policy").is_some_and(|id| !id.is_empty()))
        {
            paths.push(path);
        }
    }
    paths.sort_by_key(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("policy"))
            .and_then(|id| id.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    if paths.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No CPU cpufreq policies found",
        ));
    }
    Ok(paths)
}

/// Read the hardware frequency range and current limits for every cpufreq policy.
pub fn read_policies() -> io::Result<Vec<CpuPolicy>> {
    policy_paths()?
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned();
            let cpus = fs::read_to_string(path.join("affected_cpus"))?
                .trim()
                .to_owned();
            let energy_performance_preference =
                fs::read_to_string(path.join("energy_performance_preference"))
                    .ok()
                    .map(|value| value.trim().to_owned());
            let available_energy_performance_preferences =
                fs::read_to_string(path.join("energy_performance_available_preferences"))
                    .map(|values| values.split_whitespace().map(str::to_owned).collect())
                    .unwrap_or_default();

            Ok(CpuPolicy {
                name,
                cpus,
                hardware_min_khz: read_number(&path, "cpuinfo_min_freq")?,
                hardware_max_khz: read_number(&path, "cpuinfo_max_freq")?,
                current_min_khz: read_number(&path, "scaling_min_freq")?,
                current_max_khz: read_number(&path, "scaling_max_freq")?,
                energy_performance_preference,
                available_energy_performance_preferences,
            })
        })
        .collect()
}

fn write_policy_values(attribute: &str, paths: &[PathBuf], values: &[String]) -> io::Result<()> {
    let previous = paths
        .iter()
        .map(|path| fs::read_to_string(path.join(attribute)).map(|value| value.trim().to_owned()))
        .collect::<io::Result<Vec<_>>>()?;

    for (index, (path, value)) in paths.iter().zip(values).enumerate() {
        if previous[index] == *value {
            continue;
        }
        if let Err(error) = fs::write(path.join(attribute), format!("{value}\n")) {
            let mut rollback_error = None;
            for rollback_index in (0..=index).rev() {
                if previous[rollback_index] != values[rollback_index]
                    && let Err(error) = fs::write(
                        paths[rollback_index].join(attribute),
                        format!("{}\n", previous[rollback_index]),
                    )
                {
                    rollback_error = Some(error);
                }
            }
            if let Some(rollback_error) = rollback_error {
                return Err(io::Error::other(format!(
                    "Failed to write {attribute}: {error}; rollback also failed: {rollback_error}"
                )));
            }
            return Err(error);
        }
    }
    Ok(())
}

/// Set the maximum frequency in MHz across policies, capped at each policy's hardware maximum.
pub fn set_max_frequency_mhz(max_mhz: u64) -> io::Result<()> {
    let requested_khz = max_mhz.checked_mul(1000).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Frequency value is too large")
    })?;
    let paths = policy_paths()?;
    let mut values = Vec::with_capacity(paths.len());
    for path in &paths {
        let current_min_khz = read_number(path, "scaling_min_freq")?;
        let hardware_max_khz = read_number(path, "cpuinfo_max_freq")?;
        let target_khz = requested_khz.min(hardware_max_khz);
        if target_khz < current_min_khz {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Requested maximum {max_mhz} MHz is below the current minimum for {} ({} kHz)",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    current_min_khz
                ),
            ));
        }
        values.push(target_khz.to_string());
    }

    write_policy_values("scaling_max_freq", &paths, &values)
}

/// Restore every policy's maximum frequency limit to its hardware maximum.
pub fn reset_max_frequency() -> io::Result<()> {
    let paths = policy_paths()?;
    let values = paths
        .iter()
        .map(|path| read_number(path, "cpuinfo_max_freq").map(|value| value.to_string()))
        .collect::<io::Result<Vec<_>>>()?;

    write_policy_values("scaling_max_freq", &paths, &values)
}

/// Set the EPP preference on every policy, such as `performance` or `balance_power`.
pub fn set_energy_performance_preference(value: &str) -> io::Result<()> {
    let paths = policy_paths()?;
    for path in &paths {
        let available = fs::read_to_string(path.join("energy_performance_available_preferences"))
            .map_err(|_| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "{} does not support energy_performance_preference",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
            )
        })?;
        if !available
            .split_whitespace()
            .any(|candidate| candidate == value)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} does not support the EPP value {value}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
            ));
        }
    }
    write_policy_values(
        "energy_performance_preference",
        &paths,
        &vec![value.to_owned(); paths.len()],
    )
}
