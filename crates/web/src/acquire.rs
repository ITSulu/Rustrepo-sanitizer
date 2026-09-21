//! Resolves each of the five repository input modes into a local, isolated
//! checkout inside a job workspace.
//!
//! All modes end at the same place: a directory containing a Git repository
//! that the shared sanitizer core can read. Clone execution and DNS resolution
//! are injected so the security decisions are unit-testable without a network.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures_util::future::BoxFuture;

use crate::dto::InputSpec;
use crate::integrations::Integrations;
use crate::security::{
    ensure_public_addrs, extract_tar, extract_zip, git_clone_argv, validate_git_url,
    ExtractionBudget, SecurityError,
};
use crate::uploads::UploadStore;

#[derive(Debug, thiserror::Error)]
pub enum AcquireError {
    #[error("local path mode is disabled or the path is outside the allowed roots")]
    LocalPathNotAllowed,
    #[error("the requested repository was not found")]
    NotFound,
    #[error("the directory is not a Git repository")]
    NotARepository,
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error("clone failed: {0}")]
    Clone(String),
    #[error("uploaded repository was not found or has expired")]
    UploadNotFound,
    #[error("repository exceeds the configured size limit")]
    TooLarge,
}

/// Executes `git` with structured arguments (never a shell).
pub trait CloneRunner: Send + Sync + 'static {
    fn clone(
        &self,
        argv: Vec<String>,
        env: Vec<(String, String)>,
        dest: PathBuf,
    ) -> BoxFuture<'static, Result<(), String>>;
}

/// Resolves a hostname to addresses so SSRF checks can run before cloning.
pub trait HostResolver: Send + Sync + 'static {
    fn resolve(&self, host: String) -> BoxFuture<'static, Result<Vec<IpAddr>, String>>;
}

pub struct SystemCloneRunner {
    pub timeout: Duration,
}

impl CloneRunner for SystemCloneRunner {
    fn clone(
        &self,
        argv: Vec<String>,
        env: Vec<(String, String)>,
        dest: PathBuf,
    ) -> BoxFuture<'static, Result<(), String>> {
        let timeout = self.timeout;
        Box::pin(async move {
            let mut command = tokio::process::Command::new("git");
            command.args(&argv);
            command.env("GIT_TERMINAL_PROMPT", "0");
            command.env("GIT_ASKPASS", "");
            command.env("GIT_SSH_COMMAND", "false");
            command.env("GIT_CONFIG_NOSYSTEM", "1");
            command.env("GIT_CONFIG_GLOBAL", "/dev/null");
            // Credentials are passed through git's environment config so they
            // never appear in argv.
            for (key, value) in env {
                command.env(key, value);
            }
            command.kill_on_drop(true);
            command.stdin(std::process::Stdio::null());
            let output = tokio::time::timeout(timeout, command.output())
                .await
                .map_err(|_| "clone timed out".to_owned())?
                .map_err(|err| format!("failed to run git: {err}"))?;
            if !output.status.success() {
                return Err(format!(
                    "git clone exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            if !dest.join(".git").exists() {
                return Err("clone did not produce a Git repository".to_owned());
            }
            Ok(())
        })
    }
}

pub struct SystemResolver;

impl HostResolver for SystemResolver {
    fn resolve(&self, host: String) -> BoxFuture<'static, Result<Vec<IpAddr>, String>> {
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 443u16))
                .await
                .map_err(|err| format!("could not resolve {host}: {err}"))?
                .map(|socket| socket.ip())
                .collect::<Vec<_>>();
            Ok(addresses)
        })
    }
}

pub struct Acquirer {
    pub runner: Arc<dyn CloneRunner>,
    pub resolver: Arc<dyn HostResolver>,
    pub uploads: Arc<UploadStore>,
    pub integrations: Arc<Integrations>,
    pub allowed_local_roots: Vec<PathBuf>,
    pub max_repo_bytes: u64,
    pub extraction_budget: ExtractionBudget,
}

