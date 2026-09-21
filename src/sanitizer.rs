use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tar::{Builder, Header};

pub use crate::security::PasswordPolicy;
use crate::security::{
    default_exclusion, is_binary, is_kubernetes_secret_manifest, redact_text, safe_archive_path,
    validate_password,
};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveFormat {
    #[value(name = "none")]
    None,
    Tar,
    Zip,
    #[value(name = "7z", alias = "seven-zip")]
    #[serde(rename = "7z")]
    SevenZip,
}
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    #[value(name = "none")]
    None,
    Gzip,
    Zstd,
    Lz4,
    Lzip,
    Lzma,
    Lzo,
    Lrzip,
    Xz,
    Zlib,
    Brotli,
    Snappy,
    Bzip2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveCapability {
    pub format: ArchiveFormat,
    pub label: &'static str,
    pub extension: &'static str,
    pub password_encryption: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompressionCapability {
    pub compression: Compression,
    pub label: &'static str,
    pub extension: &'static str,
    pub standalone: bool,
}

pub const ARCHIVE_CAPABILITIES: &[ArchiveCapability] = &[
    ArchiveCapability {
        format: ArchiveFormat::None,
        label: "None (JSONL stream)",
        extension: "jsonl",
        password_encryption: false,
    },
    ArchiveCapability {
        format: ArchiveFormat::Tar,
        label: "TAR",
        extension: "tar",
        password_encryption: false,
    },
    ArchiveCapability {
        format: ArchiveFormat::Zip,
        label: "ZIP",
        extension: "zip",
        password_encryption: true,
    },
    ArchiveCapability {
        format: ArchiveFormat::SevenZip,
        label: "7z",
        extension: "7z",
        password_encryption: false,
    },
];

pub const COMPRESSION_CAPABILITIES: &[CompressionCapability] = &[
    CompressionCapability {
        compression: Compression::None,
        label: "None",
        extension: "",
        standalone: false,
    },
    CompressionCapability {
        compression: Compression::Gzip,
        label: "gzip",
        extension: "gz",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Zstd,
        label: "zstd",
        extension: "zst",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Lz4,
        label: "LZ4",
        extension: "lz4",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Lzip,
        label: "lzip",
        extension: "lz",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Lzma,
        label: "LZMA",
        extension: "lzma",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Lzo,
        label: "LZO",
        extension: "lzo",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Lrzip,
        label: "lrzip",
        extension: "lrz",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Xz,
        label: "XZ",
        extension: "xz",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Zlib,
        label: "zlib",
        extension: "zz",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Brotli,
        label: "Brotli",
        extension: "br",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Snappy,
        label: "Snappy",
        extension: "sz",
        standalone: true,
    },
    CompressionCapability {
        compression: Compression::Bzip2,
        label: "bzip2",
        extension: "bz2",
        standalone: true,
    },
];

pub fn archive_capability(format: ArchiveFormat) -> &'static ArchiveCapability {
    ARCHIVE_CAPABILITIES
        .iter()
        .find(|capability| capability.format == format)
        .expect("all archive formats have capability metadata")
}

pub fn compression_capability(compression: Compression) -> &'static CompressionCapability {
    COMPRESSION_CAPABILITIES
        .iter()
        .find(|capability| capability.compression == compression)
        .expect("all compression formats have capability metadata")
}

pub fn compatible_compressions(format: ArchiveFormat) -> Vec<Compression> {
    COMPRESSION_CAPABILITIES
        .iter()
        .filter_map(|capability| {
            let compatible = match format {
                ArchiveFormat::None => matches!(
                    capability.compression,
                    Compression::Gzip
                        | Compression::Zstd
                        | Compression::Lz4
                        | Compression::Xz
                        | Compression::Zlib
                        | Compression::Brotli
                        | Compression::Snappy
                        | Compression::Bzip2
                ),
                ArchiveFormat::Tar => matches!(
                    capability.compression,
                    Compression::None
                        | Compression::Gzip
                        | Compression::Zstd
                        | Compression::Lz4
                        | Compression::Lzip
                        | Compression::Lzma
                        | Compression::Lzo
                        | Compression::Lrzip
                        | Compression::Xz
                ),
                ArchiveFormat::Zip => matches!(
                    capability.compression,
                    Compression::Gzip | Compression::Zstd
                ),
                ArchiveFormat::SevenZip => capability.compression == Compression::None,
            };
            compatible.then_some(capability.compression)
        })
        .collect()
}

/// Resolves the zero-based compression selector index used by the GUI against
/// the same compatibility registry used for validation and CLI output.
pub fn compression_for_gui_selection(format: ArchiveFormat, index: usize) -> Option<Compression> {
    compatible_compressions(format).get(index).copied()
}

pub fn output_extension(format: ArchiveFormat, compression: Compression) -> Result<String> {
    let valid = compatible_compressions(format).contains(&compression);
    if !valid {
        bail!("compression is not valid for the selected archive format")
    }
    let archive = archive_capability(format).extension;
    let compression = compression_capability(compression).extension;
    Ok(if format == ArchiveFormat::None || archive.is_empty() {
        compression.to_owned()
    } else if compression.is_empty()
        || format == ArchiveFormat::Zip
        || format == ArchiveFormat::SevenZip
    {
        archive.to_owned()
    } else {
        format!("{archive}.{compression}")
    })
}

/// Adds a user-supplied Git-style glob to a configuration list. Empty values
/// are rejected, malformed patterns return a validation error, and duplicate
/// entries are treated as a no-op.
pub fn add_pattern(patterns: &mut Vec<String>, pattern: &str) -> Result<bool> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        bail!("glob pattern must not be empty")
    }
    Glob::new(pattern).with_context(|| format!("invalid glob: {pattern}"))?;
    if patterns.iter().any(|existing| existing == pattern) {
        return Ok(false);
    }
    patterns.push(pattern.to_owned());
    Ok(true)
}

pub fn remove_pattern(patterns: &mut Vec<String>, pattern: &str) -> bool {
    let before = patterns.len();
    patterns.retain(|existing| existing != pattern);
    patterns.len() != before
}

/// Prints the compatibility matrix from the same capability declarations used
/// by the CLI. Unsupported formats are listed deliberately, with no implication
/// that a raw stream is a valid archive.
pub fn print_formats() {
    println!("format\textension\tbackend\tpassword\tdependency");
    println!("none+gzip/none+zstd\t.gz/.zst\tinternal Rust JSONL stream\tno\tavailable");
    println!("none+lz4-frame/xz/zlib/brotli/snappy/bzip2\tcodec-specific\tcompcol 0.6.11 JSONL stream\tno\tavailable");
    println!("tar.gz\t.tar.gz\tinternal Rust\tno\tavailable");
    println!("tar.zst\t.tar.zst\tinternal Rust\tno\tavailable");
    println!("tar.lz4/tar.lz/tar.lzma/tar.lzo/tar.lrz/tar.xz\tstream\texternal fallback\tno\tdetected per request");
    println!("zip\t.zip\tinternal Rust\tAES-256\tavailable");
    println!(
        "7z\t.7z\texternal fallback\tno\t{}",
        if command_available("7z") {
            "available"
        } else {
            "missing"
        }
    );
    for (name, extension, tool) in [
        ("lrzip", ".lrz", "lrzip"),
        ("lzip", ".lz", "lzip"),
        ("lzo", ".lzo", "lzop"),
    ] {
        println!(
            "{name}\t{extension}\texternal fallback\tsee tool\t{}",
            if command_available(tool) {
                "available"
            } else {
                "missing"
            }
        );
    }
}

fn command_available(name: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .any(|dir| dir.join(name).is_file())
}

