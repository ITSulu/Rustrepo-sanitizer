//! Security primitives for filtering untrusted repositories.
//!
//! The functions in this module deliberately do not return the secret material
//! they identify.  Callers can safely use the returned counts in reports.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExclusionReason {
    GitMetadata,
    PrivateKey,
    KubeConfig,
    CredentialFile,
    EnvironmentFile,
    Database,
    Snapshot,
    Binary,
    CacheOrBuildOutput,
}

impl fmt::Display for ExclusionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::GitMetadata => "git metadata",
            Self::PrivateKey => "private key",
            Self::KubeConfig => "kubeconfig",
            Self::CredentialFile => "credential file",
            Self::EnvironmentFile => "environment secret file",
            Self::Database => "database or dump",
            Self::Snapshot => "snapshot",
            Self::Binary => "binary blob",
            Self::CacheOrBuildOutput => "cache or build output",
        };
        f.write_str(text)
    }
}

/// Classifies path-only exclusions.  This is intentionally conservative: a
/// caller may add user exclusions, but should not override a result from here.
pub fn default_exclusion(path: &Path) -> Option<ExclusionReason> {
    let components: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    let file = components.last()?.as_str();

    if components.iter().any(|part| part == ".git") {
        return Some(ExclusionReason::GitMetadata);
    }
    if components.iter().any(|part| {
        matches!(
            part.as_str(),
            "target" | "node_modules" | ".cache" | "__pycache__"
        )
    }) {
        return Some(ExclusionReason::CacheOrBuildOutput);
    }
    if file == "kubeconfig" || file == "config" && components.iter().any(|part| part == ".kube") {
        return Some(ExclusionReason::KubeConfig);
    }
    if file == ".env" || file.starts_with(".env.") || file.ends_with(".env") {
        return Some(ExclusionReason::EnvironmentFile);
    }
    if matches!(file, "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519")
        || [".pem", ".key", ".p12", ".pfx", ".jks", ".kdb"]
            .iter()
            .any(|extension| file.ends_with(extension))
    {
        return Some(ExclusionReason::PrivateKey);
    }
    if file.contains("credential")
        || file.contains("passwd")
        || file.contains("password")
        || file.ends_with(".netrc")
        || file == "tokens.json"
    {
        return Some(ExclusionReason::CredentialFile);
    }
    if [".db", ".sqlite", ".sqlite3", ".sql", ".dump", ".bak"]
        .iter()
        .any(|extension| file.ends_with(extension))
    {
        return Some(ExclusionReason::Database);
    }
    if [".snap", ".snapshot"]
        .iter()
        .any(|extension| file.ends_with(extension))
    {
        return Some(ExclusionReason::Snapshot);
    }
    if [
        ".zip", ".gz", ".xz", ".zst", ".7z", ".rar", ".pdf", ".png", ".jpg", ".jpeg", ".gif",
        ".webp", ".mp3", ".mp4", ".woff", ".woff2", ".o", ".so", ".dll", ".exe",
    ]
    .iter()
    .any(|extension| file.ends_with(extension))
    {
        return Some(ExclusionReason::Binary);
    }
    None
}

/// Returns true for bytes which should not be interpreted as text.  A NUL byte
/// is a reliable binary marker and avoids false positives for UTF-8 source.
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

#[derive(Debug, Eq, PartialEq)]
pub enum PathSafetyError {
    Absolute,
    Traversal,
    Empty,
    NonUtf8,
}

/// Produces a portable archive member name and rejects paths which could write
/// outside an extraction directory.
pub fn safe_archive_path(path: &Path) -> Result<String, PathSafetyError> {
    if path.is_absolute() {
        return Err(PathSafetyError::Absolute);
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str().ok_or(PathSafetyError::NonUtf8)?),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PathSafetyError::Traversal)
            }
        }
    }
    if parts.is_empty() {
        return Err(PathSafetyError::Empty);
    }
    Ok(parts.join("/"))
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct RedactionResult {
    pub text: String,
    pub counts: BTreeMap<&'static str, usize>,
}

/// Redacts values assigned to sensitive configuration keys and common token
/// shapes.  The original values are never retained in the result.
pub fn redact_text(input: &str) -> RedactionResult {
    let mut result = RedactionResult::default();
    for segment in input.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |s| (s, "\n"));
        let (redacted, kind) = redact_line(line);
        result.text.push_str(&redacted);
        result.text.push_str(newline);
        if let Some(kind) = kind {
            *result.counts.entry(kind).or_default() += 1;
        }
    }
    result
}

