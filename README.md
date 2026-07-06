# RMRF — Resilient Modular Backup Framework

Backup tool with chunking, SHA256 deduplication, and Zstd compression.

## Features

- **Chunking**: 8KB fixed-size chunks
- **SHA256 deduplication**: identical chunks stored once
- **Zstd compression**: 4-100x ratio on text/repeated data
- **Manifest tracking**: each backup records its chunks in a versioned bincode manifest
- **Filesystem storage**: chunks organized by hash prefix (`ab/cdef...`)

## Quick Start

### Build

```bash
cargo build --workspace --release
```

### Backup

```bash
cargo run -- backup /path/to/file ./my-store
```

Output:
```
Backup: /path/to/file
File size: 5242880 bytes
.
Manifest saved: my-store/manifests/20260706T120000Z-file.bincode
Backup complete!
Chunks: 640
Total bytes: 5242880
```

### Restore

```bash
cargo run -- restore ./my-store/manifests/20260706T120000Z-file.bincode ./my-store
```

Output:
```
Restore: my-store/manifests/20260706T120000Z-file.bincode
Progress: 100%
Total chunks restored: 640
Total size: 5242880 bytes
Restore complete: file
```

## Architecture

```
FileSource ──> hash(SHA256) ──> CompressedChunkStore(Zstd level 10)
          ──> Manifest(version, source, chunks) ──> bincode serialization
                             │
                    manifests/<UTC>-<basename>.bincode
                    
Chunk store: <store>/chunks/<XX>/<rest-of-hex>
```

### Workspace

| Crate | Purpose |
|-------|---------|
| `rmrf-core` | Types (`Source`, `ChunkStore`, `DedupIndex`), traits, error type |
| `rmrf-source` | `FileSource` with real metadata (mtime, permissions) |
| `rmrf-store` | `LocalChunkStore` (filesystem), `CompressedChunkStore` (zstd wrapper), `MemoryDedupIndex`, `hash_data` (SHA256) |
| `rmrf-manifest` | `Manifest` save/load with bincode, version validation |
| `rmrf-cli` | Binary `rmrf` with `backup` and `restore` commands |

## Test Results

| Test suite | Tests | Status |
|------------|-------|--------|
| Unit tests (Rust) | 34 | all pass |
| Basic (shell) | 6 | bit-identical backup/restore |
| Deduplication | 4 | same file = no new chunks |
| Edge cases | 8 | empty, 1B, zeros, unicode, exact chunk |
| Sizes | 9 | 1KB to 5MB random data |
| Errors | 8 | missing files, corrupt manifests, permissions |

Compression example: 7000 bytes of repeated text → 53 bytes stored (132x ratio).

## Build & Test

```bash
cargo build --workspace          # Debug build
cargo build --workspace --release # Release build
cargo test --workspace           # Unit tests
cargo clippy --workspace         # Linter
cargo fmt                        # Formatter
bash scripts/test_all.sh         # Full integration suite
```
