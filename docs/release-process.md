# Release process

All ITSulu software CI, review gates, release builds, and release orchestration
run through ITSulu Forgejo. GitHub is a downstream source and release mirror
only. Forgejo runners and Garuda workstations produce and validate artifacts;
GitHub Actions and GitHub-hosted runners are not part of this process.

Create an annotated `vX.Y.Z` tag from merged Forgejo main, verify the exact tag
and binary version, build artifacts with `release/build-artifacts.sh`, verify
`SHA256SUMS`, and publish the directory with `release/publish-release.sh`.
The publisher reconciles both release objects and verifies public download
hashes; rerunning it retains matching assets and repairs missing or mismatched
assets.

## Required release artifact matrix

Every `vX.Y.Z` release publishes exactly these files, plus `SHA256SUMS`:

- Linux glibc x86_64: `rustrepo-sanitizer-<version>-linux-glibc-x86_64.tar.gz`
- Linux glibc aarch64: `rustrepo-sanitizer-<version>-linux-glibc-aarch64.tar.gz`
- Linux musl x86_64: `rustrepo-sanitizer-<version>-linux-musl-x86_64.tar.gz`
- Linux musl aarch64: `rustrepo-sanitizer-<version>-linux-musl-aarch64.tar.gz`
- Alpine x86_64: `rustrepo-sanitizer-<version>-alpine-x86_64.tar.gz`
- Alpine aarch64: `rustrepo-sanitizer-<version>-alpine-aarch64.tar.gz`
- Debian amd64: `rustrepo-sanitizer-<version>-linux-amd64.deb`
- RPM x86_64: `rustrepo-sanitizer-<version>-linux-x86_64.rpm`
- Arch x86_64: `rustrepo-sanitizer-<version>-arch-x86_64.pkg.tar.zst`
- Windows x86_64 ZIP: `rustrepo-sanitizer-<version>-windows-x86_64.zip`
- `SHA256SUMS`: hex digest and filename for every other artifact above

`SHA256SUMS` covers every other file in the matrix and never itself. The
publisher rejects a manifest that does not exactly cover the asset set, verifies
the checksums before upload, and re-verifies each artifact from the public
Forgejo and GitHub download URLs after publication. A release is incomplete
until all of these artifacts are present and match on both mirrors.

For 0.3.2, macOS source and Cargo installation are supported. Prebuilt macOS
artifacts are deferred until ITSulu has a native macOS build and test
environment. Native Windows 11 VM validation is planned for future ITSulu CI;
0.3.2 uses a Garuda cross-build validated with Wine.
