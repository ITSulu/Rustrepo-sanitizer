//! Shared sanitizer core consumed by both the CLI and GUI frontends.
pub mod sanitizer;
pub mod security;

/// User-facing capabilities. Keep this registry authoritative for both interfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilitySurface {
    CliGui,
    CliOnly,
    Internal,
}

#[derive(Clone, Copy, Debug)]
pub struct Capability {
    pub id: &'static str,
    pub label: &'static str,
    pub surface: CapabilitySurface,
}

pub const CAPABILITIES: &[Capability] = &[
    Capability {
        id: "repository",
        label: "Repository selection",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "output",
        label: "Output location and filename",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "archive",
        label: "Archive format",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "compression",
        label: "Compression",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "tracked",
        label: "Tracked and untracked files",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "filters",
        label: "Include and exclude rules",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "limits",
        label: "Maximum file size",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "redaction",
        label: "Redaction and fail-on-secret",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "reports",
        label: "Reports",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "encryption",
        label: "Password and encryption",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "dry-run",
        label: "Dry run",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "timestamp-name",
        label: "Timestamped output naming",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "diagnostics",
        label: "Verbosity and quiet diagnostics",
        surface: CapabilitySurface::CliGui,
    },
    Capability {
        id: "list-formats",
        label: "Capability matrix listing",
        surface: CapabilitySurface::CliOnly,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    #[derive(Debug)]
    struct GuiState {
        name: &'static str,
        status: &'static str,
        running: bool,
        has_result: bool,
    }

    #[allow(dead_code)]
    #[derive(Debug)]
    struct GuiReviewState {
        name: &'static str,
        menu_entries: &'static [&'static str],
        archive: &'static str,
        compression_choices: &'static [&'static str],
        password_visible: bool,
        include_patterns: &'static [&'static str],
        exclude_patterns: &'static [&'static str],
    }
    #[test]
    fn all_capabilities_are_gui_or_explicit() {
        assert!(CAPABILITIES.iter().all(|c| matches!(
            c.surface,
            CapabilitySurface::CliGui | CapabilitySurface::CliOnly | CapabilitySurface::Internal
        )));
    }

    #[test]
    fn capability_registry_snapshot() {
        insta::assert_debug_snapshot!(CAPABILITIES);
    }

    #[test]
    fn gui_state_snapshots() {
        insta::assert_debug_snapshot!(
            "gui-stable-states",
            [
                GuiState {
                    name: "default",
                    status: "Ready to sanitize a repository",
                    running: false,
                    has_result: false
                },
                GuiState {
                    name: "running",
                    status: "Scanning… 3 files",
                    running: true,
                    has_result: false
                },
                GuiState {
                    name: "success",
                    status: "Complete: 3 files, 1 redactions",
                    running: false,
                    has_result: true
                },
                GuiState {
                    name: "validation-error",
                    status:
                        "Invalid options: compression is not valid for the selected archive format",
                    running: false,
                    has_result: false
                },
            ]
        );
    }

    #[test]
    fn gui_review2_state_snapshots() {
        insta::assert_debug_snapshot!(
            "gui-review2-states",
            [
                GuiReviewState {
                    name: "default-tar",
                    menu_entries: &["File", "Help", "Settings", "About"],
                    archive: "tar",
                    compression_choices: &["zstd", "gzip", "none"],
                    password_visible: false,
                    include_patterns: &[],
                    exclude_patterns: &[],
                },
                GuiReviewState {
                    name: "zip-password",
                    menu_entries: &["File", "Help", "Settings", "About"],
                    archive: "zip",
                    compression_choices: &["gzip", "zstd"],
                    password_visible: true,
                    include_patterns: &["src/**/*.rs", "tests/**"],
                    exclude_patterns: &["target/**"],
                },
                GuiReviewState {
                    name: "none-stream",
                    menu_entries: &["File", "Help", "Settings", "About"],
                    archive: "none",
                    compression_choices: &[
                        "gzip", "zstd", "LZ4", "XZ", "zlib", "Brotli", "Snappy", "bzip2",
                    ],
                    password_visible: false,
                    include_patterns: &["docs/**/*.md"],
                    exclude_patterns: &[],
                },
            ]
        );
    }

    #[test]
    fn application_icon_asset_is_nonempty_and_vector_source() {
        const ICON: &[u8] = include_bytes!("../assets/rustrepo-sanitizer.svg");
        let source = std::str::from_utf8(ICON).expect("application icon is UTF-8 SVG");
        assert!(source.contains("<svg"));
        assert!(source.contains("viewBox="));
        assert!(source.contains("Rustrepo Sanitizer"));
    }

    #[test]
    fn advanced_gui_panel_does_not_use_an_immutable_height() {
        let ui = include_str!("../ui/main.slint");
        assert!(
            !ui.contains("ScrollView { height: 210px;"),
            "Advanced options must remain responsive as the native window is resized"
        );
    }

    #[test]
    fn gui_title_uses_authoritative_package_version() {
        let ui = include_str!("../ui/main.slint");
        let gui = std::fs::read_to_string("src/bin/gui.rs").expect("GUI source is available");
        assert!(!ui.contains("Rustrepo Sanitizer 0.4.0"));
        assert!(gui.contains("env!(\"CARGO_PKG_VERSION\")"));
        assert!(gui.contains("set_app_title"));
    }

    #[test]
    fn gui_documentation_describes_reproducible_build_date_metadata() {
        let docs = std::fs::read_to_string("docs/gui.md").expect("GUI documentation is available");
        assert!(docs.contains("SOURCE_DATE_EPOCH"));
        assert!(docs.contains("reproducible"));
    }

    #[test]
    fn forgejo_ci_declares_native_gui_semantic_test() {
        let workflow = std::fs::read_to_string(".forgejo/workflows/ci.yml")
            .expect("Forgejo CI workflow is available");
        assert!(workflow.contains("./scripts/gui-test"));
    }

    #[test]
    fn forgejo_gui_dependencies_install_noninteractively() {
        let workflow = include_str!("../.forgejo/workflows/ci.yml");
        assert!(workflow.contains("DEBIAN_FRONTEND=noninteractive apt-get update"));
        assert!(workflow.contains("DEBIAN_FRONTEND=noninteractive apt-get install -y"));
        assert!(workflow.contains("xvfb"));
        assert!(workflow.contains("xauth"));
        assert!(workflow.contains("systemd"));
    }

    #[test]
    fn native_gui_ci_verifies_maximize_and_restore() {
        let workflow = include_str!("../.forgejo/workflows/ci.yml");
        let harness =
            std::fs::read_to_string("scripts/gui-test").expect("GUI harness is available");
        assert!(workflow.contains("x11-utils"));
        assert!(harness.contains("_NET_WM_STATE_MAXIMIZED_VERT"));
        assert!(harness.contains("_NET_WM_STATE_MAXIMIZED_HORZ"));
    }

    #[test]
    fn gui_harness_traces_nested_dbus_session() {
        let harness =
            std::fs::read_to_string("scripts/gui-test").expect("GUI harness is available");
        assert!(harness.contains("bash -x \"$0\""));
    }

    #[test]
    fn current_release_metadata_targets_0_4_2() {
        let manifest = include_str!("../Cargo.toml");
        assert!(manifest.contains("version = \"0.4.2\""));
    }

    #[test]
    fn gui_compression_model_is_not_hard_coded_in_slint() {
        let ui = include_str!("../ui/main.slint");
        assert!(ui.contains("compression-options"));
        assert!(ui.contains("archive-changed"));
        assert!(!ui.contains("[\"gzip\", \"zstd\"]"));
    }

    #[test]
    fn gui_refreshes_output_extension_when_capability_changes() {
        let ui = include_str!("../ui/main.slint");
        let gui = std::fs::read_to_string("src/bin/gui.rs").expect("GUI source is available");
        assert!(ui.contains("compression-changed"));
        assert!(ui.contains("timestamp-changed"));
        assert!(gui.contains("set_extension"));
        assert!(gui.contains("output_extension"));
    }

    #[test]
    fn gui_does_not_silently_replace_invalid_compression_selection() {
        let gui = std::fs::read_to_string("src/bin/gui.rs").expect("GUI source is available");
        assert!(!gui.contains("compression_for_gui_selection(format, compression_index as usize)\n                .unwrap_or(Compression::Zstd)"));
    }

    #[test]
    fn gui_clears_retained_password_when_archive_changes() {
        let ui = include_str!("../ui/main.slint");
        assert!(ui.contains("root.zip-password = \"\""));
    }

    #[test]
    fn seven_zip_gui_uses_truthful_owned_compression_label() {
        let gui = std::fs::read_to_string("src/bin/gui.rs").expect("GUI source is available");
        assert!(gui.contains("ArchiveFormat::SevenZip"));
        assert!(gui.contains("\"7z\""));
    }

    #[test]
    fn linux_gui_packaging_declares_desktop_identity() {
        let desktop = std::fs::read_to_string("packaging/io.itsulu.RustrepoSanitizer.desktop")
            .expect("desktop entry is available");
        let flatpak = std::fs::read_to_string("packaging/io.itsulu.RustrepoSanitizer.yml")
            .expect("Flatpak manifest is available");
        assert!(desktop.contains("Exec=rustrepo-sanitizer-gui"));
        assert!(desktop.contains("Icon=io.itsulu.RustrepoSanitizer"));
        assert!(desktop.contains("StartupWMClass=rustrepo-sanitizer-gui"));
        assert!(flatpak.contains("command: rustrepo-sanitizer-gui"));
        assert!(flatpak.contains("/app/share/applications/io.itsulu.RustrepoSanitizer.desktop"));
    }

    #[test]
    fn flatpak_builds_binaries_before_installing_them() {
        let flatpak = std::fs::read_to_string("packaging/io.itsulu.RustrepoSanitizer.yml")
            .expect("Flatpak manifest is available");
        assert!(flatpak.contains("cargo build --release --locked --bin itsulu-repo-sanitizer"));
        assert!(flatpak.contains(
            "cargo build --release --locked --features gui --bin rustrepo-sanitizer-gui"
        ));
    }

    #[test]
    fn xa11y_harness_covers_menu_and_cancellation_controls() {
        let harness = std::fs::read_to_string("tests/gui_xa11y.rs")
            .expect("semantic GUI harness is available");
        for label in ["Settings", "About", "Cancel sanitization"] {
            assert!(harness.contains(label), "xa11y harness must cover {label}");
        }
    }

    #[test]
    fn xa11y_harness_discovers_versioned_gui_title() {
        let harness =
            std::fs::read_to_string("scripts/gui-test").expect("GUI harness must be readable");
        assert!(
            harness.contains("search --name 'Rustrepo Sanitizer.*'"),
            "GUI harness must discover the versioned native window title"
        );
    }

    #[test]
    fn xa11y_harness_waits_for_native_window_creation() {
        let harness =
            std::fs::read_to_string("scripts/gui-test").expect("GUI harness must be readable");
        assert!(
            harness.contains("for attempt in {1..20}"),
            "GUI harness must poll for the native window instead of assuming a fixed startup delay"
        );
    }

    #[test]
    fn xa11y_harness_reports_gui_startup_log_when_window_is_missing() {
        let harness =
            std::fs::read_to_string("scripts/gui-test").expect("GUI harness must be readable");
        assert!(
            harness.contains("cat /tmp/rustrepo-sanitizer-gui.log"),
            "GUI harness must print the startup log when no native window is discoverable"
        );
    }

    #[test]
    fn native_gui_harness_can_override_repository_without_editing_ui_source() {
        let gui = std::fs::read_to_string("src/bin/gui.rs").expect("GUI source is available");
        assert!(gui.contains("RRS_GUI_REPOSITORY"));
    }

    #[test]
    fn manually_edited_gui_output_path_disables_automatic_extension_updates() {
        let ui = std::fs::read_to_string("ui/main.slint").expect("GUI source is available");
        assert!(ui.contains("user-edited"));
        assert!(ui.contains("output-path-automatic = false"));
    }
}
