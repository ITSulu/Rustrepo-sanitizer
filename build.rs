use std::fmt::Write as _;

/// The single source of truth for the common glob presets offered by both the
/// desktop GUI and the web UI, so the two interfaces cannot drift.
pub const COMMON_INCLUDE_GLOBS: &[&str] = &[
    "docs/**/*.md",
    "src/**/*.rs",
    "tests/**",
    ".forgejo/**",
    "target/**",
    "vendor/**",
    "*.log",
];

pub const COMMON_EXCLUDE_GLOBS: &[&str] = &[
    "docs/**",
    "target/**",
    "vendor/**",
    "node_modules/**",
    ".idea/**",
    "*.log",
    "*.tmp",
];

/// Renders a comma separated list for the Rust side of the library.
fn csv(entries: &[&str]) -> String {
    entries.join(",")
}

/// Renders a Slint array literal, for example `["a", "b"]`.
fn slint_array(entries: &[&str]) -> String {
    let mut out = String::from("[");
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{entry:?}");
    }
    out.push(']');
    out
}

fn main() {
    println!("cargo:rerun-if-changed=ui/main.slint");
    println!("cargo:rerun-if-changed=build.rs");

    // Feed the shared presets into the Slint UI so the GUI dropdowns and the web
    // dropdowns are generated from the same list.
    println!(
        "cargo:rustc-env=RRS_COMMON_INCLUDE_GLOBS={}",
        slint_array(COMMON_INCLUDE_GLOBS)
    );
    println!(
        "cargo:rustc-env=RRS_COMMON_EXCLUDE_GLOBS={}",
        slint_array(COMMON_EXCLUDE_GLOBS)
    );

    println!(
        "cargo:rustc-env=RRS_COMMON_INCLUDE_GLOBS_STR={}",
        csv(COMMON_INCLUDE_GLOBS)
    );
    println!(
        "cargo:rustc-env=RRS_COMMON_EXCLUDE_GLOBS_STR={}",
        csv(COMMON_EXCLUDE_GLOBS)
    );

    slint_build::compile("ui/main.slint").expect("Slint UI must compile");
}
