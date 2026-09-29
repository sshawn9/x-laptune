use serde::{Deserialize, Serialize};
use std::{fs, fs::File, io, path::Path};

use crate::tuxedo::io as ec;

const TABLE_START: usize = 0x0f00;
const TABLE_LENGTH: usize = 96;
const MODE: usize = 0x0751;
const SPLIT: usize = 0x07c5;
const CUSTOM: usize = 0x07c6;
const FULL_BIT: u8 = 0x40;
const SPLIT_BIT: u8 = 0x80;
const CUSTOM_BIT: u8 = 0x04;

pub const CUSTOM_POLICY_PATH: &str = "/etc/x-laptune/fan-policy.json";
const BASELINE: &str = include_str!("policies/baseline.json");
pub const BUILTIN_POLICIES: &[(&str, &str)] = &[("baseline", BASELINE)];

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Curve {
    pub rise_above_c: [u8; 16],
    pub fall_below_c: [u8; 16],
    pub pwm_percent: [f32; 16],
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub cpu: Curve,
    pub gpu: Curve,
}

impl Policy {
    fn to_bytes(&self) -> io::Result<[u8; TABLE_LENGTH]> {
        let mut bytes = [0; TABLE_LENGTH];
        for (index, (name, curve)) in [("CPU", &self.cpu), ("GPU", &self.gpu)]
            .into_iter()
            .enumerate()
        {
            if curve.fall_below_c[0] != 0 || curve.rise_above_c[15] != 255 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "{name}: first fall threshold must be 0 and last rise threshold must be 255"
                    ),
                ));
            }
            for stage in 0..16 {
                let pwm = curve.pwm_percent[stage];
                if !pwm.is_finite() || !(0.0..=100.0).contains(&pwm) || (pwm * 2.0).fract() != 0.0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("{name} stage {stage}: PWM must be 0..100 in steps of 0.5 percent"),
                    ));
                }
                if stage > 0
                    && (curve.rise_above_c[stage] < curve.rise_above_c[stage - 1]
                        || curve.fall_below_c[stage] < curve.fall_below_c[stage - 1]
                        || curve.fall_below_c[stage] > curve.rise_above_c[stage - 1]
                        || pwm < curve.pwm_percent[stage - 1])
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "{name} stage {stage}: thresholds and PWM must be nondecreasing; fall must not exceed the previous rise threshold"
                        ),
                    ));
                }
            }
            let offset = index * 48;
            bytes[offset..offset + 16].copy_from_slice(&curve.rise_above_c);
            bytes[offset + 16..offset + 32].copy_from_slice(&curve.fall_below_c);
            for (byte, pwm) in bytes[offset + 32..offset + 48]
                .iter_mut()
                .zip(curve.pwm_percent)
            {
                *byte = (pwm * 2.0) as u8;
            }
        }
        Ok(bytes)
    }
}

/// Load a policy afresh; "custom" always reads the editable system configuration.
pub fn load(name_or_path: &str) -> io::Result<Policy> {
    let text = if let Some((_, text)) = BUILTIN_POLICIES
        .iter()
        .find(|(name, _)| *name == name_or_path)
    {
        (*text).to_owned()
    } else {
        let path = if name_or_path == "custom" {
            CUSTOM_POLICY_PATH
        } else {
            name_or_path
        };
        fs::read_to_string(path)
            .map_err(|error| io::Error::new(error.kind(), format!("{path}: {error}")))?
    };
    let policy: Policy = serde_json::from_str(&text)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    policy.to_bytes()?;
    Ok(policy)
}

/// Overwrite the editable configuration with the initial baseline, without touching EC state.
pub fn reset_custom_config() -> io::Result<()> {
    let path = Path::new(CUSTOM_POLICY_PATH);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, BASELINE)
}

/// Identify a built-in policy by its data, without trusting a saved policy name.
pub fn builtin_name(policy: &Policy) -> Option<&'static str> {
    BUILTIN_POLICIES.iter().find_map(|(name, text)| {
        (serde_json::from_str::<Policy>(text).ok().as_ref() == Some(policy)).then_some(*name)
    })
}

