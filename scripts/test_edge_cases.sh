#!/usr/bin/env bash
# test_edge_cases.sh — Fichier vide, 1 octet, zéros, motifs, Unicode, permissions
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
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

TEST_DIR=$(mktemp -d)
STORE="$TEST_DIR/store"

do_test() {
    local desc="$1"
    local input="$2"
    $RMRF -- backup "$input" "$STORE" > /dev/null
    MANIFEST=$(ls -t "$STORE/manifests/"*.bincode | head -1)
    $RMRF -- restore "$MANIFEST" "$STORE" > /dev/null 2>&1
    OUTPUT=$(echo "$MANIFEST" | xargs basename | sed 's/^[^-]*-//; s/\.bincode$//')
    if diff "$input" "$OUTPUT" > /dev/null 2>&1; then
        pass_msg "$desc: bit-identical"
    else
        fail_msg "$desc: diff mismatch"
    fi
    rm -rf "${STORE:?}/"*
}

# Test 1: Empty file
touch "$TEST_DIR/empty"
do_test "Empty file (0 bytes)" "$TEST_DIR/empty"

# Test 2: 1 byte
printf "X" > "$TEST_DIR/one_byte"
do_test "One byte file" "$TEST_DIR/one_byte"

# Test 3: All zeros (1 KB)
dd if=/dev/zero of="$TEST_DIR/zeros" bs=1024 count=1 2>/dev/null
do_test "All zeros (1 KB)" "$TEST_DIR/zeros"

# Test 4: Repeated pattern
python3 -c "print('RMRF' * 250)" > "$TEST_DIR/repeated" 2>/dev/null || printf 'RMRF%.0s' {1..250} > "$TEST_DIR/repeated"
do_test "Repeated 'RMRF' pattern" "$TEST_DIR/repeated"

# Test 5: All 0xFF bytes
python3 -c "import sys; sys.stdout.buffer.write(b'\xff' * 1024)" > "$TEST_DIR/ff_bytes" 2>/dev/null || \
    tr '\0' '\377' < /dev/zero | head -c 1024 > "$TEST_DIR/ff_bytes"
do_test "All 0xFF bytes (1 KB)" "$TEST_DIR/ff_bytes"

# Test 6: Unicode filename
echo "unicode content" > "$TEST_DIR/écoß测试.txt"
$RMRF -- backup "$TEST_DIR/écoß测试.txt" "$STORE" > /dev/null
MANIFEST=$(ls -t "$STORE/manifests/"*.bincode | head -1)
$RMRF -- restore "$MANIFEST" "$STORE" > /dev/null 2>&1
OUTPUT=$(echo "$MANIFEST" | xargs basename | sed 's/^[^-]*-//; s/\.bincode$//')
if diff "$TEST_DIR/écoß测试.txt" "$OUTPUT" > /dev/null 2>&1; then
    pass_msg "Unicode filename: bit-identical"
else
    fail_msg "Unicode filename: diff mismatch"
fi
rm -rf "${STORE:?}/"*

# Test 7: 8 KB (exact chunk size)
dd if=/dev/urandom of="$TEST_DIR/exact_chunk" bs=8192 count=1 2>/dev/null
do_test "Exact chunk size (8 KB)" "$TEST_DIR/exact_chunk"

# Test 8: 8 KB + 1 byte (crosses chunk boundary)
dd if=/dev/urandom of="$TEST_DIR/chunk_plus" bs=8193 count=1 2>/dev/null
do_test "Chunk size + 1 byte (8193 B)" "$TEST_DIR/chunk_plus"

echo ""
echo "=========================="
echo "Results: $pass pass, $fail fail"
if [ "$fail" -eq 0 ]; then
    echo -e "${GREEN}All edge case tests passed!${NC}"
else
    echo -e "${RED}$fail test(s) failed${NC}"
fi
exit $fail