fn wait_for_child_with_cancellation<C: Fn() -> bool>(
    mut child: Child,
    cancelled: C,
) -> Result<std::process::ExitStatus> {
    loop {
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            bail!("sanitization cancelled");
        }
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn ensure_external_compressor_available(name: &str) -> Result<()> {
    if !command_available(name) {
        bail!("required external compressor '{name}' was not found in PATH");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportFormat {
    Markdown,
    Json,
    None,
}

/// Report members embedded in every archive. Shared so the web frontend can
/// offer them for download without duplicating the names.
pub const REPORT_MEMBERS: &[&str] = &[
    "SANITIZATION-REPORT.md",
    "SANITIZATION-REPORT.json",
    "REPOSITORY-INVENTORY.md",
    "SECRET-AUDIT.md",
    "manifest.json",
];
pub struct Config {
    pub repository: PathBuf,
    pub output: PathBuf,
    pub format: ArchiveFormat,
    pub compression: Compression,
    pub report: ReportFormat,
    pub include_untracked: bool,
    pub max_file_size: u64,
    pub excludes: Vec<String>,
    pub includes: Vec<String>,
    pub redact: bool,
    pub fail_on_secret: bool,
    pub dry_run: bool,
    pub password: Option<String>,
    pub password_policy: PasswordPolicy,
    pub password_file: Option<PathBuf>,
    pub verbose: bool,
    pub quiet: bool,
}
#[derive(Debug)]
pub struct Summary {
    pub included: usize,
    pub excluded: usize,
    pub redactions: usize,
    pub dry_run: bool,
    pub quiet: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgressEvent {
    Scanning { path: String, examined: usize },
    Writing { included: usize },
    Finished,
}

pub fn run(config: Config) -> Result<Summary> {
    run_with_progress(config, |_| {}, || false)
}

pub fn run_with_progress<F, C>(config: Config, mut progress: F, cancelled: C) -> Result<Summary>
where
    F: FnMut(ProgressEvent),
    C: Fn() -> bool,
{
    run_inner(config, &mut progress, &cancelled)
}

/// Validate selected capabilities before reading repository contents.
pub fn validate_config(config: &Config) -> Result<()> {
    let _ = output_extension(config.format, config.compression)?;
    if config.password.is_some() && config.format != ArchiveFormat::Zip {
        bail!("password protection is supported only for ZIP AES output")
    }
    if config.password.as_deref() == Some("") {
        bail!("password must not be empty")
    }
    if let Some(password) = config.password.as_deref() {
        config
            .password_policy
            .validate()
            .map_err(|error| anyhow::anyhow!(error))?;
        if let Err(unmet) = validate_password(password, config.password_policy) {
            bail!("password does not meet policy: {}", unmet.join(", "))
        }
    }
    Ok(())
}

/// Computes the deterministic output name used when `--output` is omitted.
pub fn default_output_path(
    repository: &Path,
    format: ArchiveFormat,
    compression: Compression,
    timestamp_name: bool,
) -> Result<PathBuf> {
    let root = fs::canonicalize(repository).context("repository path does not exist")?;
    let name = root
        .file_name()
        .and_then(|v| v.to_str())
        .map(|v| {
            v.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                        c
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "repository".to_owned());
    let head =
        git_one(&root, &["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let suffix = output_extension(format, compression)?;
    let stem = if timestamp_name {
        let now = chrono::Local::now();
        format!(
            "{}-{}-{name}-{head}-sanitized",
            now.format("%Y-%b-%d"),
            now.format("%H-%M")
        )
    } else {
        format!("{name}-{head}-sanitized")
    };
    Ok(root.join(format!("{stem}.{suffix}")))
}
#[derive(Serialize)]
struct Manifest {
    version: String,
    repository: String,
    branch: String,
    head: String,
    files: Vec<ManifestFile>,
    exclusions: Vec<Exclusion>,
    redactions: usize,
}
#[derive(Clone, Serialize)]
struct ManifestFile {
    path: String,
    sha256: String,
    original_bytes: u64,
    output_bytes: u64,
}
#[derive(Serialize)]
struct Exclusion {
    path: String,
    reason: String,
}

fn run_inner<F, C>(config: Config, progress: &mut F, cancelled: &C) -> Result<Summary>
where
    F: FnMut(ProgressEvent),
    C: Fn() -> bool,
{
    validate_config(&config)?;
    let root = fs::canonicalize(&config.repository).context("repository path does not exist")?;
    if config.password.is_some() && config.format != ArchiveFormat::Zip {
        bail!("password protection is supported only for ZIP AES output; TAR compression has no encryption");
    }
    if !root.join(".git").exists() {
        bail!("not a Git working tree: {}", root.display());
    }
    let output = absolute(&config.output)?;
    // Refusing to overwrite is intentional: an output path such as
    // `src/lib.rs` must never be able to truncate repository content.
    if output.exists() {
        bail!(
            "refusing to overwrite existing output: {}",
            output.display()
        );
    }
    let include = patterns(&config.includes)?;
    let exclude = patterns(&config.excludes)?;
    if config.verbose && !config.quiet {
        eprintln!("inspecting tracked files in {}", root.display());
    }
    let mut exclusions = Vec::new();
    let mut files = Vec::new();
    let mut redactions = 0usize;
    for (examined, relative) in git_files(&root, config.include_untracked)?
        .into_iter()
        .enumerate()
    {
        if cancelled() {
            bail!("sanitization cancelled")
        }
        progress(ProgressEvent::Scanning {
            path: relative.display().to_string(),
            examined: examined + 1,
        });
        let path = root.join(&relative);
        let display = match safe_archive_path(&relative) {
            Ok(path) => path,
            Err(_) => {
                exclusions.push(exclusion(
                    relative.to_string_lossy().into_owned(),
                    "unsafe path",
                ));
                continue;
            }
        };
        if path == output {
            exclusions.push(exclusion(display, "output archive"));
            continue;
        }
        if config.password_file.as_ref().is_some_and(|p| {
            path == *p
                || fs::canonicalize(p)
                    .map(|password| password == path)
                    .unwrap_or(false)
        }) {
            exclusions.push(exclusion(display, "password file"));
            continue;
        }
        if let Some(reason) = default_exclusion(&relative) {
            exclusions.push(exclusion(display, &reason.to_string()));
            continue;
        }
        if !config.includes.is_empty() && !include.is_match(&relative) {
            exclusions.push(exclusion(display, "not included by pattern"));
            continue;
        }
        if exclude.is_match(&relative) {
            exclusions.push(exclusion(display, "excluded by pattern"));
            continue;
        }
        let mut file = match open_regular_file(&path) {
            Ok(v) => v,
            Err(_) => {
                exclusions.push(exclusion(display, "unreadable"));
                continue;
            }
        };
        let meta = file.metadata()?;
        if !meta.is_file() {
            exclusions.push(exclusion(display, "not a regular file"));
            continue;
        }
        if meta.len() > config.max_file_size {
            exclusions.push(exclusion(display, "file exceeds maximum size"));
            continue;
        }
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        if is_binary(&data) {
            exclusions.push(exclusion(display, "binary content"));
            continue;
        }
        if is_kubernetes_secret_manifest(&data) {
            exclusions.push(exclusion(display, "Kubernetes Secret manifest"));
            continue;
        }
        let (content, count) = if config.redact {
            let result = redact_text(&String::from_utf8_lossy(&data));
            (result.text.into_bytes(), result.counts.values().sum())
        } else {
            (data, 0)
        };
        if config.fail_on_secret && count > 0 {
            bail!("secret detected in {}", relative.display());
        }
        redactions += count;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        files.push((
            display.clone(),
            content,
            ManifestFile {
                path: display,
                sha256: format!("{:x}", hasher.finalize()),
                original_bytes: meta.len(),
                output_bytes: 0,
            },
        ));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    progress(ProgressEvent::Writing {
        included: files.len(),
    });
    if cancelled() {
        bail!("sanitization cancelled")
    }
    for (_, content, entry) in &mut files {
        entry.output_bytes = content.len() as u64;
    }
    exclusions.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        repository: root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        branch: git_one(&root, &["branch", "--show-current"]).unwrap_or_else(|| "DETACHED".into()),
        head: git_one(&root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "UNKNOWN".into()),
        files: files
            .iter()
            .map(|x| ManifestFile {
                path: x.2.path.clone(),
                sha256: x.2.sha256.clone(),
                original_bytes: x.2.original_bytes,
                output_bytes: x.2.output_bytes,
            })
            .collect(),
        exclusions,
        redactions,
    };
    let history = git_history(&root, config.redact)?;
    if !config.dry_run {
        write_archive_with_cancellation(
            &output,
            config.format,
            config.compression,
            &files,
            &manifest,
            &history,
            config.password.as_deref(),
            config.report,
            cancelled,
        )?;
    }
    progress(ProgressEvent::Finished);
    Ok(Summary {
        included: files.len(),
        excluded: manifest.exclusions.len(),
        redactions,
        dry_run: config.dry_run,
        quiet: config.quiet,
    })
}

fn git_history(root: &Path, redact: bool) -> Result<String> {
    // Git's topo-order traversal is the ordering contract: parents precede
    // children in reverse chronology, while Git resolves equivalent ties
    // consistently for a fixed set of refs.
    // `--all` fails the entire traversal when a repository contains a stale
    // or malformed ref.  Enumerate refs first and retain only refs that
    // resolve to commits, so one broken remote-tracking ref cannot hide the
    // valid history of an otherwise usable repository.
    let refs = git_commit_refs(root)?;
    let mut command = Command::new("git");
    command.current_dir(root).args([
        "log",
        "--topo-order",
        "--reverse",
        "--format=%H%x09%h%x09%s",
    ]);
    if refs.is_empty() {
        command.arg("HEAD");
    } else {
        command.args(refs);
    }
    let out = command.output().context("reading git history")?;
    if !out.status.success() {
        bail!("git history failed (repository may be malformed or shallow)");
    }
    let mut result = String::new();
    let mut emitted = HashSet::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut fields = line.splitn(3, '\t');
        let Some(full_id) = fields.next() else {
            continue;
        };
        let Some(id) = fields.next() else {
            continue;
        };
        let Some(subject) = fields.next() else {
            continue;
        };
        if !emitted.insert(full_id) {
            continue;
        }
        let subject = subject.lines().next().unwrap_or_default();
        let subject = if redact {
            crate::security::redact_text(subject).text
        } else {
            subject.to_owned()
        };
        result.push_str(id);
        result.push(' ');
        result.push_str(subject.trim_end());
        result.push('\n');
    }
    Ok(result)
}

fn git_commit_refs(root: &Path) -> Result<Vec<String>> {
    let refs = Command::new("git")
        .current_dir(root)
        .args(["for-each-ref", "--format=%(refname)"])
        .output()
        .context("enumerating Git refs")?;
    if !refs.status.success() {
        bail!("Git ref enumeration failed");
    }

    let mut valid = Vec::new();
    for name in String::from_utf8_lossy(&refs.stdout)
        .lines()
        .filter(|name| !name.is_empty())
    {
        let resolves = Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--verify"])
            .arg(format!("{name}^{{commit}}"))
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if resolves {
            valid.push(name.to_owned());
        }
    }
    Ok(valid)
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
fn patterns(raw: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in raw {
        b.add(Glob::new(p).with_context(|| format!("invalid glob: {p}"))?);
    }
    Ok(b.build()?)
}
fn git_files(root: &Path, untracked: bool) -> Result<Vec<PathBuf>> {
    let mut args = vec!["ls-files", "-z"];
    if untracked {
        args.extend(["--cached", "--others", "--exclude-standard"]);
    }
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .context("running git ls-files")?;
    if !output.status.success() {
        bail!("git ls-files failed");
    }
    let mut files = output
        .stdout
        .split(|b| *b == 0)
        .filter(|x| !x.is_empty())
        .map(|x| PathBuf::from(String::from_utf8_lossy(x).into_owned()))
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    Ok(files)
}
fn git_one(root: &Path, args: &[&str]) -> Option<String> {
    let x = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    if x.status.success() {
        Some(String::from_utf8_lossy(&x.stdout).trim().to_owned())
    } else {
        None
    }
}
fn exclusion(path: String, reason: &str) -> Exclusion {
    Exclusion {
        path,
        reason: reason.into(),
    }
}
fn open_regular_file(path: &Path) -> Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        Ok(OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?)
    }
    #[cfg(not(unix))]
    {
        Ok(File::open(path)?)
    }
}
fn append(builder: &mut Builder<Box<dyn Write>>, path: &str, content: &[u8]) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    header.set_cksum();
    builder.append_data(&mut header, path, content)?;
    Ok(())
}

/// Writes the archive-none representation. Each line is a self-describing
/// JSON record, so multiple sanitized files are never ambiguous concatenated
/// bytes. Text content, paths, manifest, and history can be reconstructed in
/// order from the stream.
fn write_jsonl_stream(
    output: &Path,
    compression: Compression,
    files: &[(String, Vec<u8>, ManifestFile)],
    manifest: &Manifest,
    history: &str,
    report: ReportFormat,
) -> Result<()> {
    let temporary = temporary_path(output);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("creating {}", temporary.display()))?;
    let mut writer: Box<dyn Write> = match compression {
        Compression::Gzip => Box::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::default(),
        )),
        Compression::Zstd => Box::new(zstd::stream::write::Encoder::new(file, 3)?.auto_finish()),
        compression => {
            let name = match compression {
                Compression::Lz4 => "lz4-frame",
                Compression::Xz => "xz",
                Compression::Zlib => "zlib",
                Compression::Brotli => "brotli",
                Compression::Snappy => "snappy",
                Compression::Bzip2 => "bzip2",
                _ => bail!("Archive=none does not support this stream compression"),
            };
            let encoder = compcol::factory::encoder_by_name(name)
                .ok_or_else(|| anyhow::anyhow!("compcol encoder is unavailable: {name}"))?;
            Box::new(compcol::io::EncoderWriter::new(file, encoder))
        }
    };
    let result = (|| -> Result<()> {
        let mut write_record = |record: serde_json::Value| -> Result<()> {
            serde_json::to_writer(&mut writer, &record)?;
            writer.write_all(b"\n")?;
            Ok(())
        };
        write_record(serde_json::json!({ "type": "manifest", "value": manifest }))?;
        write_record(serde_json::json!({ "type": "history", "value": history }))?;
        for (path, content, _) in files {
            let text = String::from_utf8_lossy(content);
            write_record(serde_json::json!({
                "type": "file",
                "path": path,
                "content": text,
            }))?;
        }
        if report != ReportFormat::None {
            write_record(serde_json::json!({
                "type": "report",
                "format": if report == ReportFormat::Json { "json" } else { "markdown" },
                "value": manifest,
            }))?;
        }
        writer.flush()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return result;
    }
    fs::rename(temporary, output).context("installing JSONL stream output")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_archive(
    output: &Path,
    format: ArchiveFormat,
    compression: Compression,
    files: &[(String, Vec<u8>, ManifestFile)],
    manifest: &Manifest,
    history: &str,
    password: Option<&str>,
    report: ReportFormat,
) -> Result<()> {
    write_archive_with_cancellation(
        output,
        format,
        compression,
        files,
        manifest,
        history,
        password,
        report,
        || false,
    )
}

#[allow(clippy::too_many_arguments)]
fn write_archive_with_cancellation<C: Fn() -> bool>(
    output: &Path,
    format: ArchiveFormat,
    compression: Compression,
    files: &[(String, Vec<u8>, ManifestFile)],
    manifest: &Manifest,
    history: &str,
    password: Option<&str>,
    report: ReportFormat,
    cancelled: C,
) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    if format == ArchiveFormat::None {
        return write_jsonl_stream(output, compression, files, manifest, history, report);
    }
    if format == ArchiveFormat::Zip {
        return write_zip(
            output,
            compression,
            files,
            manifest,
            history,
            report,
            password,
        );
    }
    if format == ArchiveFormat::SevenZip {
        return write_seven_zip(output, files, manifest, history, report);
    }
    if !matches!(
        compression,
        Compression::Gzip | Compression::Zstd | Compression::None
    ) {
        return write_external_tar(
            output,
            compression,
            files,
            manifest,
            history,
            password,
            report,
            &cancelled,
        );
    }
    let temporary = temporary_path(output);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("creating {}", temporary.display()))?;
    let writer: Box<dyn Write> = match (format, compression) {
        (ArchiveFormat::Tar, Compression::None) => Box::new(file),
        (ArchiveFormat::Tar, Compression::Gzip) => Box::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::default(),
        )),
        (ArchiveFormat::Tar, Compression::Zstd) => {
            Box::new(zstd::stream::write::Encoder::new(file, 3)?.auto_finish())
        }
        (ArchiveFormat::Zip, _) | (ArchiveFormat::SevenZip, _) => {
            unreachable!("non-TAR format handled above")
        }
        (ArchiveFormat::Tar, _) => unreachable!("external codec handled above"),
        (ArchiveFormat::None, _) => unreachable!("JSONL stream handled above"),
    };
    let result = (|| -> Result<()> {
        let mut tar = Builder::new(writer);
        if cancelled() {
            bail!("sanitization cancelled");
        }
        append(&mut tar, ".git/COMMIT-HISTORY.txt", history.as_bytes())?;
        let mut sums = BTreeMap::new();
        for (name, data, entry) in files {
            if cancelled() {
                bail!("sanitization cancelled");
            }
            append(&mut tar, name, data)?;
            sums.insert(name.clone(), entry.sha256.clone());
        }
        let manifest_json = serde_json::to_vec_pretty(manifest)?;
        append(&mut tar, "manifest.json", &manifest_json)?;
        let mut sha = String::new();
        for (p, h) in sums {
            sha.push_str(&format!("{h}  {p}\n"));
        }
        append(&mut tar, "SHA256SUMS", sha.as_bytes())?;
        let original_bytes: u64 = manifest.files.iter().map(|file| file.original_bytes).sum();
        let output_bytes: u64 = manifest.files.iter().map(|file| file.output_bytes).sum();
        let inventory = format!(
        "# Repository Inventory\n\n- Files: {}\n- Original bytes: {original_bytes}\n- Sanitized bytes: {output_bytes}\n\n## Files\n\n{}\n",
        manifest.files.len(),
        manifest.files.iter().map(|file| format!("- `{}` — {} bytes — `{}`", file.path, file.output_bytes, file.sha256)).collect::<Vec<_>>().join("\n")
    );
        append(&mut tar, "REPOSITORY-INVENTORY.md", inventory.as_bytes())?;
        let audit = format!(
        "# Secret Audit\n\n- Redactions: {}\n- Secret values are never recorded.\n\n## Excluded sensitive paths\n\n{}\n",
        manifest.redactions,
        manifest.exclusions.iter().filter(|item| item.reason.contains("key") || item.reason.contains("credential") || item.reason.contains("environment") || item.reason.contains("kube")).map(|item| format!("- `{}`: {}", item.path, item.reason)).collect::<Vec<_>>().join("\n")
    );
        append(&mut tar, "SECRET-AUDIT.md", audit.as_bytes())?;
        if report != ReportFormat::None {
            let report_text = if report == ReportFormat::Json {
                serde_json::to_vec_pretty(manifest)?
            } else {
                format!("# Sanitization Report\n\n- Repository: `{}`\n- Branch: `{}`\n- HEAD: `{}`\n- Included files: {}\n- Excluded files: {}\n- Redactions: {}\n\n## Exclusions\n\n{}",manifest.repository,manifest.branch,manifest.head,manifest.files.len(),manifest.exclusions.len(),manifest.redactions,manifest.exclusions.iter().map(|e|format!("- `{}`: {}",e.path,e.reason)).collect::<Vec<_>>().join("\n")).into_bytes()
            };
            append(
                &mut tar,
                if report == ReportFormat::Json {
                    "SANITIZATION-REPORT.json"
                } else {
                    "SANITIZATION-REPORT.md"
                },
                &report_text,
            )?;
        }
        tar.finish()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return result;
    }
    fs::rename(&temporary, output)?;
    Ok(())
}

