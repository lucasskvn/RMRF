# RMRF — Resilient Modular Backup Framework

Backup tool with FastCDC chunking, BLAKE3 deduplication, zstd compression, and Proxmox/Libvirt VM support.

## Features

- **FastCDC chunking**: variable-size chunks (4 KB / 16 KB / 64 KB avg/max)
- **BLAKE3 deduplication**: identical chunks stored once, persistent cross-run index
- **Zstd compression**: chunks compressed at level 10
- **Manifest tracking**: each backup records its chunks in a versioned bincode manifest (v2)
- **VM backup**: Proxmox (`qm suspend/resume`) and Libvirt (`virsh suspend/resume`) with auto-detection
- **Directory backup**: recursive, one manifest per file
- **Retention + GC**: `--retain <days>` deletes old manifests and runs garbage collection
- **Safety**: `restore --force` required to overwrite existing files

## Quick Start

### Build

```bash
cargo build --workspace --release
```

### Backup a file

```bash
./target/release/rmrf backup /path/to/file -s /path/to/store
```

### Backup a directory (recursive)

```bash
./target/release/rmrf backup /path/to/dir -s /path/to/store
```

### Backup a Proxmox VM

```bash
./target/release/rmrf backup /mnt/pve/TrueNAS/images/103/vm-103-disk-1.qcow2 \
  -s /var/lib/rmrf --vm 103 --hypervisor proxmox --format qcow2
```

The VM is suspended during backup, then resumed.

### Restore

```bash
./target/release/rmrf restore <manifest> -s /path/to/store -o /tmp/restored
```

### Other commands

```bash
rmrf list -s /path/to/store              # list all backups
rmrf info <manifest> -s /path/to/store   # show backup details
rmrf verify <manifest> -s /path/to/store # check all chunks exist
rmrf gc -s /path/to/store --dry-run      # find orphan chunks
rmrf gc -s /path/to/store                # delete orphan chunks
rmrf clean                               # remove test artifacts
```

See **[USAGE.md](USAGE.md)** for the full guide.

## Architecture

```
Source (File/VmDisk)
  → FastCDC (variable chunks: 4KB-64KB)
  → BLAKE3 (hash)
  → DedupIndex (persistent: index.bincode)
      found → increment refcount, skip
      new   → zstd compress → LocalChunkStore → insert in index
  → Manifest (v2, bincode) → manifests/<UTC>-<basename>.bincode

Chunk store: <store>/chunks/<XX>/<rest-of-hex>.zst
```

### Workspace

| Crate | Purpose |
|-------|---------|
| `rmrf-core` | Types (`Source`, `ChunkStore`, `DedupIndex`), traits, `RmrfError` |
| `rmrf-source` | `FileSource`, `VmSource<L>`, `VmLifecycle` (Noop/Proxmox/Libvirt) |
| `rmrf-store` | `LocalChunkStore`, `CompressedChunkStore` (zstd), `PersistentDedupIndex`, `hash_data` (BLAKE3) |
| `rmrf-manifest` | `Manifest` save/load with bincode, version validation (v2) |
| `rmrf-cli` | Binary `rmrf` with `backup`, `restore`, `list`, `info`, `verify`, `gc`, `clean` |

## Test Results

| Test suite | Tests | Status |
|------------|-------|--------|
| Unit tests (Rust) | 45 | all pass |
| Basic (shell) | 6 | bit-identical backup/restore |
| Deduplication | 4 | same file = no new chunks |
| Edge cases | 8 | empty, 1B, zeros, unicode, exact chunk |
| Sizes | 9 | 1KB to 5MB random data |
| Errors | 8 | missing files, corrupt manifests, permissions |
| VM mock | 5 | ProxmoxLifecycle code path (mock qm) |
| Proxmox real | 13 | VM 103, qm suspend/resume, 3.2GB→340MB |

Compression example: 3.2 GB qcow2 VM disk → 340 MB store (10.5% ratio).

## Build & Test

```bash
cargo build --workspace          # Debug build
cargo build --workspace --release # Release build
cargo test --workspace           # Unit tests (45)
cargo clippy --workspace         # Linter (0 warnings)
bash scripts/test_all.sh         # Full integration suite (35+ shell tests)
bash scripts/test_proxmox.sh 103 # Real Proxmox test (requires root + qm)
```