impl Acquirer {
    pub async fn acquire(&self, spec: &InputSpec, dest: &Path) -> Result<PathBuf> {
        match spec {
            InputSpec::LocalPath { path } => self.acquire_local(path, dest),
            InputSpec::GitUrl { url } => self.acquire_url(url, dest).await,
            InputSpec::Upload { upload_id } => self.acquire_upload(upload_id, dest),
            InputSpec::Forgejo {
                owner,
                repo,
                git_ref,
            } => {
                let url = self
                    .integrations
                    .forgejo_clone_url(owner, repo)
                    .map_err(|err| AcquireError::Invalid(err.to_string()))?;
                let auth = self
                    .integrations
                    .forgejo_token()
                    .map(|token| format!("Authorization: token {token}"));
                self.acquire_authenticated(url, auth, git_ref.as_deref(), dest)
                    .await
            }
            InputSpec::GitHub {
                owner,
                repo,
                git_ref,
            } => {
                let url = Integrations::github_clone_url(owner, repo);
                let auth = self
                    .integrations
                    .github_token()
                    .map(|token| format!("Authorization: Bearer {token}"));
                self.acquire_authenticated(url, auth, git_ref.as_deref(), dest)
                    .await
            }
        }
    }

    fn acquire_local(&self, raw: &str, _dest: &Path) -> Result<PathBuf> {
        let candidate = PathBuf::from(raw);
        if !candidate.is_absolute() {
            return Err(AcquireError::LocalPathNotAllowed.into());
        }
        let canonical = std::fs::canonicalize(&candidate).map_err(|_| AcquireError::NotFound)?;
        let allowed = self.allowed_local_roots.iter().any(|root| {
            std::fs::canonicalize(root)
                .map(|root| canonical.starts_with(root))
                .unwrap_or(false)
        });
        if !allowed {
            return Err(AcquireError::LocalPathNotAllowed.into());
        }
        if !is_git_repository(&canonical) {
            return Err(AcquireError::NotARepository.into());
        }
        Ok(canonical)
    }

    async fn acquire_url(&self, raw: &str, dest: &Path) -> Result<PathBuf> {
        let url = validate_git_url(raw).map_err(AcquireError::Security)?;
        let host = url
            .host_str()
            .ok_or(AcquireError::Invalid("missing host".into()))?
            .to_owned();
        let addresses = self
            .resolver
            .resolve(host)
            .await
            .map_err(AcquireError::Invalid)?;
        ensure_public_addrs(&addresses).map_err(AcquireError::Security)?;
        self.clone_into(&url, None, None, dest).await
    }

    async fn acquire_authenticated(
        &self,
        url: url::Url,
        auth: Option<String>,
        git_ref: Option<&str>,
        dest: &Path,
    ) -> Result<PathBuf> {
        let host = url
            .host_str()
            .ok_or(AcquireError::Invalid("missing host".into()))?
            .to_owned();
        let addresses = self
            .resolver
            .resolve(host)
            .await
            .map_err(AcquireError::Invalid)?;
        ensure_public_addrs(&addresses).map_err(AcquireError::Security)?;
        self.clone_into(&url, auth, git_ref, dest).await
    }

    async fn clone_into(
        &self,
        url: &url::Url,
        auth: Option<String>,
        git_ref: Option<&str>,
        dest: &Path,
    ) -> Result<PathBuf> {
        std::fs::create_dir_all(dest).context("creating checkout directory")?;
        let mut argv = git_clone_argv(url, dest);
        if let Some(git_ref) = git_ref {
            let git_ref = validate_git_ref(git_ref)?;
            argv.insert(argv.len() - 1, "--branch".to_owned());
            argv.insert(argv.len() - 1, git_ref);
        }
        let env = auth
            .map(|header| {
                vec![
                    ("GIT_CONFIG_COUNT".to_owned(), "1".to_owned()),
                    ("GIT_CONFIG_KEY_0".to_owned(), "http.extraHeader".to_owned()),
                    ("GIT_CONFIG_VALUE_0".to_owned(), header),
                ]
            })
            .unwrap_or_default();
        self.runner
            .as_ref()
            .clone(argv, env, dest.to_path_buf())
            .await
            .map_err(AcquireError::Clone)?;
        let size = directory_size(dest);
        if size > self.max_repo_bytes {
            let _ = std::fs::remove_dir_all(dest);
            return Err(AcquireError::TooLarge.into());
        }
        Ok(dest.to_path_buf())
    }