/// Read the custom table, regardless of whether it is currently enabled.
pub fn read() -> io::Result<Policy> {
    let _lock = ec::lock_ec_read()?;
    read_unlocked()
}

// The caller must hold the device transaction lock while a driver is loaded.
pub(super) fn read_unlocked() -> io::Result<Policy> {
    let bytes: [u8; TABLE_LENGTH] = ec::read_ec(std::array::from_fn(|index| TABLE_START + index))?;
    let curve = |offset| Curve {
        rise_above_c: std::array::from_fn(|index| bytes[offset + index]),
        fall_below_c: std::array::from_fn(|index| bytes[offset + 16 + index]),
        pwm_percent: std::array::from_fn(|index| f32::from(bytes[offset + 32 + index]) / 2.0),
    };
    Ok(Policy {
        cpu: curve(0),
        gpu: curve(48),
    })
}

fn update_bits(acpi: &mut File, offset: usize, mask: u8, value: u8) -> io::Result<()> {
    let [current] = ec::read_ec([offset])?;
    ec::write_ec(acpi, offset, (current & !mask) | (value & mask))
}

fn write_table(acpi: &mut File, bytes: &[u8; TABLE_LENGTH]) -> io::Result<()> {
    for (index, value) in bytes.iter().enumerate() {
        ec::write_ec(acpi, TABLE_START + index, *value)?;
    }
    Ok(())
}

/// Apply a RAM table while keeping EC hysteresis, shared fan control, and ramping.
/// Original tables and control bits are restored if the transaction fails.
pub fn apply(policy: &Policy) -> io::Result<()> {
    let bytes = policy.to_bytes()?;
    let device = ec::open_device()?;
    // Serialize with auto/full and OEM mode changes through verification or rollback.
    device.lock()?;
    let mut acpi = ec::open_ec_writer()?;
    let [manual, mode, split, custom] = ec::read_ec([0x0741, MODE, SPLIT, CUSTOM])?;
    if manual & 1 == 0 {
        return Err(io::Error::other(
            "EC shared control (0x0741 bit0) is disabled; the fan policy cannot run in this state",
        ));
    }
    let original: [u8; TABLE_LENGTH] =
        ec::read_ec(std::array::from_fn(|index| TABLE_START + index))?;

    let result = (|| {
        // Keep the firmware's built-in table active while replacing all 96 bytes.
        update_bits(&mut acpi, CUSTOM, CUSTOM_BIT, 0)?;
        write_table(&mut acpi, &bytes)?;
        update_bits(&mut acpi, SPLIT, SPLIT_BIT, 0)?;
        update_bits(&mut acpi, MODE, FULL_BIT, 0)?;
        update_bits(&mut acpi, CUSTOM, CUSTOM_BIT, CUSTOM_BIT)?;

        let actual: [u8; TABLE_LENGTH] =
            ec::read_ec(std::array::from_fn(|index| TABLE_START + index))?;
        let [manual, mode, split, custom] = ec::read_ec([0x0741, MODE, SPLIT, CUSTOM])?;
        if actual != bytes
            || manual & 1 == 0
            || mode & FULL_BIT != 0
            || split & SPLIT_BIT != 0
            || custom & CUSTOM_BIT == 0
        {
            return Err(io::Error::other(
                "Fan policy readback does not match the requested state",
            ));
        }
        Ok(())
    })();

    if let Err(error) = result {
        let restore: io::Result<()> = (|| {
            update_bits(&mut acpi, CUSTOM, CUSTOM_BIT, 0)?;
            write_table(&mut acpi, &original)?;
            update_bits(&mut acpi, SPLIT, SPLIT_BIT, split)?;
            update_bits(&mut acpi, MODE, FULL_BIT, mode)?;
            update_bits(&mut acpi, CUSTOM, CUSTOM_BIT, custom)
        })();
        return Err(match restore {
            Ok(()) => io::Error::new(
                error.kind(),
                format!("{error}; previous fan state restored"),
            ),
            Err(restore_error) => io::Error::other(format!(
                "{error}; restoring the previous fan state also failed: {restore_error}"
            )),
        });
    }
    Ok(())
}
