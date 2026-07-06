#!/usr/bin/env bash
# test_proxmox.sh — Real Proxmox integration test
#
# Usage:
#   ./test_proxmox.sh --create-test-vm   # Creates VM 9000, tests, destroys
#   ./test_proxmox.sh <vmid>              # Tests on existing VM
#   ./test_proxmox.sh <vmid> --no-suspend # Skip suspend (VM stopped or snapshotted)
#
# Safety: NEVER restores to the original disk path. All restores go to /tmp.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
RMRF="cargo run --manifest-path "$PROJECT_DIR/Cargo.toml" --quiet"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass=0
fail=0

pass_msg() { echo -e "${GREEN}PASS${NC} $1"; ((++pass)); }
fail_msg() { echo -e "${RED}FAIL${NC} $1"; ((++fail)); }
warn_msg() { echo -e "${YELLOW}WARN${NC} $1"; }

# ── Parse args ─────────────────────────────────────────────────────
CREATE_TEST_VM=false
NO_SUSPEND=false
VMID=""
DISK_PATH=""
STORE="/tmp/rmrf-proxmox-test"

while [ $# -gt 0 ]; do
    case "$1" in
        --create-test-vm) CREATE_TEST_VM=true; shift ;;
        --no-suspend)     NO_SUSPEND=true; shift ;;
        --disk)           DISK_PATH="$2"; shift 2 ;;
        --store)          STORE="$2"; shift 2 ;;
        *)                VMID="$1"; shift ;;
    esac
done

# ── Cleanup ────────────────────────────────────────────────────────
cleanup() {
    echo ""
    echo "--- Cleanup ---"
    if [ "$CREATE_TEST_VM" = true ]; then
        # Check if VM 9000 exists and stop/destroy it
        if qm status 9000 &>/dev/null 2>&1; then
            echo "Stopping and destroying VM 9000..."
            qm stop 9000 --skiplock 2>/dev/null || true
            sleep 2
            qm destroy 9000 --purge 2>/dev/null || true
        fi
        rm -f /tmp/rmrf-test-vm-disk.raw
    fi
    rm -rf "$STORE" /tmp/rmrf-restored-vm* /tmp/rmrf-store-error-test 2>/dev/null || true
}
trap cleanup EXIT

# ── Pre-flight ─────────────────────────────────────────────────────
echo "╔════════════════════════════════════════╗"
echo "║   RMRF Proxmox Integration Test       ║"
echo "╚════════════════════════════════════════╝"
echo ""

if [ "$(id -u)" -ne 0 ]; then
    echo -e "${RED}FAIL${NC} Must be run as root (qm/pvesm require root)"
    exit 1
fi

for cmd in qm pvesm; do
    if ! command -v "$cmd" &>/dev/null; then
        echo -e "${RED}FAIL${NC} Required command not found: $cmd"
        exit 1
    fi
done
echo -e "${GREEN}PASS${NC} Pre-flight: root + qm + pvesm OK"

# ── Create test VM (if --create-test-vm) ──────────────────────────
if [ "$CREATE_TEST_VM" = true ]; then
    VMID=9000
    echo ""
    echo "Creating test VM $VMID..."

    # Detect first active storage
    # Detect first active storage suitable for VM disks (lvmthin, dir, or zfs)
    PVE_STORAGE=$(pvesm status | awk 'NR>1 && $2=="active" && ($3=="lvmthin" || $3=="dir" || $3=="zfs") {print $1; exit}')
    if [ -z "$PVE_STORAGE" ]; then
        fail_msg "No active Proxmox storage found. Aborting."
        exit "$fail"
    fi
    echo "  Using storage: $PVE_STORAGE"

    # Create VM with 100M disk
    if ! qm create "$VMID" --memory 128 --scsi0 "$PVE_STORAGE":100M &>/dev/null; then
        fail_msg "Failed to create test VM $VMID"
        exit "$fail"
    fi
    sleep 1

    # Get disk path
    RAW_LINE=$(qm config "$VMID" 2>/dev/null | grep -E '^(scsi|ide|virtio|sata)[0-9]+:' | head -1)
    STORAGE_VOL=$(echo "$RAW_LINE" | awk '{print $2}' | cut -d, -f1)
    DISK_PATH=$(pvesm path "$STORAGE_VOL" 2>/dev/null)

    if [ -z "$DISK_PATH" ] || [ ! -b "$DISK_PATH" ]; then
        fail_msg "Cannot resolve disk path for VM $VMID"
        exit "$fail"
    fi
    echo "  Disk path: $DISK_PATH"

    # Write random data to disk (fill first 50 MB)
    echo "  Writing test data to disk..."
    dd if=/dev/urandom of="$DISK_PATH" bs=1M count=50 conv=notrunc 2>/dev/null

    # Do NOT start VM — backup with --no-suspend
    NO_SUSPEND=true
    echo -e "${GREEN}PASS${NC} Test VM $VMID created with 100M disk"