    fn acquire_upload(&self, upload_id: &str, dest: &Path) -> Result<PathBuf> {
        let entry = self
            .uploads
            .get(upload_id)
            .ok_or(AcquireError::UploadNotFound)?;
        std::fs::create_dir_all(dest).context("creating upload workspace")?;
        match entry.kind {
            crate::uploads::UploadKind::Archive => {
                let file = std::fs::File::open(&entry.path).context("opening upload")?;
                let name = entry.path.to_string_lossy().to_ascii_lowercase();
                if name.ends_with(".zip") {
                    extract_zip(file, dest, self.extraction_budget)?;
                } else if name.ends_with(".tar")
                    || name.ends_with(".tar.gz")
                    || name.ends_with(".tgz")
                {
                    if name.ends_with(".tar") {
                        extract_tar(file, dest, self.extraction_budget)?;
                    } else {
                        extract_tar(
                            flate2::read::GzDecoder::new(file),
                            dest,
                            self.extraction_budget,
                        )?;
                    }
                } else {
                    return Err(AcquireError::Invalid("unsupported archive type".into()).into());
                }
                let dir = find_repository_root(dest).ok_or(AcquireError::NotARepository)?;
                Ok(dir)
            }
            crate::uploads::UploadKind::Directory => {
                let canonical = std::fs::canonicalize(&entry.path)?;
                if !canonical.starts_with(self.uploads.root()) {
                    return Err(AcquireError::Invalid("upload escaped its store".into()).into());
                }
                if !is_git_repository(&canonical) {
                    return Err(AcquireError::NotARepository.into());
                }
                Ok(canonical)
            }
        }
    }
}

/// Validates a Git ref/branch name to prevent option injection into `git`.
pub fn validate_git_ref(git_ref: &str) -> Result<String> {
    if git_ref.is_empty() || git_ref.starts_with('-') || git_ref.len() > 255 {
        bail!("invalid git ref");
    }
    if git_ref.contains("..")
        || git_ref.contains('\\')
        || git_ref.contains(' ')
        || git_ref.contains('~')
        || git_ref.contains('^')
        || git_ref.contains(':')
        || git_ref.contains('?')
        || git_ref.contains('*')
        || git_ref.contains('[')
    {
        bail!("invalid git ref");
    }
    Ok(git_ref.to_owned())
}

/// Finds a Git repository root within an extracted upload, allowing one level
/// of common wrappers (for example a `repo-main/` directory in a tarball).
pub fn find_repository_root(dest: &Path) -> Option<PathBuf> {
    if is_git_repository(dest) {
        return Some(dest.to_path_buf());
    }
    let entries = std::fs::read_dir(dest).ok()?;
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        if is_real_dir(&entry.path()) && is_git_repository(&entry.path()) {
            candidates.push(entry.path());
        }
    }
    if candidates.len() == 1 {
        candidates.pop()
    } else {
        None
    }
}

/// A path is a directory only if it is not a symlink (prevents symlink escape).
fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.is_dir())
        .unwrap_or(false)
}

/// True when `path/.git` is a real directory, not a symlinked or "gitfile"
/// pointer that could redirect Git outside the workspace.
pub fn is_git_repository(path: &Path) -> bool {
    is_real_dir(&path.join(".git"))
}

pub fn directory_size(path: &Path) -> u64 {
    fn walk(path: &Path, total: &mut u64) {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                // Use symlink_metadata so a symlink (e.g. `loop -> .`) is never
                // followed into an infinite recursion or outside the workspace.
                if let Ok(meta) = std::fs::symlink_metadata(entry.path()) {
                    if meta.file_type().is_symlink() {
                        continue;
                    }
                    if meta.is_dir() {
                        walk(&entry.path(), total);
                    } else if meta.is_file() {
                        *total = total.saturating_add(meta.len());
                    }
                }
            }
        }
    }
    let mut total = 0;
    walk(path, &mut total);
    total
}

