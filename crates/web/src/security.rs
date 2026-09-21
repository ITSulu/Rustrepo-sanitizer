//! Security primitives for the untrusted web frontend.
//!
//! Nothing here trusts the browser: repository URLs, uploads, archive members,
//! and output names are all validated before the server touches the filesystem
//! or spawns a subprocess.

use std::io::Read;
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};

use url::Url;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SecurityError {
    #[error("only https git URLs are allowed")]
    UnsupportedScheme,
    #[error("git URLs must not embed credentials")]
    EmbeddedCredentials,
    #[error("git host is a private, loopback, or reserved address")]
    PrivateHost,
    #[error("invalid repository URL")]
    InvalidUrl,
    #[error("path escapes the workspace")]
    PathEscape,
    #[error("archive entry is unsafe: {0}")]
    UnsafeArchiveEntry(String),
    #[error("archive extraction exceeds the configured budget")]
    ArchiveTooLarge,
    #[error("archive contains a symbolic link")]
    Symlink,
    #[error("archive is invalid: {0}")]
    InvalidArchive(String),
}

/// Bounds an archive extraction so a hostile upload cannot exhaust the host.
#[derive(Clone, Copy, Debug)]
pub struct ExtractionBudget {
    pub max_files: usize,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
}

impl Default for ExtractionBudget {
    fn default() -> Self {
        Self {
            max_files: 100_000,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            max_file_bytes: 512 * 1024 * 1024,
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct ExtractionReport {
    pub files: usize,
    pub bytes: u64,
}

/// Returns true for addresses a server must never be coerced into contacting.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // Unique local (fc00::/7) and link-local (fe80::/10).
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|mapped| is_private_ip(IpAddr::V4(mapped)))
        }
    }
}

/// Validates a user-supplied Git repository URL for a server-side clone.
pub fn validate_git_url(raw: &str) -> Result<Url, SecurityError> {
    let url = Url::parse(raw).map_err(|_| SecurityError::InvalidUrl)?;
    if url.scheme() != "https" {
        return Err(SecurityError::UnsupportedScheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(SecurityError::EmbeddedCredentials);
    }
    let host = url.host_str().ok_or(SecurityError::InvalidUrl)?;
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return Err(SecurityError::PrivateHost);
    }
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(SecurityError::PrivateHost);
        }
    }
    Ok(url)
}

/// Rejects a resolved address set that includes any non-public address
/// (defends against DNS-based SSRF, including rebinding).
pub fn ensure_public_addrs(addrs: &[IpAddr]) -> Result<(), SecurityError> {
    if addrs.is_empty() {
        return Err(SecurityError::InvalidUrl);
    }
    if addrs.iter().any(|ip| is_private_ip(*ip)) {
        return Err(SecurityError::PrivateHost);
    }
    Ok(())
}

/// The `git clone` argv used by the server. Arguments are passed directly to
/// the process (never through a shell), and risky transports are disabled.
pub fn git_clone_argv(url: &Url, dest: &Path) -> Vec<String> {
    vec![
        "-c".into(),
        "protocol.ext.allow=never".into(),
        "-c".into(),
        "protocol.file.allow=never".into(),
        "clone".into(),
        "--no-tags".into(),
        "--single-branch".into(),
        url.as_str().into(),
        dest.display().to_string(),
    ]
}

/// Validates an untrusted output filename: no directories, no traversal.
pub fn safe_output_name(name: &str) -> Result<String, SecurityError> {
    let path = Path::new(name);
    if path.is_absolute() {
        return Err(SecurityError::PathEscape);
    }
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(part)), None) => {
            let part = part.to_str().ok_or(SecurityError::PathEscape)?;
            if part.is_empty() || part == "." || part == ".." {
                return Err(SecurityError::PathEscape);
            }
            Ok(part.to_owned())
        }
        _ => Err(SecurityError::PathEscape),
    }
}

/// Joins a validated relative path to a root, rejecting traversal.
pub fn contained_path(root: &Path, candidate: &str) -> Result<PathBuf, SecurityError> {
    let relative = safe_relative_path(candidate)?;
    Ok(root.join(relative))
}

/// Normalizes a repository-relative path (used for include/exclude globs and
/// upload member names) and rejects traversal components.
pub fn safe_relative_path(candidate: &str) -> Result<PathBuf, SecurityError> {
    if candidate.is_empty() {
        return Err(SecurityError::PathEscape);
    }
    let path = Path::new(candidate);
    if path.is_absolute() {
        return Err(SecurityError::PathEscape);
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(SecurityError::PathEscape)
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(SecurityError::PathEscape);
    }
    Ok(out)
}

fn unix_is_symlink(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| mode & 0o170000 == 0o120000)
}

fn entry_name_is_unsafe(name: &str) -> bool {
    if name.starts_with('/') || name.starts_with('\\') || name.contains('\\') {
        return true;
    }
    Path::new(name).components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    })
}