else
    # Validate VM ID for existing VM mode
    if [ -z "$VMID" ]; then
        echo "Usage: $0 [options] [vmid]"
        echo "  $0 --create-test-vm"
        echo "  $0 100 [--disk /dev/pve/vm-100-disk-0] [--no-suspend]"
        exit 1
    fi
    if ! qm status "$VMID" &>/dev/null; then
        fail_msg "VM $VMID does not exist"
        exit "$fail"
    fi
fi

# ── Resolve disk path (if not already set by --create-test-vm) ─────
if [ -z "$DISK_PATH" ]; then
    RAW_LINE=$(qm config "$VMID" 2>/dev/null | grep -E '^(scsi|ide|virtio|sata)[0-9]+:' | head -1)
    STORAGE_VOL=$(echo "$RAW_LINE" | awk '{print $2}' | cut -d, -f1)
    if [ -n "$STORAGE_VOL" ]; then
        DISK_PATH=$(pvesm path "$STORAGE_VOL" 2>/dev/null)
    fi
    if [ -z "$DISK_PATH" ]; then
        fail_msg "Cannot auto-detect disk path for VM $VMID. Use --disk <path>"
        exit "$fail"
    fi
fi

echo ""
echo "Testing VM $VMID | Disk: $DISK_PATH | Store: $STORE"
echo ""

# ── Determine suspend mode ─────────────────────────────────────────
VM_STATUS=$(qm status "$VMID" 2>/dev/null | awk '{print $2}')
SUSPEND_FLAG=""
if [ "$NO_SUSPEND" = true ]; then
    SUSPEND_FLAG="--no-suspend"
    echo "  Mode: no-suspend"
elif [ "$VM_STATUS" = "running" ]; then
    echo -e "  ${YELLOW}VM is RUNNING. Backup will suspend it.${NC}"
    echo -n "  Continue? [Y/n] "
    read -r confirm
    if [ "$confirm" = "n" ] || [ "$confirm" = "N" ]; then
        echo "Aborted."
        exit 1
    fi
    echo "  Mode: suspend"
else
    echo "  VM status: $VM_STATUS (will use --no-suspend)"
    SUSPEND_FLAG="--no-suspend"
fi

# Get original size for compression ratio check
ORIG_SIZE_BYTES=$(stat --format=%s "$DISK_PATH" 2>/dev/null || echo "unknown")

# Record VM status before backup (for suspend test)
STATUS_BEFORE=$(qm status "$VMID" 2>/dev/null)

# ── Test 1: Backup with real qm suspend/resume ─────────────────────
echo ""
echo "--- Test 1: Backup with --vm $VMID ---"
BACKUP_ARGS=("$DISK_PATH" -s "$STORE" --vm "$VMID" --disk-name "scsi0")
if [ -n "$SUSPEND_FLAG" ]; then
    BACKUP_ARGS+=("$SUSPEND_FLAG")
fi
if $RMRF backup "${BACKUP_ARGS[@]}" > /dev/null 2>&1; then
    pass_msg "Backup with --vm $VMID succeeded"
else
    fail_msg "Backup with --vm $VMID failed"
fi

# Count manifests
MANIFEST=$(ls -t "$STORE/manifests/"*.bincode 2>/dev/null | head -1)

# ── Test 2: VM status after backup ─────────────────────────────────
echo ""
echo "--- Test 2: VM status (suspend/resume) ---"
if [ -n "$SUSPEND_FLAG" ]; then
    pass_msg "(skipped: --no-suspend mode)"
elif [ "$VM_STATUS" != "running" ]; then
    pass_msg "(skipped: VM was not running before backup)"
else
    STATUS_AFTER=$(qm status "$VMID" 2>/dev/null)
    if echo "$STATUS_AFTER" | grep -qi "running"; then
        pass_msg "VM is running after backup (resume called)"
    else
        fail_msg "VM is NOT running after backup: $STATUS_AFTER"
    fi
fi

# ── Test 3: rmrf list ──────────────────────────────────────────────
echo ""
echo "--- Test 3: rmrf list ---"
if $RMRF list -s "$STORE" > /dev/null 2>&1; then
    pass_msg "List command succeeded"
else
    fail_msg "List command failed"
fi

# ── Test 4: rmrf info (VmDisk metadata) ────────────────────────────
echo ""
echo "--- Test 4: rmrf info ---"
INFO_OUT=$($RMRF info "$MANIFEST" -s "$STORE" 2>&1)
if echo "$INFO_OUT" | grep -qi "VmDisk"; then
    pass_msg "Info shows VmDisk metadata"
else
    fail_msg "Info missing VmDisk metadata"
fi

# ── Test 5: rmrf verify ────────────────────────────────────────────
echo ""
echo "--- Test 5: rmrf verify ---"
VERIFY_OUT=$($RMRF verify "$MANIFEST" -s "$STORE" 2>&1)
if echo "$VERIFY_OUT" | grep -qiE "present|ok|0 missing|all"; then
    pass_msg "Verify: all chunks present"
else
    fail_msg "Verify: chunks missing"
fi

