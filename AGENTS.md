# Repository Agent Instructions

## Communication

- Provide only meaningful delta updates during long-running work.
- Do not send counter-only status updates. Report a counter only when the work finishes or a concrete issue occurs.
- Whenever a goal stalls, pauses, transfers, or becomes blocked, provide a cumulative goal update containing all important verified information from the current goal, not only the latest delta.

## Releases

Every `vX.Y.Z` release must publish the complete artifact matrix in
[docs/release-process.md](docs/release-process.md): Linux glibc and musl
archives for x86_64 and aarch64, Alpine archives for x86_64 and aarch64, a
Debian amd64 package, an RPM x86_64 package, an Arch x86_64 package, a Windows
x86_64 ZIP, and `SHA256SUMS`. Do not publish a release that is missing any of
these artifacts on Forgejo or the GitHub mirror.