fn charge(
    budget: &ExtractionBudget,
    report: &mut ExtractionReport,
    size: u64,
) -> Result<(), SecurityError> {
    if size > budget.max_file_bytes {
        return Err(SecurityError::ArchiveTooLarge);
    }
    report.files = report.files.saturating_add(1);
    report.bytes = report.bytes.saturating_add(size);
    if report.files > budget.max_files || report.bytes > budget.max_total_bytes {
        return Err(SecurityError::ArchiveTooLarge);
    }
    Ok(())
}

/// Extracts a zip upload into `dest`, rejecting traversal, symlinks, and
/// oversized payloads.
pub fn extract_zip(
    archive: impl Read + std::io::Seek,
    dest: &Path,
    budget: ExtractionBudget,
) -> Result<ExtractionReport, SecurityError> {
    let mut zip = zip::ZipArchive::new(archive)
        .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
    let mut report = ExtractionReport::default();
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
        let name = entry.name().to_owned();
        if entry_name_is_unsafe(&name) {
            return Err(SecurityError::UnsafeArchiveEntry(name));
        }
        if unix_is_symlink(entry.unix_mode()) {
            return Err(SecurityError::Symlink);
        }
        let target = dest.join(safe_relative_path(&name)?);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
            continue;
        }
        charge(&budget, &mut report, entry.size())?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
        }
        let mut file = std::fs::File::create(&target)
            .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
        let mut limited = (&mut entry).take(budget.max_file_bytes + 1);
        let written = std::io::copy(&mut limited, &mut file)
            .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
        if written > budget.max_file_bytes {
            return Err(SecurityError::ArchiveTooLarge);
        }
    }
    Ok(report)
}

