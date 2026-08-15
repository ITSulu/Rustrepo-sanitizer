# Benchmarks

The Criterion suite exercises expensive sanitization stages with synthetic Git
repositories: inventory/traversal, text secret scanning, redaction, SHA-256
hashing, archive writing, and the complete pipeline. Fixtures are created in a
temporary directory and contain only fake credentials.

Run the suite with:

```bash
cargo bench
```

Criterion writes HTML reports under `target/criterion/`. Use a quiet machine,
an unchanged build profile, and several repetitions before comparing results.
Record the CPU model, Rust version, command, repository size, file count, and
the reported throughput; absolute timings are not portable between systems.

| Fixture | Approximate shape | Purpose |
| --- | --- | --- |
| small | 100 small text files | Fixed overhead and traversal |
| medium | 1,000 mixed text files | Common review workload |
| large | 10,000 files / multi-MiB text | Streaming and throughput |

No baseline is committed because it would be misleading across machines. A
baseline should be captured on target CI or operator hardware before regression
thresholds are introduced.
