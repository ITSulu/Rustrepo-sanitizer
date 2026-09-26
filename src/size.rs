//! Binary size units shared by the CLI, GUI, and web UI.
//!
//! One typed parser/converter/formatter is used by every interface so a value
//! entered as `10 MiB` means exactly the same number of bytes everywhere.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeUnit {
    /// Bytes (no binary prefix).
    Byte,
    Kib,
    Mib,
    Gib,
}

impl SizeUnit {
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Byte => "B",
            Self::Kib => "KiB",
            Self::Mib => "MiB",
            Self::Gib => "GiB",
        }
    }

    pub const fn bytes(self) -> u64 {
        match self {
            Self::Byte => 1,
            Self::Kib => 1024,
            Self::Mib => 1024 * 1024,
            Self::Gib => 1024 * 1024 * 1024,
        }
    }

    /// The selectable units offered by the GUI and web UI.
    pub const ALL: [SizeUnit; 3] = [Self::Kib, Self::Mib, Self::Gib];
}

impl fmt::Display for SizeUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.suffix())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeError {
    Empty,
    NotANumber,
    UnknownUnit,
    Malformed,
    OutOfRange,
}

impl fmt::Display for SizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Empty => "size must not be empty",
            Self::NotANumber => "size must start with a number",
            Self::UnknownUnit => "unknown size unit; use KiB, MiB, or GiB",
            Self::Malformed => "size must look like 512, 10MiB, or 2 GiB",
            Self::OutOfRange => "size is out of range",
        };
        f.write_str(text)
    }
}

impl std::error::Error for SizeError {}

/// A byte count plus the typed unit it was expressed in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    bytes: f64,
    unit: SizeUnit,
}

impl Size {
    pub const fn from_bytes(bytes: u64) -> Self {
        Self {
            bytes: bytes as f64,
            unit: SizeUnit::Byte,
        }
    }

    pub const fn new(value: f64, unit: SizeUnit) -> Self {
        Self {
            bytes: value * unit.bytes() as f64,
            unit,
        }
    }

    pub fn bytes(&self) -> u64 {
        self.bytes.round().max(0.0) as u64
    }

    pub fn unit(&self) -> SizeUnit {
        self.unit
    }

    /// The same size expressed in `target`, preserving the byte value.
    pub fn to_unit(&self, target: SizeUnit) -> f64 {
        self.bytes / target.bytes() as f64
    }

    /// Re-expresses this size in `target` for display or form round-tripping.
    pub fn converted(&self, target: SizeUnit) -> Self {
        Self {
            bytes: self.bytes,
            unit: target,
        }
    }

    /// A human value for form fields: whole numbers stay whole.
    pub fn value_in(&self, target: SizeUnit) -> String {
        let value = self.to_unit(target);
        if (value.fract()).abs() < f64::EPSILON {
            format!("{}", value.round() as i64)
        } else {
            format!("{value}")
        }
    }

    /// The largest unit whose value is at least 1, so output stays readable
    /// (`1536` renders as `1.5 KiB`).
    pub fn display(&self) -> Self {
        let bytes = self.bytes();
        for unit in [SizeUnit::Gib, SizeUnit::Mib, SizeUnit::Kib] {
            if bytes >= unit.bytes() {
                return Self {
                    bytes: bytes as f64,
                    unit,
                };
            }
        }
        Self {
            bytes: bytes as f64,
            unit: SizeUnit::Byte,
        }
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let display = self.display();
        let value = display.to_unit(display.unit());
        if (value.fract()).abs() < f64::EPSILON {
            write!(f, "{} {}", value.round() as i64, display.unit())
        } else {
            write!(f, "{value} {}", display.unit())
        }
    }
}

fn unit_from_suffix(suffix: &str) -> Option<SizeUnit> {
    match suffix.to_ascii_lowercase().as_str() {
        "" => Some(SizeUnit::Byte),
        "b" | "byte" | "bytes" => Some(SizeUnit::Byte),
        "kib" | "k" => Some(SizeUnit::Kib),
        "mib" | "m" => Some(SizeUnit::Mib),
        "gib" | "g" => Some(SizeUnit::Gib),
        _ => None,
    }
}

/// Parses `512`, `10MiB`, `2 GiB`, and similar binary sizes.
pub fn parse_size(input: &str) -> Result<Size, SizeError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(SizeError::Empty);
    }
    let split = trimmed
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(trimmed.len());
    let (number, suffix) = trimmed.split_at(split);
    let number = number.replace(',', "");
    if number.is_empty() {
        return Err(SizeError::NotANumber);
    }
    let value: f64 = number.parse().map_err(|_| SizeError::NotANumber)?;
    if !value.is_finite() || value < 0.0 {
        return Err(SizeError::OutOfRange);
    }
    let suffix = suffix.trim();
    if suffix.contains(char::is_whitespace) && suffix.split_whitespace().count() > 1 {
        return Err(SizeError::Malformed);
    }
    let unit = unit_from_suffix(suffix).ok_or(SizeError::UnknownUnit)?;
    let size = Size::new(value, unit);
    if size.bytes() as f64 > u64::MAX as f64 {
        return Err(SizeError::OutOfRange);
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uses_suffix_and_defaults_to_bytes() {
        assert_eq!(parse_size("2048").unwrap().unit(), SizeUnit::Byte);
        assert_eq!(parse_size("2048").unwrap().bytes(), 2048);
        assert_eq!(parse_size("1KiB").unwrap().unit(), SizeUnit::Kib);
    }

    #[test]
    fn conversion_round_trips_exactly_for_integral_units() {
        let size = parse_size("1GiB").unwrap();
        let mib = size.to_unit(SizeUnit::Mib);
        assert_eq!(mib, 1024.0);
        assert_eq!((mib * 1024.0 * 1024.0) as u64, size.bytes());
    }

    #[test]
    fn converted_keeps_bytes_and_changes_unit() {
        let size = parse_size("10MiB").unwrap();
        let converted = size.converted(SizeUnit::Gib);
        assert_eq!(converted.bytes(), size.bytes());
        assert_eq!(converted.unit(), SizeUnit::Gib);
        assert_eq!(converted.value_in(SizeUnit::Gib), "0.009765625");
    }

    #[test]
    fn display_picks_largest_unit_with_value_at_least_one() {
        assert_eq!(parse_size("2048").unwrap().display().unit(), SizeUnit::Kib);
        // 1500 bytes is 1.46 KiB, so it still renders in KiB.
        assert_eq!(parse_size("1500").unwrap().display().unit(), SizeUnit::Kib);
        assert_eq!(parse_size("900").unwrap().display().unit(), SizeUnit::Byte);
        assert_eq!(parse_size("1536").unwrap().display().unit(), SizeUnit::Kib);
        assert_eq!(parse_size("3GiB").unwrap().display().unit(), SizeUnit::Gib);
    }
}
