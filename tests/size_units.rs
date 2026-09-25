use itsulu_repo_sanitizer::size::{parse_size, SizeError, SizeUnit};

#[test]
fn parses_binary_units_into_bytes() {
    assert_eq!(parse_size("512").unwrap().bytes(), 512);
    assert_eq!(parse_size("10MiB").unwrap().bytes(), 10 * 1024 * 1024);
    assert_eq!(parse_size("2mib").unwrap().bytes(), 2 * 1024 * 1024);
    assert_eq!(parse_size("1 GiB").unwrap().bytes(), 1024 * 1024 * 1024);
    assert_eq!(parse_size("1KiB").unwrap().bytes(), 1024);
}

#[test]
fn rejects_unknown_or_empty_units() {
    assert_eq!(parse_size(""), Err(SizeError::Empty));
    assert_eq!(parse_size("10MB"), Err(SizeError::UnknownUnit));
    assert_eq!(parse_size("abc"), Err(SizeError::NotANumber));
    assert_eq!(parse_size("10 MiB extra"), Err(SizeError::Malformed));
}

#[test]
fn converts_between_units_preserving_byte_equivalence() {
    let size = parse_size("10MiB").unwrap();
    assert_eq!(size.to_unit(SizeUnit::Kib), 10240.0);
    assert_eq!(size.to_unit(SizeUnit::Mib), 10.0);
    assert_eq!(size.to_unit(SizeUnit::Gib), 10.0 / 1024.0);
    // Converting and back is lossless for whole bytes.
    let kib = size.to_unit(SizeUnit::Kib);
    assert_eq!(kib as u64 * 1024, size.bytes());
}

#[test]
fn formats_bytes_using_the_largest_exact_unit() {
    assert_eq!(format!("{}", parse_size("512KiB").unwrap()), "512 KiB");
    assert_eq!(format!("{}", parse_size("10MiB").unwrap()), "10 MiB");
    assert_eq!(format!("{}", parse_size("2GiB").unwrap()), "2 GiB");
    // Sub-unit values stay in bytes rather than rounding to zero.
    assert_eq!(format!("{}", parse_size("900").unwrap()), "900 B");
    assert_eq!(format!("{}", parse_size("1536").unwrap()), "1.5 KiB");
}
