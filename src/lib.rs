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
        label: "ZIP password protection",
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
}
