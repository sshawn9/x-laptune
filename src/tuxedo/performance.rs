use std::{fmt, fs::File, io};

const UW_IOCTL_READ_MODE: libc::c_ulong = 0x8008_ef14;
const UW_IOCTL_SET_PERFORMANCE_PROFILE: libc::c_ulong = 0x4008_f018;
const PROFILE_MASK: i32 = 0xb0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum PerformanceMode {
    #[value(name = "powersave")]
    Powersave,
    Enthusiast,
    Overboost,
}

impl PerformanceMode {
    fn ioctl_value(self) -> i32 {
        match self {
            Self::Powersave => 1,
            Self::Enthusiast => 2,
            Self::Overboost => 3,
        }
    }

    fn from_register_value(value: i32) -> io::Result<Self> {
        match value & PROFILE_MASK {
            0xa0 => Ok(Self::Powersave),
            0x00 => Ok(Self::Enthusiast),
            0x10 => Ok(Self::Overboost),
            value => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unrecognized Uniwill performance mode bits: 0x{value:02x}"),
            )),
        }
    }

    fn read_from(file: &File) -> io::Result<Self> {
        let register = super::io::read_i32(file, UW_IOCTL_READ_MODE)?;
        Self::from_register_value(register)
    }
}

impl fmt::Display for PerformanceMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(clap::ValueEnum::to_possible_value(self).unwrap().get_name())
    }
}

/// Read the current Uniwill OEM performance mode.
pub fn read_mode() -> io::Result<PerformanceMode> {
    let file = super::io::open_device()?;
    PerformanceMode::read_from(&file)
}

/// Set the Uniwill OEM performance mode and verify the EC register readback.
pub fn set_mode(mode: PerformanceMode) -> io::Result<()> {
    let file = super::io::open_device()?;
    super::io::write_i32(&file, UW_IOCTL_SET_PERFORMANCE_PROFILE, mode.ioctl_value())?;

    let actual = PerformanceMode::read_from(&file)?;
    if actual != mode {
        return Err(io::Error::other(format!(
            "OEM performance mode read back as {actual} after requesting {mode}"
        )));
    }
    Ok(())
}
