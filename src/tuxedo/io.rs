use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
};

const DEVICE: &str = "/dev/tuxedo_io";
const UW_IOCTL_CHECK_INTERFACE: libc::c_ulong = 0x8008_ec06;
const EC_BASE: libc::off_t = 0xfe41_0000;
const EC_LENGTH: usize = 4096;

fn check_ec_firmware() -> io::Result<()> {
    if fs::read_to_string("/sys/class/dmi/id/board_name")?.trim() != "GM6AQ7C"
        || fs::read_to_string("/sys/class/dmi/id/bios_version")?.trim() != "N.1.04MRO11"
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "EC access has only been verified on GM6AQ7C / N.1.04MRO11",
        ));
    }
    Ok(())
}

/// Read the verified GM6AQ7C EC mapping without loading a driver or writing EC state.
pub(crate) fn read_ec<const N: usize>(offsets: [usize; N]) -> io::Result<[u8; N]> {
    if offsets.iter().any(|&offset| offset >= EC_LENGTH) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "EC offset exceeds the mapped range",
        ));
    }
    check_ec_firmware()?;

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_SYNC)
        .open("/dev/mem")?;
    // SAFETY: EC_BASE is page-aligned and verified from this firmware's ECMA/ECRR.
    // The live file descriptor is opened read-only; no writable mapping is created.
    let mapping = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            EC_LENGTH,
            libc::PROT_READ,
            libc::MAP_SHARED,
            file.as_raw_fd(),
            EC_BASE,
        )
    };
    if mapping == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }

    let values = offsets.map(|offset| {
        // SAFETY: Every offset was checked against the live mapping's length.
        // Volatile byte reads preserve the access order required by EC registers.
        unsafe { std::ptr::read_volatile(mapping.cast::<u8>().add(offset)) }
    });
    // SAFETY: This is the exact mapping and length returned above, no longer used.
    if unsafe { libc::munmap(mapping, EC_LENGTH) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(values)
}

/// Share the device transaction lock while reading; unloaded drivers have no writers.
pub(crate) fn lock_ec_read() -> io::Result<Option<File>> {
    let file = match File::open(DEVICE) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    file.lock_shared()?;
    Ok(Some(file))
}

pub(crate) fn open_device() -> io::Result<File> {
    let file = OpenOptions::new().read(true).write(true).open(DEVICE)?;
    let compatible = read_i32(&file, UW_IOCTL_CHECK_INTERFACE)?;
    if compatible != 1 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "The TUXEDO driver did not detect an available Uniwill interface",
        ));
    }
    Ok(file)
}

pub(crate) fn read_i32(file: &File, request: libc::c_ulong) -> io::Result<i32> {
    let mut value = 0_i32;
    // SAFETY: These read ioctls copy one i32 into the valid pointer provided here.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), request, &mut value as *mut i32) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(value)
}

pub(crate) fn write_i32(file: &File, request: libc::c_ulong, value: i32) -> io::Result<()> {
    // SAFETY: These write ioctls copy one i32 from the valid pointer provided here.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), request, &value as *const i32) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn call(file: &File, request: libc::c_ulong) -> io::Result<()> {
    // SAFETY: This ioctl is defined without a third argument.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), request) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Lock acpi_call for an EC transaction and verify the firmware's mapping.
pub(crate) fn open_ec_writer() -> io::Result<File> {
    check_ec_firmware()?;
    let mut acpi = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/proc/acpi/call")
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    error.kind(),
                    "Load the acpi_call module before applying a fan policy",
                )
            } else {
                error
            }
        })?;
    // SAFETY: acpi owns a live descriptor. Dropping it releases the lock.
    if unsafe { libc::flock(acpi.as_raw_fd(), libc::LOCK_EX) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let base = acpi_integer(&mut acpi, r"\_SB.PC00.LPCB.EC0.ECMA")?;
    if base != EC_BASE as u64 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Unexpected EC mapping: {base:#x}"),
        ));
    }
    Ok(acpi)
}

/// Write one EC byte through the firmware, then verify its readback.
pub(crate) fn write_ec(acpi: &mut File, offset: usize, value: u8) -> io::Result<()> {
    if offset >= EC_LENGTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "EC offset exceeds the mapped range",
        ));
    }
    let address = EC_BASE as u64 + offset as u64;
    // MMRW is the firmware method used by ECRW and acquires the firmware mutex.
    acpi_integer(
        acpi,
        &format!(r"\_SB.INOU.MMRW {address:#x} 1 0 {value:#x}"),
    )?;
    let actual = acpi_integer(acpi, &format!(r"\_SB.INOU.ECRR {offset:#x}"))?;
    if actual != u64::from(value) {
        return Err(io::Error::other(format!(
            "EC {offset:#06x}: wrote {value:#04x}, read back {actual:#04x}"
        )));
    }
    Ok(())
}

fn acpi_integer(file: &mut File, command: &str) -> io::Result<u64> {
    file.rewind()?;
    file.write_all(format!("{command}\n").as_bytes())?;
    file.rewind()?;
    let mut buffer = [0_u8; 4096];
    let count = file.read(&mut buffer)?;
    let response = std::str::from_utf8(&buffer[..count])
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let hex = response.strip_prefix("0x").ok_or_else(|| {
        io::Error::other(format!(
            "ACPI call {command} returned an unexpected response: {response}"
        ))
    })?;
    u64::from_str_radix(hex, 16).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
