# Usage Guide

Complete reference for all RMRF commands with practical examples.

## Quick Reference

| Command | Purpose | Example |
|---------|---------|---------|
| `backup` | Create backup | `rmrf backup file.txt -s ./store` |
| `restore` | Restore from backup | `rmrf restore manifest.bincode -s ./store -o file.txt` |
| `list` | List all backups | `rmrf list -s ./store` |
| `info` | Show backup details | `rmrf info manifest.bincode -s ./store` |
| `verify` | Check integrity | `rmrf verify manifest.bincode -s ./store` |
| `gc` | Clean orphan chunks | `rmrf gc -s ./store` |

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

For large files, FastCDC will create multiple chunks:
```
Chunks: 847
Total bytes: 314572800
Dedup skipped: 23  (chunks already in store)
```

### Backup Directory

```bash
rmrf backup ./documents -s ./backups
```

Recursively backs up all files in directory:
```
Backup directory: ./documents
Found 42 files
Files: 42
Chunks: 156
Total bytes: 5242880
Dedup skipped (already stored): 12
Directory backup complete!
```

### Backup with Retention

Delete old backups and clean up orphan chunks:

```bash
# Keep only backups from last 30 days
rmrf backup file.txt -s ./store --retain 30
```

This will:
1. Create new backup
2. Remove manifests older than 30 days
3. Run garbage collection to clean orphaned chunks

### Backup VM Disk (Proxmox)

```bash
# Backup QEMU VM
rmrf backup /var/lib/vz/images/100/vm-100-disk-0.qcow2 \
  -s ./vm-backups \
  --vm 100

# Backup LXC Container
rmrf backup /var/lib/vz/images/201/vm-201-disk-0.raw \
  -s ./vm-backups \
  --vm 201
```

The system will:
1. Suspend the VM (pause execution)
2. Read disk snapshot
3. Resume the VM immediately
4. Backup disk chunks to store

### Backup VM Disk (Libvirt)

```bash
rmrf backup /var/lib/libvirt/images/ubuntu.qcow2 \
  -s ./vm-backups \
  --vm ubuntu-vm \
  --hypervisor libvirt
```

### Specify Disk Format

By default, format is auto-detected from extension:

```bash
# Auto-detect (qcow2 from .qcow2, else raw)
rmrf backup vm.qcow2 -s ./store

# Explicit format
rmrf backup vm-image --format raw -s ./store
rmrf backup vm-image --format qcow2 -s ./store
```

### No VM Suspend (snapshot already taken)

```bash
# Skip suspend/resume if disk is already snapshotted
rmrf backup snapshot.qcow2 \
  -s ./store \
  --vm myvm \
  --no-suspend
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

### Restore to Specific Location

```bash
manifest=$(find ./my-store/manifests -name "*myfile*" | head -1)

# Restore to custom path
rmrf restore "$manifest" -s ./my-store -o /tmp/my-backup.txt
```

### Restore with Force Overwrite

```bash
# File already exists - use --force to overwrite
rmrf restore "$manifest" -s ./my-store -o existing-file.txt --force
```

### Restore Directory Backup

```bash
# Find manifest from directory backup
manifest=$(find ./my-store/manifests -name "*documents*" | head -1)

# Restore - creates files in current directory
rmrf restore "$manifest" -s ./my-store -o ./restored-documents/
```

### Verify After Restore

```bash
# Restore file
manifest=$(find ./my-store/manifests -name "*test*" | head -1)
rmrf restore "$manifest" -s ./my-store -o restored.txt

# Verify same as original
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
20260707T081019Z       11 B    1 chunks  test.txt
20260707T084548Z  100.0 KiB    5 chunks  test_binary.bin
20260707T084548Z        0 B    0 chunks  test_empty.txt
20260707T084548Z   13.5 MiB  217 chunks  test_large_zeros.txt
```

**Columns:**
- Timestamp (ISO8601 format)
- Compressed size
- Number of chunks
- Original filename

### Search Backups

```bash
# Find specific backup
rmrf list -s ./my-store | grep "test.txt"

# Count total backups
rmrf list -s ./my-store | wc -l
```

### Store Statistics

```bash
# Get manifest paths for scripting
manifests=$(find ./my-store/manifests -name "*.bincode")
echo "Total backups: $(echo "$manifests" | wc -l)"
echo "Store size: $(du -sh ./my-store)"
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

**Fields:**
- **Version**: Manifest format version
- **Source path**: Original file path at backup time
- **Source size**: Uncompressed size
- **Source type**: File or VM disk
- **Modified**: Unix timestamp of backup
- **Permissions**: Unix file permissions (octal)
- **Chunks**: Number of chunks in backup

### Extract Chunk Info

```bash
manifest=$(find ./my-store/manifests -name "*.bincode" | head -1)

# Get source path
source=$(rmrf info "$manifest" -s ./my-store | grep "Source path" | awk '{print $NF}')
echo "Backed up: $source"

# Get chunk count
chunks=$(rmrf info "$manifest" -s ./my-store | grep "Chunks" | awk '{print $NF}')
echo "Chunks: $chunks"
```

---

## VERIFY Command

Check backup integrity.

### Verify Backup

```bash
manifest=$(find ./my-store/manifests -name "*.bincode" | head -1)
rmrf verify "$manifest" -s ./my-store
```

Output (success):
```
OK: all 217 chunks present and verified
```

Output (failure):
```
FAIL: 5/217 chunks missing
Missing chunk: 3df651e1a...
Missing chunk: ab12cdef5...
```

### Verify All Backups

