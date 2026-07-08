# Changelog

## v0.1.0 (2026-07-08)

Initial release — validated on real Proxmox hardware.

### Features

- **Content-defined chunking** — FastCDC with configurable min/avg/max sizes
- **BLAKE3 hashing** — fast, cryptographically secure chunk addressing
- **zstd compression** — per-chunk compression with configurable level
- **Cross-run deduplication** — PersistentDedupIndex avoids re-storing identical chunks across backup runs
- **VM backup (Proxmox QEMU)** — validated: suspend → backup → resume, bit-à-bit restore
- **VM backup (Proxmox LXC)** — validated: same workflow as QEMU
- **VM backup (libvirt/KVM)** — implemented but **not validated** on real hardware
- **Encryption** — age-based (backup with `--encrypt <recipient>`, restore with `--identity <keyfile>`)
- **Retention policy** — `rmrf retention --days N` with `--dry-run` preview
- **Garbage collection** — `rmrf gc` removes orphan chunks with `--dry-run`
- **SIGINT resilience** — graceful shutdown: VM resumed, sentinel cleaned, lock released
- **SIGKILL resilience** — sentinel-based auto-recovery: `rmrf check` detects stale sentinels and resumes VMs
- **Anti-race guard** — `check_stale_sentinels` skips if store lock is held by a live process
- **File/directory backup** — recursive directory backup with per-file dedup
- **Backup listing** — `rmrf list` with date and source filters
- **Integrity verification** — `rmrf verify` checks all chunks exist and are readable
- **Multi-hypervisor auto-detection** — detects Proxmox QEMU, Proxmox LXC, or libvirt from available tools

### Limitations

- **libvirt/KVM**: implemented but not validated on real hardware (no test machine available)
- **QMP live backup** (`--live`): disabled — blockdev-snapshot-sync creates overlay disks with no merge-back path
- **No built-in scheduler**: use systemd timers or cron (see USAGE.md)
- **SIGKILL during ext4 sync**: a process killed during `sync_all()` may remain in D-state until the journal commits; the sentinel guard prevents premature VM resume
- **Orphan chunks**: SIGKILL during backup leaves orphan chunks (no manifest) — cleaned by `rmrf gc`

### Commands

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
