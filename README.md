# RMRF — Resilient Modular Reduction Framework

Chunk-based deduplicated backup tool for files and VM disks. Uses FastCDC (content-defined chunking), BLAKE3 hashing, and zstd compression. Supports Proxmox (QEMU/LXC) VM suspension for consistent snapshots.

## Quick Start

```bash
# Build
cargo build --release

# Backup a file
./target/release/rmrf backup myfile.txt -s ./store

# Restore
manifest=$(find ./store/manifests -name "*.bincode" | head -1)
./target/release/rmrf restore "$manifest" -s ./store -o restored.txt

# Verify
diff myfile.txt restored.txt && echo "Identical!"
```

## Validation Status

| Hypervisor | Status |
|------------|--------|
| **Proxmox QEMU** | Validated on real hardware — backup/restore bit-à-bit, SIGKILL resilience with auto-recovery, encryption |
| **Proxmox LXC** | Validated on real hardware — same tests as QEMU |
| **libvirt / KVM** | Implemented but **not validated** on real hardware (no test machine available). Use with caution. Contributions and feedback welcome. |

## Commands

| Command | Purpose |
|---------|---------|
| `backup` | Create backup of file, directory, or VM disk |
| `restore` | Restore from backup manifest |
| `list` | List all backups in store |
| `info` | Show detailed backup metadata |
| `verify` | Check backup integrity |
| `gc` | Garbage collect orphan chunks |
| `retention` | Remove manifests older than N days |
| `check` | Auto-resume VMs left suspended by interrupted backups |

## Documentation

- [Installation Guide](docs/INSTALLATION.md)
- [Usage Guide](docs/USAGE.md) — full command reference
- [License](LICENSE) — MIT