```bash
# Check each backup
for manifest in ./my-store/manifests/*.bincode; do
    echo "Verifying $manifest..."
    rmrf verify "$manifest" -s ./my-store
done
```

### Scheduled Verification

```bash
# Add to crontab (daily at 2 AM)
# 0 2 * * * cd /home/user/backups && ./verify-all.sh

cat > verify-all.sh << 'SCRIPT'
#!/bin/bash
STORE="./my-store"
LOG="$STORE/verify.log"

echo "Verification: $(date)" >> "$LOG"
for manifest in "$STORE"/manifests/*.bincode; do
    rmrf verify "$manifest" -s "$STORE" >> "$LOG" 2>&1
done
echo "Done" >> "$LOG"
SCRIPT

chmod +x verify-all.sh
```

---

## GC Command (Garbage Collection)

Clean up orphan chunks not referenced by any backup.

### Dry Run (preview what would be deleted)

```bash
rmrf gc -s ./my-store --dry-run
```

Output:
```
Dry run: would delete 12 orphan chunks, would free 5.2 MiB
```

### Actually Delete Orphans

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

# Run GC
rmrf gc -s ./my-store

echo "After GC:"
du -sh ./my-store
```

---

## Practical Workflows

### Full Backup Cycle

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

### Incremental Backup with Deduplication

```bash
# First backup
rmrf backup file.txt -s ./store

# Edit file...
echo "New content" >> file.txt

# Backup again - dedup will skip identical chunks
rmrf backup file.txt -s ./store

# Check dedup in output
# Look for "Dedup skipped" count

# List shows multiple backups
rmrf list -s ./store
```

### VM Snapshot Cycle (Proxmox)

```bash
# Create VM backup with auto-suspend
rmrf backup /var/lib/vz/images/100/vm-100-disk-0.qcow2 \
  -s ./vm-store \
  --vm 100

# Verify backup created
rmrf list -s ./vm-store

# Check backup integrity
manifest=$(find ./vm-store/manifests -name "*.bincode" | head -1)
rmrf verify "$manifest" -s ./vm-store

# Restore if needed
rmrf restore "$manifest" -s ./vm-store -o recovered-disk.qcow2
```

### Daily Backup Script

```bash
#!/bin/bash
BACKUP_DIR="/home/user/documents"
STORE="$HOME/backups/store"
LOG="$HOME/backups/backup.log"

{
    echo "=== Backup started: $(date) ==="
    
    # Run backup
    if rmrf backup "$BACKUP_DIR" -s "$STORE" --retain 30; then
        echo "✅ Backup successful"
    else
        echo "❌ Backup failed"
        exit 1
    fi
    
    # Verify
    manifest=$(find "$STORE/manifests" -name "*.bincode" -mmin -5 | head -1)
    rmrf verify "$manifest" -s "$STORE"
    
    # Store size
    echo "Store size: $(du -sh "$STORE")"
    echo "=== Done: $(date) ==="
    
} >> "$LOG" 2>&1
```

Add to crontab:
```bash
# Daily backup at 1 AM
0 1 * * * bash /home/user/backups/daily-backup.sh
```

### Restore to Alternate Location

```bash
# Backup
rmrf backup important-file -s ./store

# Restore multiple copies to different locations
manifest=$(find ./store/manifests -name "*important*" | head -1)
rmrf restore "$manifest" -s ./store -o /backup/version1
rmrf restore "$manifest" -s ./store -o /archive/version2
rmrf restore "$manifest" -s ./store -o /tmp/test
```

---

## Troubleshooting

### Cannot find manifest
```bash
# List all manifests
find ./my-store/manifests -name "*.bincode"

# If empty, no backups exist
ls ./my-store/manifests
```

### Restore permission denied
```bash
# Check store permissions
ls -ld ./my-store

# Check output directory
mkdir -p output-dir
chmod 755 output-dir

# Retry restore
rmrf restore "$manifest" -s ./my-store -o output-dir/file.txt
```

### Backup fails: "store is locked"
```bash
# Another backup in progress
# Wait for it to complete, or:

# Check lock file
ls -la ./my-store/.rmrf.lock

# If stale (process not running), remove it
rm ./my-store/.rmrf.lock
```

### GC reports missing chunks
```bash
# Corruption detected - do NOT run GC
# Investigate backup integrity first

manifest=$(find ./my-store/manifests -name "*.bincode" | head -1)
rmrf verify "$manifest" -s ./my-store

# Check if all manifests still exist
ls -la ./my-store/manifests
```

---

## Performance Tips

- **Large files**: FastCDC will create multiple chunks automatically
- **Compression**: Text/logs compress 4-10x, binaries 10-50%
- **Deduplication**: Identical 1GB files stored as 1GB (not 2GB)
- **GC**: Run periodically to clean orphans after deleting backups
- **Verification**: Run weekly on important backups

---

## Examples Summary

```bash
# Simple file backup & restore
echo "test" > file.txt
rmrf backup file.txt -s store
manifest=$(find store/manifests -name "*.bincode" | head -1)
rmrf restore "$manifest" -s store -o restored.txt
cat restored.txt

# Directory with multiple files
rmrf backup ./my-docs -s store
rmrf list -s store

# VM backup (Proxmox)
rmrf backup /var/lib/vz/images/100/disk.qcow2 \
  -s vm-store --vm 100

# Verify + cleanup
rmrf verify "$manifest" -s store
rmrf gc -s store --dry-run
rmrf gc -s store
```

---

**Need help?** Run `rmrf <command> --help` for command-specific options.
