#[allow(unsafe_code)]
mod io;

pub mod battery;
pub mod fan;
pub mod performance;

/// Return whether tuxedo_io has finished loading; requires no root access.
pub fn is_driver_loaded() -> bool {
    std::fs::read_to_string("/sys/module/tuxedo_io/initstate")
        .is_ok_and(|state| state.trim() == "live")
}
