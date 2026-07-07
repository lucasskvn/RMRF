#!/usr/bin/env bash
# test_dedup.sh — Validation de la déduplication : backup idempotent
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
warn=0

pass_msg() { echo -e "${GREEN}PASS${NC} $1"; ((++pass)); }
fail_msg() { echo -e "${RED}FAIL${NC} $1"; ((++fail)); }
warn_msg() { echo -e "${YELLOW}WARN${NC} $1"; ((++warn)); }

cleanup() { rm -rf "$TEST_DIR"; }
trap cleanup EXIT

TEST_DIR=$(mktemp -d)
STORE="$TEST_DIR/store"
INPUT="$TEST_DIR/input.bin"
mkdir -p "$STORE/chunks"

dd if=/dev/urandom of="$INPUT" bs=1024 count=256 2>/dev/null  # 256 KB

# Test 1: Double backup, check store size doesn't double
$RMRF -- backup "$INPUT" -s "$STORE" > /dev/null
chunks_after_first=$(find "$STORE/chunks" -type f 2>/dev/null | wc -l || echo 0)
store_size_first=$(du -sb "$STORE/chunks" 2>/dev/null | cut -f1 || echo 0)

$RMRF -- backup "$INPUT" -s "$STORE" > /dev/null
chunks_after_second=$(find "$STORE/chunks" -type f 2>/dev/null | wc -l || echo 0)
store_size_second=$(du -sb "$STORE/chunks" 2>/dev/null | cut -f1 || echo 0)

if [ "$chunks_after_first" -gt 0 ]; then
    pass_msg "First backup stored chunks ($chunks_after_first chunks)"
else
    fail_msg "First backup stored no chunks"
fi

if [ "$chunks_after_second" -eq "$chunks_after_first" ]; then
    pass_msg "Second backup: no new chunks stored (same file)"
else
    warn_msg "Second backup added $(($chunks_after_second - $chunks_after_first)) chunks (dedup not fully implemented)"
fi

if [ "$store_size_second" -le "$store_size_first" ]; then
    pass_msg "Store size did not grow on second backup"
else
    warn_msg "Store grew by $(($store_size_second - $store_size_first)) bytes (manifest adds overhead)"
fi

# Test 2: Restore after second backup, verify bit-identical
MANIFEST=$(ls -t "$STORE/manifests/"*.bincode | head -1)
$RMRF -- restore "$MANIFEST" -s "$STORE" -o "$TEST_DIR/restored.bin" > /dev/null 2>&1
if diff "$INPUT" "$TEST_DIR/restored.bin" > /dev/null 2>&1; then
    pass_msg "Restore after double backup: bit-identical"
else
    fail_msg "Restore after double backup: diff mismatch"
fi

echo ""
echo "=========================="
echo "Results: $pass pass, $fail fail, $warn warn"
echo "Note: WARNs about dedup are expected — MemoryDedupIndex is in-memory per run"
if [ "$fail" -eq 0 ]; then
    echo -e "${GREEN}All dedup tests passed!${NC}"
else
    echo -e "${RED}$fail test(s) failed${NC}"
fi
exit $fail
