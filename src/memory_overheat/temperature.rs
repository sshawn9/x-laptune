use std::{collections::BTreeMap, fs, io, path::PathBuf};

pub fn discover_sensors() -> io::Result<Vec<PathBuf>> {
    let mut sensors = Vec::new();
    for entry in fs::read_dir("/sys/class/hwmon")? {
        let path = entry?.path();
        if fs::read_to_string(path.join("name")).is_ok_and(|name| name.trim() == "spd5118") {
            sensors.push(path);
        }
    }
    if sensors.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No spd5118 memory temperature sensors found",
        ));
    }
    Ok(sensors)
}

/// Values are in millidegrees Celsius; unreadable temperatures are None.
pub fn read_temperatures(sensors: &[PathBuf]) -> BTreeMap<String, Option<i32>> {
    sensors
        .iter()
        .map(|path| {
            let sensor = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let temperature = fs::read_to_string(path.join("temp1_input"))
                .ok()
                .and_then(|value| value.trim().parse::<i32>().ok());
            (sensor, temperature)
        })
        .collect()
}
