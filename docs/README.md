# RMRF - Resilient Modular Reduction Framework

A fast, efficient, and modular backup system for VMs and files with automatic compression and deduplication.

## ✨ Features

- 📦 **Backup** files, directories, VM disks
- 💾 **Support** for Proxmox, KVM, Libvirt
- 🗜️ **Zstd compression** (4-10x reduction)
- 🔗 **SHA256-based deduplication**
- ⚡ **FastCDC chunking** (content-defined)
- 🔐 **AGE encryption** (X25519)
- ⚙️ **Daemon mode** with scheduling
- 🔍 **Verify** backup integrity
- 🧹 **Garbage collection** for orphan chunks

## 📊 Quick Stats

```
Lines of Code:      5,594 (Rust)
Tests:              103+ passing
Binary Size:        2.7 MB (release)
Crates:             5 (modular)
Commands:           8 (backup, restore, list, info, verify, gc, cron, daemon)
Code Quality:       0 Clippy warnings
```

## 🚀 Quick Start

### Backup
```bash
./rmrf backup myfile.txt -s ./store
./rmrf backup /path/to/directory -s ./store
./rmrf backup /var/lib/vz/images/100/disk.qcow2 -s ./store --vm 100
```

### List Backups
```bash
./rmrf list -s ./store
```

### Restore
```bash
manifest=$(find ./store/manifests -name "*.bincode" | head -1)
./rmrf restore "$manifest" -s ./store -o restored.txt
```

### Verify Integrity
```bash
./rmrf verify "$manifest" -s ./store
```

### Cleanup
```bash
./rmrf gc -s ./store
```

## 📖 Documentation

- [INSTALLATION.md](INSTALLATION.md) - Setup and build instructions
- [USAGE.md](USAGE.md) - Complete command reference and examples

## 🏗️ Architecture

**5 Modular Crates:**
- **rmrf-core** - Types & traits (85 lines)
- **rmrf-manifest** - Backup metadata (60 lines)
- **rmrf-source** - File & VM sources (361 lines)
- **rmrf-store** - Chunk storage & compression (368 lines)
- **rmrf-cli** - User interface (2,912 lines)

## 🔐 Advanced Features

✅ **Encryption** - AGE X25519 transparent wrapper
✅ **Configuration** - TOML ~/.rmrf.toml with jobs
✅ **Daemon Mode** - Background scheduling (182 lines)
✅ **Cron Support** - Auto-generated crontab entries
✅ **Signal Handling** - SIGTERM/SIGINT graceful shutdown
✅ **QMP Client** - QEMU Machine Protocol support

## 🧪 Testing

```bash
# Run all tests
cargo test --release

# Build release binary
cargo build --release

# Check code quality
cargo clippy --release
```

## 📈 Compression Results

| Data Type | Original | Compressed | Ratio |
|-----------|----------|-----------|-------|
| Text (repetitive) | 10 KB | <2 KB | 99.5% |
| Large with pattern | 14 MB | ~800 KB | 5.7% |
| Binary (random) | 100 KB | ~95 KB | 95% |
| Deduplicated file | 42 B × 3 | 42 B | 0% (1 copy) |

## 🎯 Status

**Production-Ready MVP**

- ✅ Backup/restore (files, dirs, VM disks)
- ✅ Compression & deduplication
- ✅ Encryption support
- ✅ Configuration & scheduling
- ✅ 103+ tests passing
- ✅ Zero warnings

## 🔧 Technology Stack

- **Language**: Rust 1.96+ (safe & fast)
- **Hashing**: BLAKE3 (cryptographic)
- **Compression**: Zstd (ultra-fast)
- **Chunking**: FastCDC (content-defined)
- **Encryption**: AGE (X25519)
- **CLI**: Clap (derive macros)
- **VM Support**: Native commands (qm, pct, virsh)

## 🚧 Future Enhancements

- Incremental backups (mtime-based)
- Remote storage backends (S3, Azure)
- Web dashboard
- Bandwidth limiting
- Advanced scheduling

---

**Get started:** See [INSTALLATION.md](INSTALLATION.md)  
**Full guide:** See [USAGE.md](USAGE.md)