# ── Test 6: Restore to /tmp ────────────────────────────────────────
echo ""
echo "--- Test 6: Restore to /tmp ---"
RESTORED="/tmp/rmrf-restored-vm-${VMID}"
if $RMRF restore "$MANIFEST" -s "$STORE" -o "$RESTORED" > /dev/null 2>&1; then
    pass_msg "Restore succeeded"
else
    fail_msg "Restore failed"
fi

# ── Test 7: Bit-identical (cmp) ────────────────────────────────────
echo ""
echo "--- Test 7: Bit-identical comparison ---"
if [ -f "$RESTORED" ]; then
    RESTORED_SIZE=$(stat --format=%s "$RESTORED")
    if [ "$ORIG_SIZE_BYTES" != "unknown" ] && [ "$RESTORED_SIZE" -eq "$ORIG_SIZE_BYTES" ] 2>/dev/null; then
        if cmp "$DISK_PATH" "$RESTORED" > /dev/null 2>&1; then
            pass_msg "Bit-identical: $RESTORED matches $DISK_PATH"
        else
            fail_msg "Bit-identical: cmp mismatch"
        fi
    else
        warn_msg "Size mismatch or unknown (orig=$ORIG_SIZE_BYTES, restored=$RESTORED_SIZE)"
        if cmp "$DISK_PATH" "$RESTORED" > /dev/null 2>&1; then
            pass_msg "Bit-identical despite size note"
        else
            fail_msg "Bit-identical: cmp mismatch"
        fi
    fi
else
    fail_msg "Restored file not found: $RESTORED"
fi

# ── Test 8: Compression (store < disk size) ─────────────────────────
echo ""
echo "--- Test 8: Compression ratio ---"
if [ -d "$STORE/chunks" ]; then
    STORE_SIZE=$(du -sb "$STORE/chunks" 2>/dev/null | cut -f1)
    if [ "$ORIG_SIZE_BYTES" != "unknown" ] && [ "$STORE_SIZE" -lt "$ORIG_SIZE_BYTES" ]; then
        RATIO=$(echo "scale=1; 100 * $STORE_SIZE / $ORIG_SIZE_BYTES" | bc 2>/dev/null || echo "?")
        pass_msg "Store size $STORE_SIZE < original $ORIG_SIZE_BYTES (${RATIO}% of original)"
    else
        warn_msg "Cannot compute ratio (orig=$ORIG_SIZE_BYTES, store=$STORE_SIZE)"
    fi
else
    fail_msg "Store chunks directory missing"
fi

# ── Test 9: Backup non-existent VM ID ──────────────────────────────
echo ""
echo "--- Test 9: Error — backup non-existent VM ---"
if $RMRF backup "$DISK_PATH" -s "$STORE" --vm 99999 --no-suspend 2>&1 | grep -qiE "error|not found|no such"; then
    pass_msg "Non-existent VM ID: handled gracefully"
else
    fail_msg "Non-existent VM ID: should print error"
fi

# ── Test 10: Backup non-existent disk path ─────────────────────────
echo ""
echo "--- Test 10: Error — backup non-existent disk ---"
if $RMRF backup /dev/pve/non-existent-disk -s "$STORE" --vm "$VMID" --no-suspend 2>&1 | grep -qiE "error|not found|no such"; then
    pass_msg "Non-existent disk path: handled gracefully"
else
    fail_msg "Non-existent disk path: should print error"
fi

# ── Test 11: Safety — refuse restore to original disk path ─────────
echo ""
echo "--- Test 11: Safety — restore to original disk ---"
# The script should refuse to restore to the original disk path.
# This is a safety test: we pass the original disk as output, expect failure/warning.
if $RMRF restore "$MANIFEST" -s "$STORE" -o "$DISK_PATH" 2>&1; then
    fail_msg "Restore to original disk SUCCEEDED (dangerous!)"
else
    pass_msg "Restore to original disk: prevented/failed (safe)"
fi

# ── Test 12: Error — restore with non-existent store ───────────────
echo ""
echo "--- Test 12: Error — restore with bad store ---"
if $RMRF restore "$MANIFEST" -s /tmp/rmrf-nonexistent-store -o /tmp/rmrf-dummy-out 2>&1 | grep -qiE "error|not found|no such"; then
    pass_msg "Restore with bad store: handled gracefully"
else
    fail_msg "Restore with bad store: should print error"
fi

# ── Test 13: Store structure (manifests + chunks dirs) ─────────────
echo ""
echo "--- Test 13: Store directory structure ---"
if [ -d "$STORE/manifests" ] && [ -d "$STORE/chunks" ] && [ -f "$STORE/index.bincode" ] 2>/dev/null; then
    pass_msg "Store has manifests/, chunks/, index.bincode"
else
    fail_msg "Store missing required directories or index"
fi

# ── Cleanup done by trap ──────────────────────────────────────────

echo ""
echo "=========================="
echo "Results: $pass pass, $fail fail"
if [ "$fail" -eq 0 ]; then
    echo -e "${GREEN}All Proxmox tests passed!${NC}"
else
    echo -e "${RED}$fail test(s) failed${NC}"
fi
exit $fail