fn write_seven_zip(
    output: &Path,
    files: &[(String, Vec<u8>, ManifestFile)],
    manifest: &Manifest,
    history: &str,
    report: ReportFormat,
) -> Result<()> {
    let staging = output.with_extension(format!("staging-{}", std::process::id()));
    fs::create_dir(&staging)
        .with_context(|| format!("creating staging directory {}", staging.display()))?;
    let result = (|| -> Result<()> {
        let add = |name: &str, data: &[u8]| -> Result<()> {
            let path = staging.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, data)?;
            Ok(())
        };
        add(".git/COMMIT-HISTORY.txt", history.as_bytes())?;
        for (name, data, _) in files {
            add(name, data)?;
        }
        add("manifest.json", &serde_json::to_vec_pretty(manifest)?)?;
        if report != ReportFormat::None {
            add(
                if report == ReportFormat::Json {
                    "SANITIZATION-REPORT.json"
                } else {
                    "SANITIZATION-REPORT.md"
                },
                &if report == ReportFormat::Json {
                    serde_json::to_vec_pretty(manifest)?
                } else {
                    format!("# Sanitization Report\n\n- Included files: {}\n- Excluded files: {}\n- Redactions: {}\n", manifest.files.len(), manifest.exclusions.len(), manifest.redactions).into_bytes()
                },
            )?;
        }
        let temporary = temporary_path(output);
        let status = Command::new("7z")
            .args(["a", "-t7z", "-mx=5", "-mtc=off", "-mtm=off", "-mta=off"])
            .arg(&temporary)
            .arg(".")
            .current_dir(&staging)
            .output()
            .context("running 7z")?;
        if !status.status.success() {
            bail!("7z failed with status {}", status.status);
        }
        fs::rename(temporary, output).context("installing 7z output")?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

#[allow(clippy::too_many_arguments)]
fn write_external_tar(
    output: &Path,
    compression: Compression,
    files: &[(String, Vec<u8>, ManifestFile)],
    manifest: &Manifest,
    history: &str,
    password: Option<&str>,
    report: ReportFormat,
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    if password.is_some() {
        bail!("password protection is unavailable for external compression tools");
    }
    let (tool, args): (&str, &[&str]) = match compression {
        Compression::Lz4 => ("lz4", &["-f"]),
        Compression::Lzip => ("lzip", &["-c"]),
        Compression::Lzma => ("lzma", &["-c"]),
        Compression::Lzo => ("lzop", &["-c"]),
        Compression::Lrzip => ("lrzip", &["-o"]),
        Compression::Xz => ("xz", &["-c"]),
        _ => unreachable!("external compressor requested only for external codec"),
    };
    ensure_external_compressor_available(tool)?;
    let tar_path = temporary_path(output).with_extension("tar");
    write_archive(
        &tar_path,
        ArchiveFormat::Tar,
        Compression::None,
        files,
        manifest,
        history,
        None,
        report,
    )?;
    let temporary = temporary_path(output);
    let child = if tool == "lrzip" {
        Command::new(tool)
            .args(["-q", "-o"])
            .arg(&temporary)
            .arg(&tar_path)
            .spawn()
            .with_context(|| format!("running {tool}"))?
    } else {
        let input = File::open(&tar_path)?;
        let output_file = File::create(&temporary)?;
        Command::new(tool)
            .args(args)
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output_file))
            .spawn()
            .with_context(|| format!("running {tool}"))?
    };
    let status_result = wait_for_child_with_cancellation(child, cancelled);
    let _ = fs::remove_file(&tar_path);
    let _ = fs::remove_file(&temporary);
    let status = status_result?;
    if !status.success() {
        let _ = fs::remove_file(&temporary);
        bail!("{tool} failed with status {status}");
    }
    fs::rename(temporary, output).context("installing compressed TAR output")?;
    Ok(())
}

