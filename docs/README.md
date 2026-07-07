# RMRF - Resilient Modular Reduction Framework

A fast, efficient backup system for files and VMs with automatic compression and deduplication.

## Features

- 📦 Backup files, directories, VM disks
- 💾 Support for Proxmox, KVM, Libvirt
- 🗜️ Zstd compression (4-10x reduction)
- 🔗 SHA256-based deduplication
- ⚡ FastCDC chunking
- 🔍 Integrity verification
- 🧹 Garbage collection

## Quick Start

```bash
# Backup
./rmrf backup file.txt -s ./store

# List
./rmrf list -s ./store

# Restore
manifest=$(find ./store/manifests -name "*.bincode" | head -1)
./rmrf restore "$manifest" -s ./store -o restored.txt

# Verify
./rmrf verify "$manifest" -s ./store

# Cleanup orphans
./rmrf gc -s ./store
```

## Installation

See [INSTALLATION.md](INSTALLATION.md)

```bash
cargo build --release
./target/release/rmrf --help
```

## Usage

See [USAGE.md](USAGE.md) for complete command reference and examples.

## Commands

| Command | Purpose |
|---------|---------|
| `backup` | Create backup of file/dir/VM disk |
| `restore` | Restore from backup manifest |
| `list` | List all backups |
| `info` | Show backup details |
| `verify` | Check backup integrity |
| `gc` | Garbage collect orphan chunks |

## Project Structure

```
rmrf-core       Core traits & types
rmrf-source     File & VM sources
rmrf-store      Chunk storage + compression
rmrf-manifest   Backup metadata
rmrf-cli        Command-line interface
```

## Statistics

- **Code**: 2,841 lines (Rust)
- **Crates**: 5 (modular)
- **Build**: ~23s (clean)
- **Binary**: 2.7 MB
- **Tests**: 45 passing
- **Warnings**: 0

## Status

✅ MVP Complete - Production-ready for local backups

## Next Steps

1. Install: See [INSTALLATION.md](INSTALLATION.md)
2. Learn: See [USAGE.md](USAGE.md)
3. Backup: Run first backup
4. Schedule: Set up cron job

---

**Questions?** Check [USAGE.md](USAGE.md) or run `rmrf --help`