/// Ensures a decoded upload name is a safe single path segment.
pub fn validate_upload_name(name: &str) -> Result<String, SecurityError> {
    crate::security::safe_output_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::InputSpec;
    use crate::integrations::IntegrationsConfig;
    use std::net::IpAddr;

    struct FakeRunner;
    impl CloneRunner for FakeRunner {
        fn clone(
            &self,
            _argv: Vec<String>,
            _env: Vec<(String, String)>,
            _dest: PathBuf,
        ) -> BoxFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }

    struct PrivateResolver;
    impl HostResolver for PrivateResolver {
        fn resolve(&self, _host: String) -> BoxFuture<'static, Result<Vec<IpAddr>, String>> {
            Box::pin(async { Ok(vec!["127.0.0.1".parse().unwrap()]) })
        }
    }

    struct PublicResolver;
    impl HostResolver for PublicResolver {
        fn resolve(&self, _host: String) -> BoxFuture<'static, Result<Vec<IpAddr>, String>> {
            Box::pin(async { Ok(vec!["1.1.1.1".parse().unwrap()]) })
        }
    }

    fn acquirer(
        roots: Vec<PathBuf>,
        resolver: Arc<dyn HostResolver>,
        uploads: Arc<UploadStore>,
    ) -> Acquirer {
        Acquirer {
            runner: Arc::new(FakeRunner),
            resolver,
            uploads,
            integrations: Arc::new(Integrations::new(IntegrationsConfig::default())),
            allowed_local_roots: roots,
            max_repo_bytes: 1024 * 1024,
            extraction_budget: ExtractionBudget::default(),
        }
    }

    fn git_init(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(path)
            .status()
            .unwrap();
    }

    #[tokio::test]
    async fn local_paths_must_be_inside_allowed_roots() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        git_init(&repo);
        let outside = tempfile::tempdir().unwrap();
        git_init(&outside.path().join("other"));
        let uploads =
            Arc::new(UploadStore::new(tempfile::tempdir().unwrap().path().to_path_buf()).unwrap());
        let acq = acquirer(
            vec![root.path().to_path_buf()],
            Arc::new(PublicResolver),
            uploads,
        );

        let dest = tempfile::tempdir().unwrap();
        assert!(acq
            .acquire(
                &InputSpec::LocalPath {
                    path: repo.to_string_lossy().into_owned()
                },
                dest.path(),
            )
            .await
            .is_ok());
        let outside_path = outside.path().join("other");
        let err = acq
            .acquire(
                &InputSpec::LocalPath {
                    path: outside_path.to_string_lossy().into_owned(),
                },
                dest.path(),
            )
            .await
            .err()
            .unwrap();
        assert!(matches!(
            err.downcast_ref::<AcquireError>(),
            Some(AcquireError::LocalPathNotAllowed)
        ));
        assert!(acq
            .acquire(
                &InputSpec::LocalPath {
                    path: "relative".into()
                },
                dest.path()
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn url_mode_rejects_private_hosts_before_cloning() {
        let uploads =
            Arc::new(UploadStore::new(tempfile::tempdir().unwrap().path().to_path_buf()).unwrap());
        let acq = acquirer(vec![], Arc::new(PrivateResolver), uploads);
        let dest = tempfile::tempdir().unwrap();
        let err = acq
            .acquire(
                &InputSpec::GitUrl {
                    url: "https://git.example.com/org/repo.git".into(),
                },
                dest.path(),
            )
            .await
            .err()
            .unwrap();
        assert!(matches!(
            err.downcast_ref::<AcquireError>(),
            Some(AcquireError::Security(SecurityError::PrivateHost))
        ));
    }

    #[tokio::test]
    async fn url_mode_accepts_public_https() {
        let uploads =
            Arc::new(UploadStore::new(tempfile::tempdir().unwrap().path().to_path_buf()).unwrap());
        let acq = acquirer(vec![], Arc::new(PublicResolver), uploads);
        let dest = tempfile::tempdir().unwrap();
        let repo = acq
            .acquire(
                &InputSpec::GitUrl {
                    url: "https://git.example.com/org/repo.git".into(),
                },
                dest.path(),
            )
            .await
            .unwrap();
        assert_eq!(repo, dest.path());
    }

    #[tokio::test]
    async fn http_and_private_ip_urls_are_rejected() {
        let uploads =
            Arc::new(UploadStore::new(tempfile::tempdir().unwrap().path().to_path_buf()).unwrap());
        let acq = acquirer(vec![], Arc::new(PublicResolver), uploads);
        let dest = tempfile::tempdir().unwrap();
        for bad in [
            "http://git.example.com/org/repo.git",
            "https://127.0.0.1/org/repo.git",
            "https://user:pass@git.example.com/org/repo.git",
        ] {
            assert!(acq
                .acquire(&InputSpec::GitUrl { url: bad.into() }, dest.path())
                .await
                .is_err());
        }
    }

    #[test]
    fn git_ref_validation_blocks_option_injection() {
        assert!(validate_git_ref("main").is_ok());
        assert!(validate_git_ref("v1.2.3").is_ok());
        for bad in ["--upload-pack=evil", "main..other", "a b", "a~1", "a:b"] {
            assert!(validate_git_ref(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn upload_names_are_single_safe_segments() {
        assert_eq!(validate_upload_name("repo.zip").unwrap(), "repo.zip");
        assert!(validate_upload_name("../evil.zip").is_err());
        assert!(validate_upload_name("a/b.zip").is_err());
    }
}
