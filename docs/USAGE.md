# Usage Guide

Complete reference for all RMRF commands with practical examples.

## Quick Reference

| Command | Purpose |
|---------|---------|
| `backup` | Create backup of file, directory, or VM disk |
| `restore` | Restore file from backup manifest |
| `list` | List all backups in store |
| `info` | Show detailed backup metadata |
| `verify` | Check backup integrity (all chunks exist) |
| `gc` | Garbage collect orphan chunks |
| `check` | Check for stale VM sentinels and auto-resume suspended VMs |
| `retention` | Remove manifests older than N days |

---

## BACKUP Command

Create backups of files, directories, or VM disks.

### Basic File Backup
```bash
rmrf backup myfile.txt -s ./my-store
```

Output:
```
Backup: myfile.txt
Source size: 1234 bytes
Chunks: 1
Total bytes: 1234
Dedup skipped (already stored): 0
Manifest saved: ./my-store/manifests/20260707T084548Z-myfile.txt.bincode
Backup complete!
```

### Backup Large File
```bash
rmrf backup large-video.mp4 -s ./media-backups
```

For large files, FastCDC creates multiple chunks automatically.

### Backup Directory
```bash
rmrf backup ./documents -s ./backups
```

Recursively backs up all files in directory, with deduplication across files.

### Backup with Retention
```bash
rmrf backup file.txt -s ./store --retain 30
```

Creates backup, removes manifests older than 30 days, runs garbage collection.

### Backup VM Disk (Proxmox)
```bash
# QEMU VM
rmrf backup /var/lib/vz/images/100/vm-100-disk-0.qcow2 \
  -s ./vm-backups --vm 100

# LXC Container
rmrf backup /var/lib/vz/images/201/vm-201-disk-0.raw \
  -s ./vm-backups --vm 201
```

System will:
1. Suspend the VM (pause execution)
2. Read disk snapshot
3. Resume the VM immediately
4. Backup chunks with compression & dedup

### Backup VM with Libvirt
```bash
rmrf backup /var/lib/libvirt/images/ubuntu.qcow2 \
  -s ./vm-backups --vm ubuntu-vm --hypervisor libvirt
```

---

## RESTORE Command

Restore files from backup manifests.

### Basic Restore
```bash
# Find manifest
manifest=$(find ./my-store/manifests -name "*.bincode" | head -1)

# Restore
rmrf restore "$manifest" -s ./my-store -o restored-file.txt
```

### Restore to Custom Location
```bash
manifest=$(find ./my-store/manifests -name "*myfile*" | head -1)
rmrf restore "$manifest" -s ./my-store -o /tmp/my-backup.txt
```

### Restore with Overwrite
```bash
# File exists - use --force
rmrf restore "$manifest" -s ./my-store -o existing-file.txt --force
```

### Verify After Restore
```bash
# Restore
manifest=$(find ./my-store/manifests -name "*.bincode" | head -1)
rmrf restore "$manifest" -s ./my-store -o restored.txt

# Verify
diff original.txt restored.txt && echo "✅ Identical!"
```

---

## LIST Command

Show all backups in the store.

### List All Backups
```bash
rmrf list -s ./my-store
```

Output:
```
20260706T190116Z       11 B    1 chunks  test.txt
20260707T084548Z  100.0 KiB    5 chunks  test_binary.bin
20260707T084548Z   13.5 MiB  217 chunks  test_large_zeros.txt
```

**Columns:** Timestamp | Compressed Size | Chunk Count | Filename

### Search Backups
```bash
# Find specific backup
rmrf list -s ./my-store | grep "test.txt"

# Count total
rmrf list -s ./my-store | wc -l
```

### Store Statistics
```bash
echo "Store size: $(du -sh ./my-store)"
echo "Total backups: $(find ./my-store/manifests -name '*.bincode' | wc -l)"
```

---

## INFO Command

Show detailed backup metadata.

### Display Backup Info
```bash
manifest=$(find ./my-store/manifests -name "*.bincode" | head -1)
rmrf info "$manifest" -s ./my-store
```

Output:
```
Version:     v2
Source path: test_large_zeros.txt
Source size: 14164977 (13.5 MiB)
Source type: File
Modified:    1783413932
Permissions: 100664
Chunks:      217
```

---

## VERIFY Command

Check backup integrity.

### Verify Backup
```bash
manifest=$(find ./my-store/manifests -name "*.bincode" | head -1)
rmrf verify "$manifest" -s ./my-store
```