fn write_zip(
    output: &Path,
    compression: Compression,
    files: &[(String, Vec<u8>, ManifestFile)],
    manifest: &Manifest,
    history: &str,
    report: ReportFormat,
    password: Option<&str>,
) -> Result<()> {
    let temporary = temporary_path(output);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let mut zip = zip::ZipWriter::new(file);
    let method = match compression {
        Compression::Gzip => zip::CompressionMethod::Deflated,
        Compression::Zstd => zip::CompressionMethod::Zstd,
        _ => bail!("ZIP supports only gzip (Deflate) or zstd compression"),
    };
    let mut options = zip::write::SimpleFileOptions::default()
        .compression_method(method)
        .last_modified_time(zip::DateTime::default());
    if let Some(password) = password {
        options = options.with_aes_encryption(zip::AesMode::Aes256, password);
    }
    let result = (|| -> Result<()> {
        let mut add = |name: &str, data: &[u8]| -> Result<()> {
            zip.start_file(name, options)
                .context("starting ZIP member")?;
            std::io::Write::write_all(&mut zip, data).context("writing ZIP member")?;
            Ok(())
        };
        add(".git/COMMIT-HISTORY.txt", history.as_bytes())?;
        for (name, data, _) in files {
            add(name, data)?;
        }
        add("manifest.json", &serde_json::to_vec_pretty(manifest)?)?;
        if report != ReportFormat::None {
            let report_data = if report == ReportFormat::Json {
                serde_json::to_vec_pretty(manifest)?
            } else {
                format!(
                    "# Sanitization Report\n\n- Repository: `{}`\n- Branch: `{}`\n- HEAD: `{}`\n- Included files: {}\n- Excluded files: {}\n- Redactions: {}\n\n## Exclusions\n\n{}",
                    manifest.repository,
                    manifest.branch,
                    manifest.head,
                    manifest.files.len(),
                    manifest.exclusions.len(),
                    manifest.redactions,
                    manifest
                        .exclusions
                        .iter()
                        .map(|e| format!("- `{}`: {}", e.path, e.reason))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
                .into_bytes()
            };
            add(
                if report == ReportFormat::Json {
                    "SANITIZATION-REPORT.json"
                } else {
                    "SANITIZATION-REPORT.md"
                },
                &report_data,
            )?;
        }
        zip.finish().context("finishing ZIP")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return result;
    }
    fs::rename(temporary, output).context("installing ZIP output")?;
    Ok(())
}

fn temporary_path(output: &Path) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    output.with_extension(format!("partial-{}-{nonce}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn repo() -> tempfile::TempDir {
        let d = tempdir().unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(d.path())
            .status()
            .unwrap();
        fs::write(d.path().join("a.txt"), "TOKEN=not-a-real-secret\nhello\n").unwrap();
        fs::write(
            d.path().join("ref.yaml"),
            "secretKeyRef:\n  name: app-secret\n",
        )
        .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(d.path())
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "initial TOKEN=history-secret",
            ])
            .current_dir(d.path())
            .status()
            .unwrap();
        d
    }

    #[test]
    fn missing_external_compressor_is_reported_before_staging() {
        let error = ensure_external_compressor_available(
            "rustrepo-sanitizer-test-compressor-that-does-not-exist",
        )
        .unwrap_err();
        assert!(error.to_string().contains("was not found in PATH"));
    }

    #[test]
    fn cancellation_during_archive_writing_removes_partial_output() {
        let d = tempdir().unwrap();
        let output = d.path().join("cancelled.tar");
        let files = (0..32)
            .map(|index| {
                (
                    format!("file-{index}.txt"),
                    vec![b'x'; 4096],
                    ManifestFile {
                        path: format!("file-{index}.txt"),
                        sha256: "hash".to_owned(),
                        original_bytes: 4096,
                        output_bytes: 4096,
                    },
                )
            })
            .collect::<Vec<_>>();
        let manifest = Manifest {
            version: "0.4.0".to_owned(),
            repository: "test".to_owned(),
            branch: "main".to_owned(),
            head: "head".to_owned(),
            files: files.iter().map(|(_, _, file)| file.clone()).collect(),
            exclusions: vec![],
            redactions: 0,
        };
        let error = write_archive_with_cancellation(
            &output,
            ArchiveFormat::Tar,
            Compression::None,
            &files,
            &manifest,
            "history",
            None,
            ReportFormat::None,
            || true,
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert!(!output.exists());
        assert!(fs::read_dir(d.path()).unwrap().next().is_none());
    }

    #[test]
    fn external_child_cancellation_terminates_process() {
        let child = Command::new("sh").args(["-c", "sleep 30"]).spawn().unwrap();
        let result = wait_for_child_with_cancellation(child, || true);
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn external_compressor_cancellation_removes_staging_files() {
        if Command::new("xz").arg("--version").output().is_err() {
            return;
        }
        let d = tempdir().unwrap();
        let output = d.path().join("cancelled.tar.xz");
        let files = vec![(
            "file.txt".to_owned(),
            vec![b'x'; 4096],
            ManifestFile {
                path: "file.txt".to_owned(),
                sha256: "hash".to_owned(),
                original_bytes: 4096,
                output_bytes: 4096,
            },
        )];
        let manifest = Manifest {
            version: "0.4.0".to_owned(),
            repository: "test".to_owned(),
            branch: "main".to_owned(),
            head: "head".to_owned(),
            files: files.iter().map(|(_, _, file)| file.clone()).collect(),
            exclusions: vec![],
            redactions: 0,
        };
        let error = write_external_tar(
            &output,
            Compression::Xz,
            &files,
            &manifest,
            "history",
            None,
            ReportFormat::None,
            &|| true,
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert!(!output.exists());
        assert!(fs::read_dir(d.path()).unwrap().next().is_none());
    }

    #[test]
    fn gui_compression_selection_follows_authoritative_capabilities() {
        for format in [
            ArchiveFormat::None,
            ArchiveFormat::Tar,
            ArchiveFormat::Zip,
            ArchiveFormat::SevenZip,
        ] {
            let choices = compatible_compressions(format);
            for (index, expected) in choices.iter().enumerate() {
                assert_eq!(
                    compression_for_gui_selection(format, index),
                    Some(*expected)
                );
            }
            assert_eq!(compression_for_gui_selection(format, choices.len()), None);
        }
    }
    #[test]
    fn redacts_value_not_reference() {
        let redacted = redact_text("TOKEN=not-a-real-secret\nsecretKeyRef: app-secret\n");
        let (x, n) = (
            redacted.text.into_bytes(),
            redacted.counts.values().sum::<usize>(),
        );
        assert_eq!(n, 1);
        let s = String::from_utf8(x).unwrap();
        assert!(s.contains("TOKEN= [REDACTED]"));
        assert!(s.contains("secretKeyRef"));
    }
    #[test]
    fn safe_rejects_parent() {
        assert!(safe_archive_path(Path::new("../x")).is_err());
    }
    #[test]
    fn default_output_name_is_deterministic_and_safe() {
        let d = repo();
        let path =
            default_output_path(d.path(), ArchiveFormat::Tar, Compression::Zstd, true).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with("-sanitized.tar.zst"));
        assert!(name.contains("-sanitized.tar.zst"));
        assert!(!name.contains('/'));
        let stable =
            default_output_path(d.path(), ArchiveFormat::Tar, Compression::Zstd, false).unwrap();
        let stable_name = stable.file_name().unwrap().to_string_lossy();
        assert!(stable_name.ends_with("-sanitized.tar.zst"));
        assert!(!stable_name.contains("-202"));
        assert_eq!(stable.parent().unwrap(), d.path());
    }

    #[test]
    fn timestamped_output_name_starts_with_timestamp_before_repository_name() {
        let d = repo();
        let path =
            default_output_path(d.path(), ArchiveFormat::Zip, Compression::Gzip, true).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        let timestamp = name
            .as_bytes()
            .get(0..18)
            .is_some_and(|prefix| prefix[4] == b'-' && prefix[8] == b'-' && prefix[11] == b'-');
        assert!(timestamp, "timestamp must be first: {name}");
        assert!(name.ends_with("-sanitized.zip"));
        let head = git_one(d.path(), &["rev-parse", "--short=7", "HEAD"]).unwrap();
        assert!(name.contains(&format!("-{head}-sanitized.zip")));

        let stable =
            default_output_path(d.path(), ArchiveFormat::Zip, Compression::Gzip, false).unwrap();
        let stable_name = stable.file_name().unwrap().to_string_lossy();
        assert_ne!(stable_name.as_bytes().get(4), Some(&b'-'));
        assert!(stable_name.ends_with("-sanitized.zip"));
    }

    #[test]
    fn include_untracked_unions_eligible_tracked_and_untracked_files() {
        let d = repo();
        fs::write(d.path().join(".gitignore"), "ignored.txt\n").unwrap();
        Command::new("git")
            .args(["add", ".gitignore"])
            .current_dir(d.path())
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "add ignore rules",
            ])
            .current_dir(d.path())
            .status()
            .unwrap();
        fs::write(d.path().join("a.txt"), "modified\n").unwrap();
        fs::write(d.path().join("untracked.txt"), "new\n").unwrap();
        fs::write(d.path().join("ignored.txt"), "ignored\n").unwrap();

        let files = git_files(d.path(), true).unwrap();
        assert_eq!(
            files,
            vec![
                PathBuf::from(".gitignore"),
                PathBuf::from("a.txt"),
                PathBuf::from("ref.yaml"),
                PathBuf::from("untracked.txt"),
            ]
        );
    }

    #[test]
    fn capability_matrix_has_expected_compatibility_and_extensions() {
        assert_eq!(
            output_extension(ArchiveFormat::Tar, Compression::Gzip).unwrap(),
            "tar.gz"
        );
        assert_eq!(
            output_extension(ArchiveFormat::Tar, Compression::Xz).unwrap(),
            "tar.xz"
        );
        assert_eq!(
            output_extension(ArchiveFormat::Zip, Compression::Zstd).unwrap(),
            "zip"
        );
        assert_eq!(
            output_extension(ArchiveFormat::SevenZip, Compression::None).unwrap(),
            "7z"
        );
        assert!(output_extension(ArchiveFormat::Zip, Compression::Xz).is_err());
        assert_eq!(
            compatible_compressions(ArchiveFormat::Zip),
            vec![Compression::Gzip, Compression::Zstd]
        );
        assert_eq!(
            compatible_compressions(ArchiveFormat::SevenZip),
            vec![Compression::None]
        );
        assert!(compatible_compressions(ArchiveFormat::None).contains(&Compression::Brotli));
        assert_eq!(
            output_extension(ArchiveFormat::None, Compression::Bzip2).unwrap(),
            "bz2"
        );
    }

    #[test]
    fn compcol_exposed_stream_encoders_are_available() {
        for name in ["lz4-frame", "xz", "zlib", "brotli", "snappy", "bzip2"] {
            assert!(
                compcol::factory::encoder_by_name(name).is_some(),
                "compcol encoder must be available: {name}"
            );
        }
    }

    #[test]
    fn compcol_exposed_streams_round_trip_through_authoritative_decoders() {
        use std::io::{Cursor, Read, Write};

        let input = b"Rustrepo Sanitizer compcol interoperability test\n";
        for name in ["lz4-frame", "xz", "zlib", "brotli", "snappy", "bzip2"] {
            let encoder = compcol::factory::encoder_by_name(name).unwrap();
            let mut writer = compcol::io::EncoderWriter::new(Vec::new(), encoder);
            writer.write_all(input).unwrap();
            let encoded = writer.finish().unwrap();
            let decoder = compcol::factory::decoder_by_name(name).unwrap();
            let mut reader = compcol::io::DecoderReader::new(Cursor::new(encoded), decoder);
            let mut decoded = Vec::new();
            reader.read_to_end(&mut decoded).unwrap();
            assert_eq!(decoded, input, "compcol round trip failed for {name}");
        }
    }

    #[test]
    fn compatibility_matrix_is_exhaustive_for_every_registered_codec() {
        for archive in [
            ArchiveFormat::None,
            ArchiveFormat::Tar,
            ArchiveFormat::Zip,
            ArchiveFormat::SevenZip,
        ] {
            for capability in COMPRESSION_CAPABILITIES {
                let compatible = compatible_compressions(archive).contains(&capability.compression);
                let extension = output_extension(archive, capability.compression);
                assert_eq!(
                    compatible,
                    extension.is_ok(),
                    "matrix mismatch for {archive:?}/{:?}",
                    capability.compression
                );
                if compatible {
                    assert!(!extension.unwrap().is_empty());
                }
            }
        }
    }

    #[test]
    fn every_supported_capability_has_its_declared_output_extension() {
        for archive in [
            ArchiveFormat::None,
            ArchiveFormat::Tar,
            ArchiveFormat::Zip,
            ArchiveFormat::SevenZip,
        ] {
            for capability in COMPRESSION_CAPABILITIES {
                if compatible_compressions(archive).contains(&capability.compression) {
                    let expected = output_extension(archive, capability.compression).unwrap();
                    assert!(!expected.is_empty());
                    if archive == ArchiveFormat::Tar && capability.standalone {
                        assert!(expected.starts_with("tar."));
                    }
                }
            }
        }
    }

    #[test]
    fn pattern_lists_validate_deduplicate_and_remove() {
        let mut patterns = Vec::new();
        assert!(add_pattern(&mut patterns, "  docs/**/*.md ").unwrap());
        assert!(!add_pattern(&mut patterns, "docs/**/*.md").unwrap());
        assert!(add_pattern(&mut patterns, "tests/**").unwrap());
        assert_eq!(patterns, vec!["docs/**/*.md", "tests/**"]);
        assert!(remove_pattern(&mut patterns, "docs/**/*.md"));
        assert!(!remove_pattern(&mut patterns, "missing/**"));
        assert!(add_pattern(&mut patterns, "[").is_err());
        assert!(add_pattern(&mut patterns, " ").is_err());
    }

    #[test]
    fn built_in_globs_cover_nested_extensions_dotfiles_and_interactions() {
        let include_presets = [
            ("docs/**/*.md", "docs/guide/README.md"),
            ("src/**/*.rs", "src/nested/module.rs"),
            ("tests/**", "tests/fixtures/input.json"),
            (".forgejo/**", ".forgejo/workflows/ci.yml"),
            ("target/**", "target/debug/app"),
            ("vendor/**", "vendor/lib/source.c"),
            ("*.log", "build.log"),
        ];
        for (glob, path) in include_presets {
            assert!(patterns(&[glob.to_owned()]).unwrap().is_match(path));
        }
        let exclude_presets = [
            ("docs/**", "docs/guide.md"),
            ("target/**", "target/debug/app"),
            ("vendor/**", "vendor/lib/source.c"),
            ("node_modules/**", "node_modules/pkg/index.js"),
            (".idea/**", ".idea/workspace.xml"),
            ("*.log", "build.log"),
            ("*.tmp", "scratch.tmp"),
        ];
        for (glob, path) in exclude_presets {
            assert!(patterns(&[glob.to_owned()]).unwrap().is_match(path));
        }
        let include = patterns(&["**".to_owned()]).unwrap();
        let exclude = patterns(&["*.log".to_owned(), "target/**".to_owned()]).unwrap();
        for path in [".env", "src/main.rs", "nested/docs/readme.md"] {
            assert!(include.is_match(path));
            assert!(!exclude.is_match(path));
        }
        for path in ["build.log", "target/debug/app"] {
            assert!(exclude.is_match(path));
        }
    }

    #[test]
    fn archive_none_is_a_reversible_jsonl_stream() {
        let d = repo();
        let output = d.path().join("bundle.gz");
        run(Config {
            repository: d.path().into(),
            output: output.clone(),
            format: ArchiveFormat::None,
            compression: Compression::Gzip,
            report: ReportFormat::None,
            include_untracked: false,
            max_file_size: 100_000,
            excludes: vec![],
            includes: vec![],
            redact: true,
            fail_on_secret: false,
            dry_run: false,
            password: None,
            password_policy: PasswordPolicy::default(),
            password_file: None,
            verbose: false,
            quiet: true,
        })
        .unwrap();
        let bytes = fs::read(output).unwrap();
        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
        let mut text = String::new();
        decoder.read_to_string(&mut text).unwrap();
        let records: Vec<serde_json::Value> = text
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert!(records.iter().any(|record| record["type"] == "manifest"));
        assert!(records.iter().any(|record| record["type"] == "history"));
        assert!(records.iter().any(|record| record["type"] == "file"));
        assert!(records
            .iter()
            .all(|record| record["path"].is_null() || record["path"].is_string()));
    }

    #[test]
    fn zip_markdown_report_is_markdown_and_json_report_is_json() {
        let d = repo();
        for (report, name, expected) in [
            (
                ReportFormat::Markdown,
                "SANITIZATION-REPORT.md",
                "# Sanitization Report",
            ),
            (ReportFormat::Json, "SANITIZATION-REPORT.json", "\"files\""),
        ] {
            let output = d.path().join(format!("{}.zip", name));
            run(Config {
                repository: d.path().into(),
                output: output.clone(),
                format: ArchiveFormat::Zip,
                compression: Compression::Gzip,
                report,
                include_untracked: false,
                max_file_size: 100_000,
                excludes: vec![],
                includes: vec![],
                redact: true,
                fail_on_secret: false,
                dry_run: false,
                password: None,
                password_policy: PasswordPolicy::default(),
                password_file: None,
                verbose: false,
                quiet: true,
            })
            .unwrap();
            let file = fs::File::open(output).unwrap();
            let mut archive = zip::ZipArchive::new(file).unwrap();
            let mut report_file = archive.by_name(name).unwrap();
            let mut contents = String::new();
            report_file.read_to_string(&mut contents).unwrap();
            assert!(contents.contains(expected));
            if report == ReportFormat::Markdown {
                assert!(!contents.trim_start().starts_with('{'));
            } else {
                serde_json::from_str::<serde_json::Value>(&contents).unwrap();
            }
        }
    }

    #[test]
    fn zip_password_produces_aes_encrypted_readable_output() {
        let d = repo();
        let output = d.path().join("encrypted.zip");
        run(Config {
            repository: d.path().into(),
            output: output.clone(),
            format: ArchiveFormat::Zip,
            compression: Compression::Gzip,
            report: ReportFormat::None,
            include_untracked: false,
            max_file_size: 100_000,
            excludes: vec![],
            includes: vec![],
            redact: true,
            fail_on_secret: false,
            dry_run: false,
            password: Some("ValidPass1!".to_owned()),
            password_policy: PasswordPolicy::default(),
            password_file: None,
            verbose: false,
            quiet: true,
        })
        .unwrap();

        let file = fs::File::open(&output).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert!(archive.by_name("a.txt").is_err());
        let mut member = archive.by_name_decrypt("a.txt", b"ValidPass1!").unwrap();
        let mut contents = String::new();
        member.read_to_string(&mut contents).unwrap();
        assert!(contents.contains("hello"));
    }

    #[test]
    fn archive_none_compcol_streams_write_nonempty_outputs() {
        let d = repo();
        for (index, compression) in [
            Compression::Lz4,
            Compression::Xz,
            Compression::Zlib,
            Compression::Brotli,
            Compression::Snappy,
            Compression::Bzip2,
        ]
        .into_iter()
        .enumerate()
        {
            let output = d.path().join(format!("bundle-{index}"));
            run(Config {
                repository: d.path().into(),
                output: output.clone(),
                format: ArchiveFormat::None,
                compression,
                report: ReportFormat::None,
                include_untracked: false,
                max_file_size: 100_000,
                excludes: vec![],
                includes: vec![],
                redact: true,
                fail_on_secret: false,
                dry_run: false,
                password: None,
                password_policy: PasswordPolicy::default(),
                password_file: None,
                verbose: false,
                quiet: true,
            })
            .unwrap();
            assert!(fs::metadata(output).unwrap().len() > 0);
        }
    }

    #[test]
    fn archive_has_no_secret() {
        let d = repo();
        let o = d.path().join("bundle.tar.zst");
        let r = run(Config {
            repository: d.path().into(),
            output: o.clone(),
            format: ArchiveFormat::Tar,
            compression: Compression::Zstd,
            report: ReportFormat::Markdown,
            include_untracked: false,
            max_file_size: 100000,
            excludes: vec![],
            includes: vec![],
            redact: true,
            fail_on_secret: false,
            dry_run: false,
            password: None,
            password_policy: PasswordPolicy::default(),
            password_file: None,
            verbose: false,
            quiet: true,
        })
        .unwrap();
        assert_eq!(r.redactions, 1);
        let bytes = fs::read(o).unwrap();
        let mut ar = tar::Archive::new(zstd::stream::read::Decoder::new(&bytes[..]).unwrap());
        let mut all = String::new();
        let mut names = Vec::new();
        for e in ar.entries().unwrap() {
            let mut e = e.unwrap();
            names.push(e.path().unwrap().to_string_lossy().into_owned());
            e.read_to_string(&mut all).ok();
        }
        assert!(!all.contains("not-a-real-secret"));
        assert!(all.contains("secretKeyRef"));
        assert!(names.iter().any(|name| name == ".git/COMMIT-HISTORY.txt"));
    }

    #[test]
    fn history_is_reverse_chronological_and_subject_only() {
        let d = repo();
        let history = git_history(d.path(), true).unwrap();
        assert!(history.lines().next().unwrap().split_whitespace().count() >= 2);
        assert!(!history.contains("Author"));
    }

    #[test]
    fn history_uses_subjects_and_redacts_sensitive_values() {
        let d = repo();
        Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "TOKEN=historical-secret\nsecond line",
            ])
            .current_dir(d.path())
            .status()
            .unwrap();
        let history = git_history(d.path(), true).unwrap();
        assert!(history.contains("[REDACTED]"));
        assert!(!history.contains("historical-secret"));
        assert!(!history.contains("second line"));
        assert_eq!(history.lines().count(), 2);
    }

    #[test]
    fn history_includes_each_commit_from_all_refs_once_in_topological_order() {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q"]);
        git(d.path(), &["branch", "-M", "main"]);
        git_config(d.path());
        commit(d.path(), "main root");
        git(d.path(), &["branch", "feature"]);
        commit(d.path(), "main before merge");
        git(d.path(), &["checkout", "-q", "feature"]);
        commit(d.path(), "TOKEN=feature-secret");
        commit(d.path(), "feature tip\n\nsecond line");
        git(d.path(), &["checkout", "-q", "main"]);
        git(
            d.path(),
            &["merge", "--no-ff", "-q", "feature", "-m", "merge feature"],
        );
        commit(d.path(), "main tip");
        git(d.path(), &["checkout", "-q", "--orphan", "tag-branch"]);
        git(d.path(), &["rm", "-q", "-rf", "."]);
        commit(d.path(), "tag-only commit");
        git(d.path(), &["tag", "tag-only"]);
        git(d.path(), &["checkout", "-q", "main"]);

        let history = git_history(d.path(), true).unwrap();
        let lines: Vec<_> = history.lines().collect();
        assert_eq!(lines.len(), 7);
        assert!(lines
            .iter()
            .any(|line| !line.contains("feature-secret") && line.contains("[REDACTED]")));
        assert!(lines.iter().any(|line| line.ends_with("feature tip")));
        assert!(lines.iter().any(|line| line.ends_with("tag-only commit")));
        assert!(lines.iter().any(|line| line.ends_with("merge feature")));
        let expected: Vec<_> = String::from_utf8_lossy(
            &Command::new("git")
                .args(["rev-list", "--all", "--topo-order"])
                .current_dir(d.path())
                .output()
                .unwrap()
                .stdout,
        )
        .lines()
        .map(|id| git(d.path(), &["rev-parse", "--short", id]))
        .collect();
        let actual: Vec<_> = lines
            .iter()
            .map(|line| line.split_once(' ').unwrap().0.to_owned())
            .collect();
        assert_eq!(actual.len(), expected.len());
        assert_eq!(
            actual
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            actual.len()
        );
        assert_eq!(
            actual
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>(),
            expected.into_iter().collect()
        );
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.ends_with("main root"))
                .count(),
            1
        );
        assert!(
            lines
                .iter()
                .position(|line| line.ends_with("main root"))
                .unwrap()
                < lines
                    .iter()
                    .position(|line| line.ends_with("merge feature"))
                    .unwrap()
        );
        assert_eq!(history, git_history(d.path(), true).unwrap());
        assert!(!history.contains("Test"));
        assert!(!history.contains("example.invalid"));
        assert!(!history.contains("refs/") && !history.contains("objects/"));
    }

    #[test]
    fn history_ignores_broken_refs_but_keeps_valid_history() {
        let d = repo();
        let broken = d.path().join(".git/refs/remotes/broken");
        fs::create_dir_all(broken.parent().unwrap()).unwrap();
        fs::write(&broken, "0000000000000000000000000000000000000000\n").unwrap();

        let history = git_history(d.path(), true).unwrap();
        assert_eq!(history.lines().count(), 1);
        assert!(history.contains("initial"));
    }

    fn git_config(path: &Path) {
        git(path, &["config", "user.name", "Test"]);
        git(path, &["config", "user.email", "test@example.invalid"]);
    }

    #[test]
    fn rejects_invalid_combinations_before_execution() {
        let config = Config {
            repository: PathBuf::from("."),
            output: PathBuf::from("out.zip"),
            format: ArchiveFormat::Zip,
            compression: Compression::None,
            report: ReportFormat::None,
            include_untracked: false,
            max_file_size: 1,
            excludes: vec![],
            includes: vec![],
            redact: true,
            fail_on_secret: false,
            dry_run: true,
            password: None,
            password_policy: PasswordPolicy::default(),
            password_file: None,
            verbose: false,
            quiet: true,
        };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn configured_password_policy_is_used_by_sanitizer_validation() {
        let mut config = Config {
            repository: PathBuf::from("."),
            output: PathBuf::from("out.zip"),
            format: ArchiveFormat::Zip,
            compression: Compression::Zstd,
            report: ReportFormat::None,
            include_untracked: false,
            max_file_size: 1,
            excludes: vec![],
            includes: vec![],
            redact: true,
            fail_on_secret: false,
            dry_run: true,
            password: Some("abcd".to_owned()),
            password_policy: PasswordPolicy {
                minimum_length: 4,
                require_uppercase: false,
                require_lowercase: true,
                require_number: false,
                require_special: false,
            },
            password_file: None,
            verbose: false,
            quiet: true,
        };
        assert!(validate_config(&config).is_ok());
        config.password_policy.require_uppercase = true;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn seven_zip_passwords_are_rejected_without_echoing_secret() {
        let secret = "NotPrinted7!";
        let config = Config {
            repository: PathBuf::from("."),
            output: PathBuf::from("out.7z"),
            format: ArchiveFormat::SevenZip,
            compression: Compression::None,
            report: ReportFormat::None,
            include_untracked: false,
            max_file_size: 1,
            excludes: vec![],
            includes: vec![],
            redact: true,
            fail_on_secret: false,
            dry_run: false,
            password: Some(secret.to_owned()),
            password_policy: PasswordPolicy::default(),
            password_file: None,
            verbose: false,
            quiet: false,
        };
        let error = validate_config(&config).unwrap_err().to_string();
        assert!(error.contains("only for ZIP AES"));
        assert!(!error.contains(secret));
    }

    #[test]
    fn cancellation_stops_before_scanning() {
        let d = tempdir().unwrap();
        git(d.path(), &["init", "-q"]);
        git_config(d.path());
        fs::write(d.path().join("input.txt"), "safe").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-q", "-m", "initial"]);
        let config = Config {
            repository: d.path().to_owned(),
            output: d.path().join("output.tar.zst"),
            format: ArchiveFormat::Tar,
            compression: Compression::Zstd,
            report: ReportFormat::None,
            include_untracked: false,
            max_file_size: 1024,
            excludes: vec![],
            includes: vec![],
            redact: true,
            fail_on_secret: false,
            dry_run: true,
            password: None,
            password_policy: PasswordPolicy::default(),
            password_file: None,
            verbose: false,
            quiet: true,
        };
        let result = run_with_progress(
            config,
            |_| panic!("cancelled run emitted progress"),
            || true,
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn progress_events_include_writing_and_finished_after_scanning() {
        let d = repo();
        let output = d.path().join("events.tar.zst");
        let mut events = Vec::new();
        run_with_progress(
            Config {
                repository: d.path().into(),
                output,
                format: ArchiveFormat::Tar,
                compression: Compression::Zstd,
                report: ReportFormat::None,
                include_untracked: false,
                max_file_size: 100_000,
                excludes: vec![],
                includes: vec![],
                redact: true,
                fail_on_secret: false,
                dry_run: false,
                password: None,
                password_policy: PasswordPolicy::default(),
                password_file: None,
                verbose: false,
                quiet: true,
            },
            |event| events.push(event),
            || false,
        )
        .unwrap();
        assert!(matches!(
            events.first(),
            Some(ProgressEvent::Scanning { .. })
        ));
        assert!(events
            .iter()
            .any(|event| matches!(event, ProgressEvent::Writing { .. })));
        assert!(matches!(events.last(), Some(ProgressEvent::Finished)));
    }

    fn commit(path: &Path, message: &str) {
        let file = format!("file-{}", message.len());
        fs::write(path.join(&file), message).unwrap();
        git(path, &["add", &file]);
        git(path, &["commit", "-q", "-m", message]);
    }

    fn git(path: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}