fn redact_line(line: &str) -> (String, Option<&'static str>) {
    let lowered = line.to_ascii_lowercase();
    if lowered.contains("-----begin ") && lowered.contains("private key-----") {
        return (
            "[REDACTED PRIVATE KEY MATERIAL]".to_owned(),
            Some("private_key"),
        );
    }
    for separator in ['=', ':'] {
        if let Some(position) = line.find(separator) {
            let key = line[..position]
                .trim()
                .trim_matches(['\'', '"'])
                .to_ascii_lowercase();
            // References name a secret but do not contain one.
            if key.contains("secretkeyref") || key.ends_with("_ref") || key.ends_with("_name") {
                continue;
            }
            if [
                "password",
                "passwd",
                "token",
                "api_key",
                "apikey",
                "client_secret",
                "private_key",
                "access_key",
                "secret_key",
            ]
            .iter()
            .any(|needle| key.contains(needle))
            {
                let value = line[position + separator.len_utf8()..].trim();
                if !value.is_empty() && !value.starts_with("${") {
                    return (
                        format!("{}{} [REDACTED]", &line[..position], separator),
                        Some("assigned_value"),
                    );
                }
            }
        }
    }
    if line.split_whitespace().any(looks_like_token) {
        return ("[REDACTED TOKEN]".to_owned(), Some("token"));
    }
    (line.to_owned(), None)
}

fn looks_like_token(word: &str) -> bool {
    let token = word.trim_matches(|character: char| {
        !character.is_ascii_alphanumeric()
            && character != '.'
            && character != '_'
            && character != '-'
    });
    (token.starts_with("ghp_") && token.len() >= 20)
        || (token.starts_with("github_pat_") && token.len() >= 20)
        || (token.starts_with("AKIA")
            && token.len() == 20
            && token[4..].chars().all(|c| c.is_ascii_alphanumeric()))
        || (token.split('.').count() == 3 && token.len() > 30)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn excludes_sensitive_and_generated_paths() {
        assert_eq!(
            default_exclusion(Path::new(".git/config")),
            Some(ExclusionReason::GitMetadata)
        );
        assert_eq!(
            default_exclusion(Path::new("deploy/.env.production")),
            Some(ExclusionReason::EnvironmentFile)
        );
        assert_eq!(
            default_exclusion(Path::new("secrets/server.pem")),
            Some(ExclusionReason::PrivateKey)
        );
        assert_eq!(
            default_exclusion(Path::new("var/state.sqlite3")),
            Some(ExclusionReason::Database)
        );
        assert_eq!(
            default_exclusion(Path::new("target/release/app")),
            Some(ExclusionReason::CacheOrBuildOutput)
        );
        assert_eq!(default_exclusion(Path::new("docs/guide.md")), None);
    }

    #[test]
    fn redacts_values_but_not_symbolic_secret_references() {
        let source =
            "password: fake-value-for-test\nsecretKeyRef: database-password\nTOKEN=${TOKEN}\n";
        let result = redact_text(source);
        assert!(!result.text.contains("fake-value-for-test"));
        assert!(result.text.contains("password: [REDACTED]"));
        assert!(result.text.contains("secretKeyRef: database-password"));
        assert!(result.text.contains("TOKEN=${TOKEN}"));
        assert_eq!(result.counts.get("assigned_value"), Some(&1));
    }

    #[test]
    fn redacts_known_token_without_echoing_it() {
        let secret = "ghp_123456789012345678901234567890123456";
        let result = redact_text(&format!("value {secret}"));
        assert!(!result.text.contains(secret));
        assert_eq!(result.text, "[REDACTED TOKEN]");
    }

    #[test]
    fn archive_paths_cannot_escape() {
        assert_eq!(
            safe_archive_path(Path::new("src/../src/lib.rs")),
            Err(PathSafetyError::Traversal)
        );
        assert_eq!(
            safe_archive_path(Path::new("/etc/passwd")),
            Err(PathSafetyError::Absolute)
        );
        assert_eq!(
            safe_archive_path(Path::new("./src/lib.rs")),
            Ok("src/lib.rs".to_owned())
        );
    }

    #[test]
    fn binary_detection_is_conservative() {
        assert!(is_binary(b"hello\0world"));
        assert!(!is_binary("valid UTF-8 \u{1f980}".as_bytes()));
    }
}
