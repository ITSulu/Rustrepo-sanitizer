use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tar::{Builder, Header};

use crate::security::{
    default_exclusion, is_binary, is_kubernetes_secret_manifest, redact_text, safe_archive_path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveFormat {
    TarGz,
    TarZst,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportFormat {
    Markdown,
    Json,
    None,
}
pub struct Config {
    pub repository: PathBuf,
    pub output: PathBuf,
    pub format: ArchiveFormat,
    pub report: ReportFormat,
    pub include_untracked: bool,
    pub max_file_size: u64,
    pub excludes: Vec<String>,
    pub includes: Vec<String>,
    pub redact: bool,
    pub fail_on_secret: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub quiet: bool,
}
pub struct Summary {
    pub included: usize,
    pub excluded: usize,
    pub redactions: usize,
    pub dry_run: bool,
    pub quiet: bool,
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
#[derive(Serialize)]
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

pub fn run(config: Config) -> Result<Summary> {
    let root = fs::canonicalize(&config.repository).context("repository path does not exist")?;
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
    for relative in git_files(&root, config.include_untracked)? {
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
    if !config.dry_run {
        write_archive(&output, config.format, &files, &manifest, config.report)?;
    }
    Ok(Summary {
        included: files.len(),
        excluded: manifest.exclusions.len(),
        redactions,
        dry_run: config.dry_run,
        quiet: config.quiet,
    })
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
        args.extend(["--others", "--exclude-standard"]);
    }
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .context("running git ls-files")?;
    if !output.status.success() {
        bail!("git ls-files failed");
    }
    Ok(output
        .stdout
        .split(|b| *b == 0)
        .filter(|x| !x.is_empty())
        .map(|x| PathBuf::from(String::from_utf8_lossy(x).into_owned()))
        .collect())
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
fn write_archive(
    output: &Path,
    format: ArchiveFormat,
    files: &[(String, Vec<u8>, ManifestFile)],
    manifest: &Manifest,
    report: ReportFormat,
) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = output.with_extension(format!("partial-{}", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("creating {}", temporary.display()))?;
    let writer: Box<dyn Write> = match format {
        ArchiveFormat::TarGz => Box::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::default(),
        )),
        ArchiveFormat::TarZst => {
            Box::new(zstd::stream::write::Encoder::new(file, 3)?.auto_finish())
        }
    };
    let result = (|| -> Result<()> {
        let mut tar = Builder::new(writer);
        let mut sums = BTreeMap::new();
        for (name, data, entry) in files {
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
        d
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
    fn archive_has_no_secret() {
        let d = repo();
        let o = d.path().join("bundle.tar.zst");
        let r = run(Config {
            repository: d.path().into(),
            output: o.clone(),
            format: ArchiveFormat::TarZst,
            report: ReportFormat::Markdown,
            include_untracked: false,
            max_file_size: 100000,
            excludes: vec![],
            includes: vec![],
            redact: true,
            fail_on_secret: false,
            dry_run: false,
            verbose: false,
            quiet: true,
        })
        .unwrap();
        assert_eq!(r.redactions, 1);
        let bytes = fs::read(o).unwrap();
        let mut ar = tar::Archive::new(zstd::stream::read::Decoder::new(&bytes[..]).unwrap());
        let mut all = String::new();
        for e in ar.entries().unwrap() {
            let mut e = e.unwrap();
            e.read_to_string(&mut all).ok();
        }
        assert!(!all.contains("not-a-real-secret"));
        assert!(all.contains("secretKeyRef"));
    }
}
