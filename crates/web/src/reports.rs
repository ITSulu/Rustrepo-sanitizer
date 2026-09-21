//! Extracts the human-readable reports that the sanitizer embeds in its output
//! archive so the browser can download them individually.
//!
//! The archive is produced by our own core, so entries are trusted, but we
//! still only read a fixed allow-list of reporter filenames.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use itsulu_repo_sanitizer::sanitizer::{ArchiveFormat, Compression};

/// Report members the web UI can offer for download. The names come from the
/// shared core so the two cannot drift.
pub const REPORT_MEMBERS: &[&str] = itsulu_repo_sanitizer::sanitizer::REPORT_MEMBERS;

fn member_is_report(name: &str) -> bool {
    REPORT_MEMBERS.contains(&name)
}

/// Extracts report members from the archive into `dest`, returning the written
/// paths. Unsupported containers yield an empty list.
pub fn extract_reports(
    archive: &Path,
    format: ArchiveFormat,
    compression: Compression,
    dest: &Path,
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dest).context("creating report directory")?;
    match format {
        ArchiveFormat::Zip => extract_from_zip(archive, dest),
        ArchiveFormat::Tar => extract_from_tar(archive, compression, dest),
        // `none` is a JSONL stream and `7z` needs an external tool; neither is
        // unpacked here.
        ArchiveFormat::None | ArchiveFormat::SevenZip => Ok(Vec::new()),
    }
}

fn writer_for(dest: &Path, name: &str) -> Result<std::fs::File> {
    let path = dest.join(name);
    std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))
}

fn extract_from_zip(archive: &Path, dest: &Path) -> Result<Vec<PathBuf>> {
    let file = std::fs::File::open(archive).context("opening zip archive")?;
    let mut zip = zip::ZipArchive::new(file).context("reading zip archive")?;
    let mut written = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name().to_owned();
        if !member_is_report(&name) {
            continue;
        }
        let mut file = writer_for(dest, &name)?;
        std::io::copy(&mut entry, &mut file)?;
        written.push(dest.join(&name));
    }
    Ok(written)
}

fn extract_from_tar(archive: &Path, compression: Compression, dest: &Path) -> Result<Vec<PathBuf>> {
    let file = std::fs::File::open(archive).context("opening tar archive")?;
    let reader: Box<dyn Read> = match compression {
        Compression::Zstd => Box::new(zstd::stream::read::Decoder::new(file)?),
        Compression::Gzip => Box::new(flate2::read::GzDecoder::new(file)),
        _ => Box::new(file),
    };
    let mut tar = tar::Archive::new(reader);
    let mut written = Vec::new();
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let name = path.to_string_lossy().into_owned();
        if !member_is_report(&name) {
            continue;
        }
        let mut file = writer_for(dest, &name)?;
        std::io::copy(&mut entry, &mut file)?;
        written.push(dest.join(&name));
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_reports_from_zip() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("out.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("SANITIZATION-REPORT.md", options).unwrap();
            zip.write_all(b"# report").unwrap();
            zip.start_file("src/main.rs", options).unwrap();
            zip.write_all(b"fn main() {}").unwrap();
            zip.finish().unwrap();
        }
        let reports = extract_reports(
            &archive,
            ArchiveFormat::Zip,
            Compression::None,
            &dir.path().join("reports"),
        )
        .unwrap();
        assert_eq!(reports.len(), 1);
        assert!(reports[0].ends_with("SANITIZATION-REPORT.md"));
        assert!(!dir.path().join("reports/src/main.rs").exists());
    }

    #[test]
    fn extracts_reports_from_tar_zstd() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("out.tar.zst");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let encoder = zstd::stream::write::Encoder::new(file, 3).unwrap();
            let mut tar = tar::Builder::new(encoder);
            let data = b"# report";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "SANITIZATION-REPORT.json", &data[..])
                .unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let reports = extract_reports(
            &archive,
            ArchiveFormat::Tar,
            Compression::Zstd,
            &dir.path().join("reports"),
        )
        .unwrap();
        assert_eq!(reports.len(), 1);
        assert!(dir
            .path()
            .join("reports/SANITIZATION-REPORT.json")
            .is_file());
    }

    #[test]
    fn unsupported_formats_yield_no_reports() {
        let dir = tempfile::tempdir().unwrap();
        assert!(extract_reports(
            &dir.path().join("missing.jsonl"),
            ArchiveFormat::None,
            Compression::Gzip,
            &dir.path().join("reports")
        )
        .unwrap()
        .is_empty());
    }
}
