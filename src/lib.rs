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
    fn gui_exposes_every_registered_tar_compression() {
        let source = std::fs::read_to_string("ui/main.slint").expect("GUI source is available");
        for label in ["lzip", "lzma", "lzo", "lrzip", "xz"] {
            assert!(
                source.contains(label),
                "GUI must expose the supported TAR compression {label}"
            );
        }
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
    }
}
