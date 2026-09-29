use std::io;

pub mod policy;

const UW_IOCTL_READ_MODE: libc::c_ulong = 0x8008_ef14;
const UW_IOCTL_WRITE_MODE: libc::c_ulong = 0x4008_f012;
const UW_IOCTL_FAN_AUTO: libc::c_ulong = 0x0000_f014;
const UW_MODE_FULL_FAN_BIT: i32 = 0x40;

/// EC control flags; full speed and manual control may both be enabled.
#[derive(Debug)]
pub struct FanControlState {
    /// Native automatic control: manual, full-speed, and custom-table control are off.
    pub automatic: bool,
    pub full_speed: bool,
    pub manual_control: bool,
    pub custom_curve: bool,
    pub separate_fans: bool,
}

#[derive(Debug)]
pub struct FanRpm {
    pub current_rpm: u16,
    /// Hardware RPM limits, not observed minimum/maximum samples. Unknown on this EC.
    pub min_rpm: Option<u16>,
    pub max_rpm: Option<u16>,
}

/// Read fan control flags without loading TUXEDO; requires read access to /dev/mem.
pub fn read_control_state() -> io::Result<FanControlState> {
    let _lock = super::io::lock_ec_read()?;
    read_control_state_unlocked()
}

fn read_control_state_unlocked() -> io::Result<FanControlState> {
    let [manual, mode, split, table] = super::io::read_ec([0x0741, 0x0751, 0x07c5, 0x07c6])?;
    let manual_control = manual & 1 != 0;
    let full_speed = i32::from(mode) & UW_MODE_FULL_FAN_BIT != 0;
    Ok(FanControlState {
        automatic: !manual_control && !full_speed && table & 0x04 == 0,
        full_speed,
        manual_control,
        custom_curve: table & 0x04 != 0,
        separate_fans: split & 0x80 != 0,
    })
}

/// Read control flags and the enabled custom table under one shared transaction lock.
pub fn read_control_state_and_policy() -> io::Result<(FanControlState, Option<policy::Policy>)> {
    let _lock = super::io::lock_ec_read()?;
    let state = read_control_state_unlocked()?;
    let current = if state.custom_curve {
        Some(policy::read_unlocked()?)
    } else {
        None
    };
    Ok((state, current))
}

/// Read fan 1 and fan 2 RPM, in that order; physical left/right is not established.
/// Requires read access to /dev/mem, without loading TUXEDO.
/// This EC has no verified minimum/maximum RPM fields, so both limits are None.
pub fn read_fan_speeds() -> io::Result<[FanRpm; 2]> {
    for _ in 0..4 {
        // Read each high byte again to catch a counter change between byte reads.
        let [high1, low1, check1, high2, low2, check2] =
            super::io::read_ec([0x0464, 0x0465, 0x0464, 0x046c, 0x046d, 0x046c])?;
        if high1 == check1 && high2 == check2 {
            return Ok([[high1, low1], [high2, low2]].map(|bytes| FanRpm {
                current_rpm: u16::from_be_bytes(bytes),
                min_rpm: None,
                max_rpm: None,
            }));
        }
    }
    Err(io::Error::other(
        "Fan RPM changed during the read; could not obtain a consistent reading",
    ))
}

/// Set the full-fan bit, preserving other mode bits.
pub fn set_full_mode() -> io::Result<()> {
    let file = super::io::open_device()?;
    // Share the policy/OEM mode transaction lock through the final readback.
    file.lock()?;
    let current = super::io::read_i32(&file, UW_IOCTL_READ_MODE)?;
    let requested = (current & 0xff) | UW_MODE_FULL_FAN_BIT;
    if current & UW_MODE_FULL_FAN_BIT == 0 {
        super::io::write_i32(&file, UW_IOCTL_WRITE_MODE, requested)?;
    }

    let actual = super::io::read_i32(&file, UW_IOCTL_READ_MODE)?;
    if actual & UW_MODE_FULL_FAN_BIT == 0 {
        return Err(io::Error::other("Full-speed fan mode did not take effect"));
    }
    Ok(())
}

/// Leave full-fan/custom-table mode, preserving the EC manual-control setting.
pub fn set_auto_mode() -> io::Result<()> {
    let file = super::io::open_device()?;
    // Share the policy/OEM mode transaction lock through the final readback.
    file.lock()?;
    let current = super::io::read_i32(&file, UW_IOCTL_READ_MODE)?;
    super::io::call(&file, UW_IOCTL_FAN_AUTO)?;
    super::io::write_i32(
        &file,
        UW_IOCTL_WRITE_MODE,
        current & 0xff & !UW_MODE_FULL_FAN_BIT,
    )?;

    let actual = super::io::read_i32(&file, UW_IOCTL_READ_MODE)?;
    if actual & UW_MODE_FULL_FAN_BIT != 0 {
        return Err(io::Error::other(
            "Full-speed mode bit is still set after requesting automatic fan mode",
        ));
    }
    let [split, table] = super::io::read_ec([0x07c5, 0x07c6])?;
    if split & 0x80 != 0 || table & 0x04 != 0 {
        return Err(io::Error::other(
            "Custom fan control is still enabled after requesting automatic fan mode",
        ));
    }
    Ok(())
}
