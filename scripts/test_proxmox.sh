#!/usr/bin/env bash
# test_proxmox.sh — Real Proxmox integration test (QEMU + LXC)
#
# Usage:
#   ./test_proxmox.sh <vmid>                # QEMU VM, auto-detect disk
#   ./test_proxmox.sh <vmid> --no-suspend   # Skip suspend (stopped VM or snapshot)
#   ./test_proxmox.sh <vmid> --disk scsi1   # Specific disk
#   ./test_proxmox.sh <ctid> --lxc          # LXC container
#
# Safety: NEVER restores to the original disk. All restores go to /tmp.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

if command -v rmrf &>/dev/null; then
    RMRF="rmrf"
elif [ -x /usr/local/bin/rmrf ]; then
    RMRF="/usr/local/bin/rmrf"
else
    RMRF="cargo run --manifest-path $PROJECT_DIR/Cargo.toml --quiet"
fi

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'
pass=0 fail=0
pass_msg() { echo -e "${GREEN}PASS${NC} $1"; ((++pass)); }
fail_msg() { echo -e "${RED}FAIL${NC} $1"; ((++fail)); }
warn_msg() { echo -e "${YELLOW}WARN${NC} $1"; }

# ── Parse args ─────────────────────────────────────────────────────
VMID="" DISK_OPT="" STORE="/tmp/rmrf-proxmox-test"
LXC=false NO_SUSPEND=false

while [ $# -gt 0 ]; do
    case "$1" in
        --lxc)        LXC=true; shift ;;
        --no-suspend) NO_SUSPEND=true; shift ;;
        --disk)       DISK_OPT="$2"; shift 2 ;;
        --store)      STORE="$2"; shift 2 ;;
        -*)           echo "Unknown option: $1"; exit 1 ;;
        *)            VMID="$1"; shift ;;
    esac
done

if [ -z "$VMID" ]; then
    echo "Usage: $0 <vmid|ctid> [--lxc] [--no-suspend] [--disk <spec>] [--store <path>]"
    exit 1
fi

HYPERVISOR_FLAG=""
CMD_PREFIX="qm"
if [ "$LXC" = true ]; then
    HYPERVISOR_FLAG="--hypervisor proxmox-lxc"
    CMD_PREFIX="pct"
fi

# ── Cleanup ────────────────────────────────────────────────────────
cleanup() {
    echo ""
    echo "--- Cleanup ---"
    if [ "$LXC" = false ] && [ "$VM_WAS_RUNNING" = true ] && [ "$NO_SUSPEND" = false ]; then
        local after
        after=$(qm status "$VMID" 2>/dev/null || echo "unknown")
        if echo "$after" | grep -qiE "suspended|paused"; then
            echo "WARNING: VM $VMID left suspended — resuming"
            qm resume "$VMID" 2>/dev/null || true
        fi
    fi
    rm -rf "$STORE" /tmp/rmrf-restored-vm* 2>/dev/null || true
}
trap cleanup EXIT

# ── Pre-flight ─────────────────────────────────────────────────────
echo "╔══════════════════════════════════════════════╗"
echo "║   RMRF Proxmox Integration Test ($($LXC && echo LXC || echo QEMU))     ║"
echo "╚══════════════════════════════════════════════╝"
echo ""

if [ "$(id -u)" -ne 0 ]; then
    fail_msg "Must be run as root"; exit 1
fi

if ! $CMD_PREFIX status "$VMID" &>/dev/null; then
    fail_msg "$CMD_PREFIX: instance $VMID does not exist"; exit 1
fi

VM_WAS_RUNNING=false
if $CMD_PREFIX status "$VMID" 2>/dev/null | grep -qi "running"; then
    VM_WAS_RUNNING=true
fi

