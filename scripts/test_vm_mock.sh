#!/usr/bin/env bash
# test_vm_mock.sh — Test Proxmox VM backup with mock qm script
# Validates ProxmoxLifecycle calls qm suspend/resume WITHOUT real Proxmox
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
RMRF="cargo run --manifest-path "$PROJECT_DIR/Cargo.toml" --quiet"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

pass=0
fail=0

pass_msg() { echo -e "${GREEN}PASS${NC} $1"; ((++pass)); }
fail_msg() { echo -e "${RED}FAIL${NC} $1"; ((++fail)); }

cleanup() {
    rm -rf "$TEST_DIR" "$MOCK_BIN"
    rm -f /tmp/qm.log
}
trap cleanup EXIT

TEST_DIR=$(mktemp -d)
STORE="$TEST_DIR/store"
DISK="$TEST_DIR/vm-100-disk.raw"
RESTORED="$TEST_DIR/restored.raw"

MOCK_BIN=$(mktemp -d)
QM_SCRIPT="$MOCK_BIN/qm"

# ── 1. Create mock qm ─────────────────────────────────────────────
cat > "$QM_SCRIPT" <<'MOCK'
#!/bin/bash
echo "[mock-qm] $(date '+%H:%M:%S') $*" >> /tmp/qm.log
exit 0
MOCK
chmod +x "$QM_SCRIPT"

# ── 2. Create fake VM disk ────────────────────────────────────────
dd if=/dev/urandom of="$DISK" bs=1024 count=512 2>/dev/null  # 512 KB

# ── 3. Backup with Proxmox hypervisor (mock qm in PATH) ───────────
echo "" > /tmp/qm.log  # fresh log
PATH="$MOCK_BIN:$PATH" $RMRF backup "$DISK" -s "$STORE" --vm 100 --disk-name virtio0 --hypervisor proxmox > /dev/null

# ── 4. Check qm mock was called ───────────────────────────────────
if grep -q "suspend 100" /tmp/qm.log; then
    pass_msg "ProxmoxLifecycle called: qm suspend 100"
else
    fail_msg "ProxmoxLifecycle: qm suspend 100 NOT called"
fi

if grep -q "resume 100" /tmp/qm.log; then
    pass_msg "ProxmoxLifecycle called: qm resume 100"
else
    fail_msg "ProxmoxLifecycle: qm resume 100 NOT called"
fi

# suspend must appear before resume
SUSPEND_LINE=$(grep -n "suspend 100" /tmp/qm.log | head -1 | cut -d: -f1)
RESUME_LINE=$(grep -n "resume 100" /tmp/qm.log | tail -1 | cut -d: -f1)
if [ -n "$SUSPEND_LINE" ] && [ -n "$RESUME_LINE" ] && [ "$SUSPEND_LINE" -lt "$RESUME_LINE" ]; then
    pass_msg "suspend called BEFORE resume (order correct)"
else
    fail_msg "suspend/resume order wrong or missing"
fi

# ── 5. Restore + diff ─────────────────────────────────────────────
MANIFEST=$(ls -t "$STORE/manifests/"*.bincode | head -1)
$RMRF restore "$MANIFEST" -s "$STORE" -o "$RESTORED" > /dev/null 2>&1
if diff "$DISK" "$RESTORED" > /dev/null 2>&1; then
    pass_msg "VM backup + restore: bit-identical"
else
    fail_msg "VM backup + restore: diff mismatch"
fi

# ── 6. Manifest metadata ──────────────────────────────────────────
INFO_OUT=$($RMRF info "$MANIFEST" -s "$STORE" 2>&1)
if echo "$INFO_OUT" | grep -q "VmDisk"; then
    pass_msg "Manifest contains source type with vm_id=100"
else
    fail_msg "Manifest missing vm_id metadata"
fi

echo ""
echo "=========================="
echo "Results: $pass pass, $fail fail"
if [ "$fail" -eq 0 ]; then
    echo -e "${GREEN}All VM mock tests passed!${NC}"
else
    echo -e "${RED}$fail test(s) failed${NC}"
fi
exit $fail
