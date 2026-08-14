//! End-to-end Criterion measurements against the shipped CLI.
//!
//! The repositories intentionally use only fake credentials.  Keeping this at
//! the process boundary makes the measurements cover Git discovery, traversal,
//! redaction, hashing, report creation, compression, and archive writing.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tempfile::TempDir;

const EXE: &str = env!("CARGO_BIN_EXE_itsulu-repo-sanitizer");

struct Fixture {
    _temp: TempDir,
    repo: PathBuf,
}

fn git(path: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(path)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run git");
    assert!(status.success(), "git command failed: {args:?}");
}

fn fixture(files: usize, bytes_per_file: usize) -> Fixture {
    let temp = TempDir::new().expect("temporary fixture directory");
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).expect("create fixture repository");
    git(&repo, &["init", "--quiet"]);
    git(
        &repo,
        &["config", "user.email", "benchmark@example.invalid"],
    );
    git(&repo, &["config", "user.name", "Benchmark"]);
    let payload = format!(
        "# synthetic benchmark input\nSERVICE_TOKEN=benchmark-token-value-is-fake\n{}\n",
        "x".repeat(bytes_per_file.saturating_sub(72))
    );
    for index in 0..files {
        let directory = repo.join(format!("src/{:03}", index % 100));
        fs::create_dir_all(&directory).expect("create source directory");
        fs::write(directory.join(format!("file-{index:05}.txt")), &payload)
            .expect("write synthetic input");
    }
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "fixture"]);
    Fixture { _temp: temp, repo }
}

fn invoke(repo: &Path, dry_run: bool) {
    let output = repo
        .parent()
        .expect("fixture parent")
        .join("review.tar.zst");
    if !dry_run {
        let _ = fs::remove_file(&output);
    }
    let mut command = Command::new(EXE);
    command.arg("sanitize").arg(repo).arg("--quiet");
    if dry_run {
        command.arg("--dry-run");
    } else {
        command.arg("--output").arg(output);
    }
    let status = command.status().expect("run sanitizer");
    assert!(status.success(), "sanitizer invocation failed");
}

fn sanitize_benches(c: &mut Criterion) {
    let cases = [
        ("small", 100, 1024),
        ("medium", 1_000, 2048),
        ("large", 10_000, 1024),
    ];
    let mut group = c.benchmark_group("sanitize_pipeline");
    for (name, files, bytes_each) in cases {
        let fixture = fixture(files, bytes_each);
        let bytes = (files * bytes_each) as u64;
        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(
            BenchmarkId::new("inventory_traversal", name),
            &fixture,
            |b, f| {
                b.iter(|| invoke(&f.repo, true));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("scan_redact_hash_archive", name),
            &fixture,
            |b, f| b.iter(|| invoke(&f.repo, false)),
        );
    }
    group.finish();
}

criterion_group!(benches, sanitize_benches);
criterion_main!(benches);
