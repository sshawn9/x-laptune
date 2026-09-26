use std::{fs, io, path::PathBuf};

const POWER_SUPPLY_ROOT: &str = "/sys/class/power_supply";

const TUXEDO_PLATFORM_DRIVER: &str = "/sys/bus/platform/drivers/tuxedo_keyboard";

fn find_tuxedo_attribute(attribute: &str) -> io::Result<PathBuf> {
    for entry in fs::read_dir(TUXEDO_PLATFORM_DRIVER)? {
        let path = entry?.path().join("charging_profile").join(attribute);
        if path.is_file() {
            return Ok(path);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("The TUXEDO driver does not expose charging_profile/{attribute}"),
    ))
}

/// Read the active TUXEDO charging profile name.
pub fn read_profile() -> io::Result<String> {
    fs::read_to_string(find_tuxedo_attribute("charging_profile")?)
        .map(|value| value.trim().to_owned())
}

/// Read profile names accepted by the loaded driver.
pub fn read_available_profiles() -> io::Result<String> {
    fs::read_to_string(find_tuxedo_attribute("charging_profiles_available")?)
        .map(|value| value.trim().to_owned())
}

/// Set a profile accepted by the loaded TUXEDO driver.
pub fn set_profile(profile: &str) -> io::Result<()> {
    fs::write(
        find_tuxedo_attribute("charging_profile")?,
        format!("{profile}\n"),
    )
}

/// Returns design charge (µAh), current charge (µAh), and charge percentage.
pub fn read_design_and_current_capacity() -> io::Result<(u64, u64, u8)> {
    for entry in fs::read_dir(POWER_SUPPLY_ROOT)? {
        let path = entry?.path();
        let Ok(power_supply_type) = fs::read_to_string(path.join("type")) else {
            continue;
        };
        if power_supply_type.trim() != "Battery"
            || !["charge_full_design", "charge_now", "capacity"]
                .iter()
                .all(|attribute| path.join(attribute).is_file())
        {
            continue;
        }

        return Ok((
            fs::read_to_string(path.join("charge_full_design"))?
                .trim()
                .parse()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            fs::read_to_string(path.join("charge_now"))?
                .trim()
                .parse()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            fs::read_to_string(path.join("capacity"))?
                .trim()
                .parse()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        ));
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "No system battery exposes both design capacity and current charge",
    ))
}
