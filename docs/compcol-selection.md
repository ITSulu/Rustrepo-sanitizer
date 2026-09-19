# compcol compressor selection

Rustrepo-sanitizer uses compcol `0.6.11` only where the project has verified a
useful byte-stream encoder and a deterministic output extension. The factory's
`names()` list is not sufficient evidence: compcol also lists codecs whose
encoder is deliberately unsupported or whose implementation is only a raw
payload/filter/transform.

## User-selectable direct streams

The currently verified `Archive = none` compcol streams are:

| Codec | Extension | Verification status |
| --- | --- | --- |
| gzip | `.gz` | compcol round trip and standard stream use |
| zlib | `.zz` | compcol round trip and standard stream use |
| XZ | `.xz` | compcol round trip and XZ container |
| Brotli | `.br` | compcol round trip |
| Snappy | `.sz` | compcol raw-block round trip; framed interoperability is not claimed |
| bzip2 | `.bz2` | compcol round trip |

LZ4 is available for TAR through the existing backend, but compcol's framing
is not claimed to be a canonical LZ4 frame. The registry therefore does not
advertise unverified interoperability.

## Deliberate exclusions

The following are not selectable until project-level framing and
interoperability evidence exists:

* decoder-only or deliberately unsupported encoders: Quantum, LZFSE, PPMd,
  LZHAM, Arsenic, RAR variants, SIT13, and LZAH;
* raw payload methods without a complete file container: LZMA2, LHA methods,
  and ARC Crunch/Squeeze/Squash;
* filters or transforms: BCJ, BCJ2, Delta, MTF, and BWT;
* protocol/header codecs: HPACK and QPACK;
* primitive or entropy codecs without a useful interoperable file format:
  generic Huffman, HPACK Huffman, and range coder.

Additional candidates such as Deflate, Deflate64, LZMA, LZW, LZO, RLE,
PackBits, Xpress, LZNT1, and related codecs require dedicated encoder,
round-trip, extension, archive-compatibility, and reference-tool tests before
being added to the public capability registry.
