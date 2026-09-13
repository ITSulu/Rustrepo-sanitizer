# Format backend decisions

The sanitizer separates content generation, container creation, compression,
and encryption. This note records the current backend choices so a format is
not enabled merely because a decoder or an unmaintained crate exists.

| Output | Backend | Status | Rationale |
|---|---|---|---|
| TAR + gzip/Zstandard | `tar`, `flate2`, `zstd` | internal Rust | Mature streaming encoders and deterministic member metadata. |
| ZIP + Deflate/Zstandard | `zip` 2.x | internal Rust | Standard container; AES-256 support is available through the crate's `aes-crypto` feature. |
| 7z | `7z` executable | external fallback | No suitable maintained Rust 7z writer was selected; invocation uses structured arguments and validates exit status. Password mode is disabled because passing a password as a process argument can leak it. |
| TAR + LZ4/lzip/LZMA/LZO/lrzip/XZ | OS executable fallback | implemented when tool is installed | Uses structured stdin/stdout processes, validates exit status, and removes intermediate TAR data. |

`compcol` was evaluated from its upstream repository and crates.io release
metadata. It is pure Rust, `no_std`, feature-gated, streaming-oriented, and
actively released, with gzip, LZ4, LZMA/XZ, LZO, and Zstandard components.
It is not yet adopted because each requested codec still needs independent
standards-interoperability tests, bounded-memory measurements, and archive
integration tests. In particular, codec support does not provide a container
format or encryption.

References:

- <https://github.com/KarpelesLab/compcol>
- <https://crates.io/crates/compcol>
- <https://docs.rs/zip/latest/zip/write/struct.FileOptions.html>
