#!/usr/bin/env bash
# test_sizes.sh — Various file sizes for stress testing
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
RMRF="cargo run --manifest-path "$PROJECT_DIR/Cargo.toml" --quiet"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

pass=0
fail=0
skip=0

pass_msg()  { echo -e "${GREEN}PASS${NC} $1"; ((++pass)); }
fail_msg()  { echo -e "${RED}FAIL${NC} $1"; ((++fail)); }
skip_msg()  { echo -e "${YELLOW}SKIP${NC} $1"; ((++skip)); }

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

TEST_DIR=$(mktemp -d)
STORE="$TEST_DIR/store"
INPUT="$TEST_DIR/input.bin"

do_size_test() {
    local label="$1"
    local size_kb="$2"

    [ "$size_kb" -gt 102400 ] && { skip_msg "$label: too large for ci"; return; }

    dd if=/dev/urandom of="$INPUT" bs=1024 count="$size_kb" 2>/dev/null || {
        skip_msg "$label: dd failed"
        return
    }
    if ! $RMRF -- backup "$INPUT" -s "$STORE" > /dev/null 2>&1; then
        fail_msg "$label: backup failed"
        rm -rf "${STORE:?}/"*
        return
    fi
    MANIFEST=$(ls -t "$STORE/manifests/"*.bincode | head -1)
    if ! $RMRF -- restore "$MANIFEST" -s "$STORE" > /dev/null 2>&1; then
        fail_msg "$label: restore failed"
        rm -rf "${STORE:?}/"*
        return
    fi
    OUTPUT=$(echo "$MANIFEST" | xargs basename | sed 's/^[^-]*-//; s/\.bincode$//')
    if diff "$INPUT" "$OUTPUT" > /dev/null 2>&1; then
        local chunk_count=$(find "$STORE/manifests/" -name "*.bincode" -exec bincode 2>/dev/null \; | wc -l || echo "?")
        pass_msg "$label: bit-identical"
    else
        fail_msg "$label: diff mismatch"
    fi
    rm -rf "${STORE:?}/"*
}

echo "Size stress tests (random data):"
echo ""

# Small files
do_size_test "1 KB"       1
do_size_test "8 KB"       8
do_size_test "16 KB"     16
do_size_test "64 KB"     64
do_size_test "128 KB"   128
do_size_test "256 KB"   256
do_size_test "512 KB"   512

# Medium file
do_size_test "1 MB"    1024

# Larger file (comment out if too slow)
do_size_test "5 MB"    5120

echo ""
echo "=========================="
echo "Results: $pass pass, $fail fail, $skip skip"
if [ "$fail" -eq 0 ]; then
    echo -e "${GREEN}All size tests passed!${NC}"
else
    echo -e "${RED}$fail test(s) failed${NC}"
fi
exit $fail