**Success:**
```
OK: all 217 chunks present and verified
```

**Failure:**
```
FAIL: 5/217 chunks missing
Missing chunk: 3df651e1a...
```

### Verify All Backups
```bash
for manifest in ./my-store/manifests/*.bincode; do
    echo "Verifying $manifest..."
    rmrf verify "$manifest" -s ./my-store
done
```

---

## GC Command (Garbage Collection)

Clean up orphan chunks.

### Dry Run
```bash
rmrf gc -s ./my-store --dry-run
```

Output:
```
Dry run: would delete 12 orphan chunks, would free 5.2 MiB
```

### Actually Delete
```bash
rmrf gc -s ./my-store
```

Output:
```
Deleted 12 orphan chunks, freed 5.2 MiB
```

### Monitor Storage
```bash
echo "Before GC:"
du -sh ./my-store

rmrf gc -s ./my-store

echo "After GC:"
du -sh ./my-store
```

---

## RETENTION Command

Remove old backups.

### Remove Manifests Older Than N Days
```bash
rmrf retention -s ./my-store --days 30
```

Output:
```
Deleted 5 manifests older than 30 days
```

### Dry Run (Preview Only)
```bash
rmrf retention -s ./my-store --days 30 --dry-run
```

Output:
```
Dry run: would delete 5 manifests older than 30 days
```

### Retention + GC
```bash
rmrf retention -s ./my-store --days 30
rmrf gc -s ./my-store
```

---

## CHECK Command

Check for stale VM sentinels and auto-resume suspended VMs.

When a backup is interrupted by SIGKILL (or a crash), the VM may remain suspended
and a sentinel file (`.suspended-<vmid>`) is left in the store. This command
detects and recovers from that state.

### Basic Check
```bash
rmrf check -s ./vm-store
```

**No output** if everything is clean (no sentinels found). Exit code 0.

**With stale sentinel:**
```
Warning: found stale sentinel for VM 103 — attempting auto-resume
  VM 103 resumed successfully, removing sentinel
```

Exit code 0 on success, non-zero if a VM could not be resumed.

### Safety Guard
If a backup is currently running (store lock held by a live process), `check`
skips the scan entirely — sentinels are legitimate during an active backup.

### Scheduling with systemd Timer

Run `rmrf check` every 1-5 minutes to minimise the window between a crash and
automatic recovery.

**Service unit** (`/etc/systemd/system/rmrf-check.service`):
```ini
[Unit]
Description=RMRF sentinel check

[Service]
Type=oneshot
ExecStart=/usr/local/bin/rmrf check -s /path/to/store
```

**Timer unit** (`/etc/systemd/system/rmrf-check.timer`):
```ini
[Unit]
Description=RMRF sentinel check every 5 minutes

[Timer]
OnBootSec=1min
OnUnitActiveSec=5min

[Install]
WantedBy=timers.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable rmrf-check.timer
sudo systemctl start rmrf-check.timer
```

> **Note:** There is a residual risk window between the crash and the next
> `rmrf check` run (1-5 minutes with the timer above). The VM remains suspended
> during this window. This is not instantaneous recovery.

### Manual Recovery

If `rmrf check` fails or you need to recover immediately:

1. **Identify the sentinel:**
   ```bash
   ls -la /path/to/store/.suspended-*
   ```
   The file name contains the VM ID (e.g. `.suspended-103`).

2. **Check VM state:**
   ```bash
   qm status 103
   # or for LXC:
   pct status 201
   ```

3. **Resume the VM manually:**
   ```bash
   qm resume 103
   # or for LXC:
   pct resume 201
   ```

4. **Remove the sentinel:**
   ```bash
   rm /path/to/store/.suspended-103
   ```

5. **Verify:**
   ```bash
   qm status 103
   # Should show "running"
   ```

---

## Scheduling

RMRF does not include a built-in scheduler. Use systemd timers or cron.

### systemd Service + Timer

**Service unit** (`/etc/systemd/system/rmrf-backup.service`):
```ini
[Unit]
Description=RMRF backup

[Service]
Type=oneshot
ExecStart=/usr/local/bin/rmrf backup /path/to/data -s ./store
```

**Timer unit** (`/etc/systemd/system/rmrf-backup.timer`):
```ini
[Unit]
Description=Daily RMRF backup

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable rmrf-backup.timer
sudo systemctl start rmrf-backup.timer
```