if [ "$VM_WAS_RUNNING" = true ] && [ "$NO_SUSPEND" = false ] && [ "$LXC" = false ]; then
    echo -e "  ${YELLOW}VM $VMID is RUNNING. Backup will suspend it.${NC}"
    echo -n "  Continue? [Y/n] "
    read -r confirm
    [[ "$confirm" =~ ^[Nn] ]] && exit 0
fi

SUSPEND_FLAG=""
[ "$NO_SUSPEND" = true ] && SUSPEND_FLAG="--no-suspend"

echo ""
echo "Testing $VMID | Store: $STORE"
echo ""

# ── Test 1: Backup ─────────────────────────────────────────────────
echo "--- Test 1: Backup ---"
BACKUP_ARGS=(--vm "$VMID" -s "$STORE")
[ -n "$SUSPEND_FLAG" ] && BACKUP_ARGS+=("$SUSPEND_FLAG")
[ -n "$DISK_OPT" ] && BACKUP_ARGS+=("--disk" "$DISK_OPT")
BACKUP_ARGS+=($HYPERVISOR_FLAG)

set +e
BACKUP_STDERR=$($RMRF backup "${BACKUP_ARGS[@]}" 2>&1 >/dev/null)
BACKUP_RC=$?
set -e
if [ $BACKUP_RC -eq 0 ]; then
    pass_msg "Backup succeeded"
else
    fail_msg "Backup failed (exit $BACKUP_RC)"
    echo "$BACKUP_STDERR" | sed 's/^/  │ /'
fi

MANIFEST=$(ls -t "$STORE/manifests/"*.bincode 2>/dev/null | head -1) || true
[ -z "$MANIFEST" ] && { fail_msg "No manifest found"; echo ""; echo "Results: $pass pass, $fail fail"; exit $fail; }

# ── Test 2: VM resume ──────────────────────────────────────────────
echo ""
echo "--- Test 2: VM resume ---"
if [ "$VM_WAS_RUNNING" = true ] && [ "$NO_SUSPEND" = false ] && [ "$LXC" = false ]; then
    if qm status "$VMID" 2>/dev/null | grep -qi "running"; then
        pass_msg "VM running after backup (resume OK)"
    else
        fail_msg "VM NOT running after backup"
    fi
else
    pass_msg "(skipped)"
fi

# ── Test 3: list ───────────────────────────────────────────────────
echo ""
echo "--- Test 3: list ---"
$RMRF list -s "$STORE" > /dev/null 2>&1 && pass_msg "list OK" || fail_msg "list failed"

# ── Test 4: info ───────────────────────────────────────────────────
echo ""
echo "--- Test 4: info ---"
INFO_OUT=$($RMRF info "$MANIFEST" -s "$STORE" 2>&1)
echo "$INFO_OUT" | grep -qi "VmDisk" && pass_msg "info VmDisk metadata" || fail_msg "info missing VmDisk"

# ── Test 5: verify ─────────────────────────────────────────────────
echo ""
echo "--- Test 5: verify ---"
$RMRF verify "$MANIFEST" -s "$STORE" 2>&1 | grep -qiE "present|ok|0 missing|all chunks" \
    && pass_msg "verify OK" || fail_msg "verify failed"

# ── Test 6: Restore ────────────────────────────────────────────────
echo ""
echo "--- Test 6: Restore to /tmp ---"
RESTORED="/tmp/rmrf-restored-vm-${VMID}"
$RMRF restore "$MANIFEST" -s "$STORE" -o "$RESTORED" > /dev/null 2>&1 \
    && pass_msg "Restore OK" || fail_msg "Restore failed"

# ── Test 7: Bit-identical ──────────────────────────────────────────
echo ""
echo "--- Test 7: Bit-identical ---"
if [ ! -f "$RESTORED" ]; then
    fail_msg "Restored file missing"
elif [ "$VM_WAS_RUNNING" = true ] && [ "$NO_SUSPEND" = false ]; then
    warn_msg "VM running: cmp skipped (live disk changed)"