/// Extracts a tar upload (optionally gzip-compressed) into `dest` with the
/// same safety guarantees as [`extract_zip`].
pub fn extract_tar(
    archive: impl Read,
    dest: &Path,
    budget: ExtractionBudget,
) -> Result<ExtractionReport, SecurityError> {
    let mut tar = tar::Archive::new(archive);
    let mut report = ExtractionReport::default();
    let entries = tar
        .entries()
        .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
        let header = entry.header().clone();
        let kind = header.entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(SecurityError::Symlink);
        }
        let name = entry
            .path()
            .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?
            .to_path_buf();
        let name = name.to_string_lossy().into_owned();
        if entry_name_is_unsafe(&name) {
            return Err(SecurityError::UnsafeArchiveEntry(name));
        }
        let target = dest.join(safe_relative_path(&name)?);
        if kind.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
            continue;
        }
        if !kind.is_file() {
            return Err(SecurityError::UnsafeArchiveEntry(name));
        }
        let size = header.size().unwrap_or(0);
        charge(&budget, &mut report, size)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
        }
        let mut file = std::fs::File::create(&target)
            .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
        let mut limited = (&mut entry).take(budget.max_file_bytes + 1);
        let written = std::io::copy(&mut limited, &mut file)
            .map_err(|err| SecurityError::InvalidArchive(err.to_string()))?;
        if written > budget.max_file_bytes {
            return Err(SecurityError::ArchiveTooLarge);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rejects_non_https_and_credentials_and_private_hosts() {
        assert_eq!(
            validate_git_url("http://example.com/repo.git"),
            Err(SecurityError::UnsupportedScheme)
        );
        assert_eq!(
            validate_git_url("ssh://git@example.com/repo.git"),
            Err(SecurityError::UnsupportedScheme)
        );
        assert_eq!(
            validate_git_url("https://user:pass@example.com/repo.git"),
            Err(SecurityError::EmbeddedCredentials)
        );
        assert_eq!(
            validate_git_url("https://localhost/repo.git"),
            Err(SecurityError::PrivateHost)
        );
        assert_eq!(
            validate_git_url("https://127.0.0.1/repo.git"),
            Err(SecurityError::PrivateHost)
        );
        assert_eq!(
            validate_git_url("https://[::1]/repo.git"),
            Err(SecurityError::PrivateHost)
        );
        assert!(validate_git_url("https://git.example.com/org/repo.git").is_ok());
    }

    #[test]
    fn private_addresses_are_detected() {
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.4.4",
            "192.168.1.1",
            "169.254.1.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:10.0.0.1",
        ] {
            assert!(
                is_private_ip(ip.parse().unwrap()),
                "{ip} must be treated as private"
            );
        }
        for ip in ["1.1.1.1", "140.82.112.3", "2606:4700::1"] {
            assert!(
                !is_private_ip(ip.parse().unwrap()),
                "{ip} must be treated as public"
            );
        }
    }

    #[test]
    fn resolved_hosts_must_be_public() {
        assert_eq!(
            ensure_public_addrs(&["127.0.0.1".parse().unwrap()]),
            Err(SecurityError::PrivateHost)
        );
        assert_eq!(ensure_public_addrs(&[]), Err(SecurityError::InvalidUrl));
        assert!(ensure_public_addrs(&["1.1.1.1".parse().unwrap()]).is_ok());
    }

    #[test]
    fn clone_argv_disables_risky_transports() {
        let url = validate_git_url("https://git.example.com/org/repo.git").unwrap();
        let argv = git_clone_argv(&url, Path::new("/tmp/dest"));
        assert!(argv.contains(&"protocol.ext.allow=never".to_owned()));
        assert!(argv.contains(&"protocol.file.allow=never".to_owned()));
        assert_eq!(argv.last().unwrap(), "/tmp/dest");
    }

    #[test]
    fn output_names_reject_traversal() {
        assert_eq!(
            safe_output_name("review.tar.zst").unwrap(),
            "review.tar.zst"
        );
        assert_eq!(
            safe_output_name("../escape.tar"),
            Err(SecurityError::PathEscape)
        );
        assert_eq!(safe_output_name("a/b.tar"), Err(SecurityError::PathEscape));
        assert_eq!(safe_output_name("/abs.tar"), Err(SecurityError::PathEscape));
    }

    #[test]
    fn contained_path_rejects_escape() {
        let root = Path::new("/srv/job");
        assert_eq!(
            contained_path(root, "reports/report.md").unwrap(),
            PathBuf::from("/srv/job/reports/report.md")
        );
        assert_eq!(
            contained_path(root, "../../etc/passwd"),
            Err(SecurityError::PathEscape)
        );
    }

    #[test]
    fn zip_extraction_rejects_traversal_and_symlinks() {
        use std::io::Cursor;
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            writer
                .start_file("../escape.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"nope").unwrap();
            writer.finish().unwrap();
        }
        let dest = tempfile::tempdir().unwrap();
        assert_eq!(
            extract_zip(
                Cursor::new(&buffer),
                dest.path(),
                ExtractionBudget::default()
            ),
            Err(SecurityError::UnsafeArchiveEntry("../escape.txt".into()))
        );

        // Absolute path entry (the writer strips nothing, so this is a real
        // hostile archive).
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            writer
                .start_file("/etc/passwd", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"root").unwrap();
            writer.finish().unwrap();
        }
        assert_eq!(
            extract_zip(
                Cursor::new(&buffer),
                dest.path(),
                ExtractionBudget::default()
            ),
            Err(SecurityError::UnsafeArchiveEntry("/etc/passwd".into()))
        );
    }

    #[test]
    fn unix_symlink_modes_are_detected() {
        assert!(unix_is_symlink(Some(0o120777)));
        assert!(!unix_is_symlink(Some(0o100644)));
        assert!(!unix_is_symlink(None));
    }

    #[test]
    fn zip_extraction_enforces_budget_and_writes_files() {
        use std::io::Cursor;
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            writer
                .start_file("src/lib.rs", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"fn main() {}").unwrap();
            writer.finish().unwrap();
        }
        let dest = tempfile::tempdir().unwrap();
        let report = extract_zip(
            Cursor::new(&buffer),
            dest.path(),
            ExtractionBudget::default(),
        )
        .unwrap();
        assert_eq!(report.files, 1);
        assert!(dest.path().join("src/lib.rs").is_file());

        let strict = ExtractionBudget {
            max_files: 1,
            max_total_bytes: 4,
            max_file_bytes: 4,
        };
        assert_eq!(
            extract_zip(Cursor::new(&buffer), dest.path(), strict),
            Err(SecurityError::ArchiveTooLarge)
        );
    }

    #[test]
    fn tar_extraction_rejects_links_and_traversal() {
        let dest = tempfile::tempdir().unwrap();

        let mut buffer = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut buffer);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_cksum();
            builder
                .append_link(&mut header, "link", "/etc/passwd")
                .unwrap();
            builder.finish().unwrap();
        }
        assert_eq!(
            extract_tar(&buffer[..], dest.path(), ExtractionBudget::default()),
            Err(SecurityError::Symlink)
        );

        let mut buffer = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut buffer);
            let data = b"escape";
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            // The writer validates `append_data` paths, so set the hostile name
            // directly in the header to produce a genuinely malicious archive.
            let name = b"../escape.txt";
            header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name);
            header.set_cksum();
            builder.append(&header, &data[..]).unwrap();
            builder.finish().unwrap();
        }
        assert_eq!(
            extract_tar(&buffer[..], dest.path(), ExtractionBudget::default()),
            Err(SecurityError::UnsafeArchiveEntry("../escape.txt".into()))
        );
    }

    #[test]
    fn tar_extraction_succeeds_for_safe_entries() {
        let dest = tempfile::tempdir().unwrap();
        let mut buffer = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut buffer);
            let data = b"hello";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "a/b.txt", &data[..])
                .unwrap();
            builder.finish().unwrap();
        }
        let report = extract_tar(&buffer[..], dest.path(), ExtractionBudget::default()).unwrap();
        assert_eq!(report.files, 1);
        assert!(dest.path().join("a/b.txt").is_file());
    }
}