### Cron

```bash
# Daily backup at 2 AM
0 2 * * * /usr/local/bin/rmrf backup /path/to/data -s ./store

# Weekly VM backup (Sunday midnight)
0 0 * * 0 /usr/local/bin/rmrf backup /var/lib/vz/images/100/vm-100-disk-0.qcow2 -s ./vm-store --vm 100

# Retention + GC every Sunday at 3 AM
0 3 * * 0 rmrf retention -s ./store --days 30 && rmrf gc -s ./store
```

---

## Hypervisor Support

RMRF can suspend VMs before backup for consistent snapshots.

### Proxmox (QEMU)
```bash
rmrf backup /var/lib/vz/images/100/vm-100-disk-0.qcow2 -s ./store --vm 100
```

Uses `qm suspend` / `qm resume`. The VM ID is extracted from the disk path.

### Proxmox (LXC)
```bash
rmrf backup /var/lib/vz/images/201/vm-201-disk-0.raw -s ./store --vm 201
```

Uses `pct suspend` / `pct resume`.

### Libvirt
```bash
rmrf backup /var/lib/libvirt/images/ubuntu.qcow2 -s ./store --vm ubuntu-vm --hypervisor libvirt
```

Uses `virsh suspend` / `virsh resume`.

> **Note:** libvirt/KVM support is implemented but has **not been validated** on real hardware (no test machine available). Proxmox QEMU and LXC have been validated. Use libvirt with caution and verify your restore workflow.

### Skip VM Suspension
```bash
rmrf backup /path/to/disk.qcow2 -s ./store --vm 100 --no-suspend
```

Useful when the VM is already stopped or you want a crash-consistent backup.

### Auto-Detection
When `--hypervisor` is omitted, RMRF auto-detects by checking available tools:
- `qm` → Proxmox QEMU
- `pct` → Proxmox LXC
- `virsh` → Libvirt

If none are found, backup proceeds without suspension (Noop lifecycle).

---

## Practical Workflows

### Complete Backup Cycle
```bash
# 1. Create backup
echo "Creating backup..."
rmrf backup /path/to/data -s ./store

# 2. List backups
echo "Backups in store:"
rmrf list -s ./store

# 3. Verify integrity
manifest=$(find ./store/manifests -name "*.bincode" | head -1)
echo "Verifying..."
rmrf verify "$manifest" -s ./store

# 4. Test restore
echo "Testing restore..."
rmrf restore "$manifest" -s ./store -o /tmp/test-restore

# 5. Cleanup
echo "Cleaning up orphans..."
rmrf gc -s ./store
```

### Incremental Backup with Dedup
```bash
# First backup
rmrf backup file.txt -s ./store

# Edit file...
echo "New content" >> file.txt

# Backup again - dedup skips identical chunks
rmrf backup file.txt -s ./store

# Check dedup in output - "Dedup skipped" count
rmrf list -s ./store
```

### Automated Backups
```bash
# systemd timer (recommended)
# /etc/systemd/system/rmrf-backup.service
[Unit]
Description=RMRF backup

[Service]
Type=oneshot
ExecStart=/usr/local/bin/rmrf backup /home/user/important -s ./store --retain 30

# /etc/systemd/system/rmrf-backup.timer
[Timer]
OnCalendar=daily
Persistent=true

# Enable
sudo systemctl enable rmrf-backup.timer
sudo systemctl start rmrf-backup.timer

# Or with cron
# crontab -e
0 2 * * * /usr/local/bin/rmrf backup /home/user/important -s ./store --retain 30
```

---

## Performance Tips

- **Large files**: FastCDC automatically creates multiple chunks
- **Compression**: Text/logs compress 4-10x, binaries 10-50%
- **Deduplication**: Identical 1GB files stored as 1GB (not 2GB)
- **GC**: Run periodically after deleting old backups
- **Verification**: Run weekly on important backups

---

## Troubleshooting

### Manifest not found
```bash
# List available manifests
find ./my-store/manifests -name "*.bincode"
```

### Restore permission denied
```bash
# Check output directory
mkdir -p output-dir
chmod 755 output-dir

# Retry restore
rmrf restore "$manifest" -s ./my-store -o output-dir/file.txt
```

### Store locked
```bash
# Wait for other backup to finish, or remove stale lock
rm ./my-store/.rmrf.lock

# Retry
rmrf backup file.txt -s ./my-store
```

---

For more help: `rmrf <command> --help`