else
    SRC_PATH=$(echo "$INFO_OUT" | grep -i "^Source path:" | sed 's/^[^:]*: *//')
    if [ -n "$SRC_PATH" ] && cmp "$SRC_PATH" "$RESTORED" > /dev/null 2>&1; then
        pass_msg "Bit-identical"
    else
        fail_msg "cmp mismatch (src=$SRC_PATH)"
    fi
fi

# ── Test 8: Compression ────────────────────────────────────────────
echo ""
echo "--- Test 8: Compression ---"
STORE_SIZE=$(du -sb "$STORE/chunks" 2>/dev/null | cut -f1)
ORIG_SIZE=$(echo "$INFO_OUT" | grep -i "Source size:" | grep -oE '[0-9]+' | head -1)
if [ -n "$ORIG_SIZE" ] && [ "$STORE_SIZE" -lt "$ORIG_SIZE" ]; then
    if command -v bc &>/dev/null; then
        RATIO=$(echo "scale=1; 100 * $STORE_SIZE / $ORIG_SIZE" | bc)
    else
        RATIO=$((100 * STORE_SIZE / ORIG_SIZE))
    fi
    pass_msg "Compressed: ${RATIO}% of original"
else
    warn_msg "Ratio unavailable (orig=$ORIG_SIZE, store=$STORE_SIZE)"
fi

# ── Test 9: Error — non-existent VM ────────────────────────────────
echo ""
echo "--- Test 9: Error — non-existent VM ---"
ERR_HV="--hypervisor proxmox"
[ "$LXC" = true ] && ERR_HV="--hypervisor proxmox-lxc"
$RMRF backup --vm 99999 $ERR_HV -s "$STORE" 2>&1 | grep -qiE "error|not found|failed" \
    && pass_msg "Non-existent VM: error handled" || fail_msg "Non-existent VM: no error"

# ── Test 10: Error — non-existent disk ─────────────────────────────
echo ""
echo "--- Test 10: Error — non-existent disk ---"
$RMRF backup /dev/pve/nonexistent-disk -s "$STORE" --vm "$VMID" --no-suspend 2>&1 | grep -qiE "error|not found|no such" \
    && pass_msg "Bad disk path: error handled" || fail_msg "Bad disk path: no error"

# ── Test 11: Safety — refuse restore to original ───────────────────
echo ""
echo "--- Test 11: Safety — restore to original refused ---"
SRC_PATH=$(echo "$INFO_OUT" | grep -i "^Source path:" | sed 's/^[^:]*: *//')
if [ -n "$SRC_PATH" ]; then
    $RMRF restore "$MANIFEST" -s "$STORE" -o "$SRC_PATH" 2>&1 | grep -qiE "refuse|overwrite|exists|force" \
        && pass_msg "Restore to original: refused (safe)" || fail_msg "Restore to original: NOT refused"
else
    warn_msg "Source path unknown, skipping"
fi

# ── Test 12: Error — bad store ─────────────────────────────────────
echo ""
echo "--- Test 12: Error — bad store ---"
$RMRF restore "$MANIFEST" -s /tmp/rmrf-nonexistent-store -o /tmp/rmrf-dummy 2>&1 | grep -qiE "error|not found|no such" \
    && pass_msg "Bad store: error handled" || fail_msg "Bad store: no error"

# ── Test 13: Store structure ───────────────────────────────────────
echo ""
echo "--- Test 13: Store structure ---"
if [ -d "$STORE/manifests" ] && [ -d "$STORE/chunks" ] && [ -f "$STORE/index.bincode" ]; then
    pass_msg "manifests/ + chunks/ + index.bincode"
else
    fail_msg "Missing store directories"
fi

# ── Summary ────────────────────────────────────────────────────────
echo ""
echo "=========================="
echo "Results: $pass pass, $fail fail"
[ "$fail" -eq 0 ] && echo -e "${GREEN}All Proxmox tests passed!${NC}" || echo -e "${RED}$fail test(s) failed${NC}"
exit $fail
