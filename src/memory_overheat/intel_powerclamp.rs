use std::{
    fs, io,
    path::{Path, PathBuf},
};

const MAX_IDLE_PATH: &str = "/sys/module/intel_powerclamp/parameters/max_idle";
const CALIBRATION_TABLE_PATH: &str = "/sys/kernel/debug/intel_powerclamp/powerclamp_calib";

/// Find the intel_powerclamp cooling device.
pub fn find_cooling_device() -> io::Result<PathBuf> {
    for entry in fs::read_dir("/sys/class/thermal")? {
        let path = entry?.path();
        if fs::read_to_string(path.join("type"))
            .is_ok_and(|value| value.trim() == "intel_powerclamp")
        {
            return Ok(path);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "No intel_powerclamp cooling device found",
    ))
}

/// Read the raw max_idle driver parameter.
pub fn read_max_idle() -> io::Result<u8> {
    fs::read_to_string(MAX_IDLE_PATH)?
        .trim()
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Set max_idle to 75 and return the value read back from the driver.
pub fn maximize_idle_limit() -> io::Result<u8> {
    fs::write(MAX_IDLE_PATH, "75")?;
    read_max_idle()
}

/// Read the throttling target percentage from cur_state.
pub fn read_target_percent(device: &Path) -> io::Result<u8> {
    fs::read_to_string(device.join("cur_state"))?
        .trim()
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Write the throttling target and return the value read back; 0 disables throttling.
pub fn set_target_percent(device: &Path, percent: u8) -> io::Result<u8> {
    fs::write(device.join("cur_state"), percent.to_string())?;
    read_target_percent(device)
}

/// Read the raw calibration table: idle ratios, confidence, steady-state and dynamic compensation.
pub fn read_calibration_table() -> io::Result<String> {
    fs::read_to_string(CALIBRATION_TABLE_PATH)
}
