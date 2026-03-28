# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.2](https://github.com/ArthurBrussee/serde_ply/compare/v0.2.1...v0.2.2) - 2026-03-27

### Performance

- Binary deserialization of compatible structs (no lists, binary, and no serde features too advanced) now uses `visit_seq` with bulk reads, eliminating key matching overhead (+40% faster on some benchmark)
- Reuse header line buffer across iterations instead of allocating per line
- Use `write_all` instead of `write!` for constant strings in ASCII serializer

### Fixed

- Validate list counts are non-negative

### Other

- Removed redundant `BufReader` wrapping in `from_bytes`

## [0.2.1](https://github.com/ArthurBrussee/serde_ply/compare/v0.2.0...v0.2.1) - 2025-08-13

### Fixed

- Handle parsing header when lines end in \r\n

## [0.2.0](https://github.com/ArthurBrussee/serde_ply/compare/v0.1.1...v0.2.0) - 2025-08-13

### Added

- Rename some API to be more consistent (ChunkPlyFile -> PlyChunkedReader, PlyFileDeserializer -> PlyReader)
- Improve API & documentation
- Serialize with "SerializeOptions" builder for a terser API & ability to set `obj_info`.

### Other

- Add doc tests
- Fix release-plz
- Removed ListCountU8 as it's the default anyway

## [0.1.1](https://github.com/ArthurBrussee/serde_ply/compare/v0.1.0...v0.1.1) - 2025-08-12

### Fixed

- Correct license

### Other

- Change GitHub link again

## [0.1.0]

### Added
- Initial release
