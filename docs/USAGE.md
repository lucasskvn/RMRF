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
| `cron` | Generate crontab entries for scheduling |
| `daemon` | Run background scheduler |

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

## CRON Command

Generate crontab entries for scheduling.

### Generate Crontab
```bash
rmrf cron > /tmp/rmrf-cron.txt
cat /tmp/rmrf-cron.txt
```

Output (from `~/.rmrf.toml` jobs):
```
# RMRF scheduled jobs
# RMRF: daily-docs
0 1 * * * /usr/local/bin/rmrf daemon --once --job daily-docs

# RMRF: weekly-vm
0 0 * * 0 /usr/local/bin/rmrf daemon --once --job weekly-vm
```

### Install to Crontab
```bash
rmrf cron | crontab -
```

---

## DAEMON Command

Run scheduler in background.

### Run Once (Test)
```bash
rmrf daemon --once
```

Executes all jobs from `~/.rmrf.toml` once.

### Run Specific Job
```bash
rmrf daemon --once --job daily-docs
```

Executes only the "daily-docs" job.

### Run as Service
```bash
# For systemd, create /etc/systemd/system/rmrf-daemon.service
[Unit]
Description=RMRF Backup Daemon
After=network.target

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/rmrf daemon
Restart=always

[Install]
WantedBy=multi-user.target
```

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
# ~/.rmrf.toml
[[jobs]]
name = "nightly"
path = "/home/user/important"
schedule = "0 2 * * *"
retain = 30

# Install to cron
rmrf cron | crontab -

# Verify
crontab -l | grep rmrf
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
