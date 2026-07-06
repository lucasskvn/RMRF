#!/usr/bin/env bash
# test_basic.sh — Backup + restore + diff bit-à-bit (critère done Phase 1)
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
INPUT="$TEST_DIR/input.txt"
RESTORED="$TEST_DIR/restored.txt"
STORE="$TEST_DIR/store"

# Test 1: Simple text file
echo "Hello RMRF - integration test" > "$INPUT"
$RMRF -- backup "$INPUT" "$STORE" > /dev/null
MANIFEST=$(ls "$STORE/manifests/"*.bincode | head -1)
$RMRF -- restore "$MANIFEST" "$STORE" > /dev/null 2>&1
OUTPUT=$(echo "$MANIFEST" | xargs basename | sed 's/^[^-]*-//; s/\.bincode$//')
if diff "$INPUT" "$OUTPUT" > /dev/null 2>&1; then
    pass_msg "Simple text file: backup + restore bit-identical"
else
    fail_msg "Simple text file: diff mismatch"
fi
rm -rf "$TEST_DIR"/*

# Test 2: Empty file
touch "$INPUT"
$RMRF -- backup "$INPUT" "$STORE" > /dev/null
MANIFEST=$(ls "$STORE/manifests/"*.bincode | head -1)
$RMRF -- restore "$MANIFEST" "$STORE" > /dev/null 2>&1
OUTPUT=$(echo "$MANIFEST" | xargs basename | sed 's/^[^-]*-//; s/\.bincode$//')
if [ -f "$OUTPUT" ] && [ ! -s "$OUTPUT" ]; then
    pass_msg "Empty file: restore is empty"
else
    fail_msg "Empty file: restore failed or not empty"
fi
rm -rf "$TEST_DIR"/*

# Test 3: Single byte
printf "A" > "$INPUT"
$RMRF -- backup "$INPUT" "$STORE" > /dev/null
MANIFEST=$(ls "$STORE/manifests/"*.bincode | head -1)
$RMRF -- restore "$MANIFEST" "$STORE" > /dev/null 2>&1
OUTPUT=$(echo "$MANIFEST" | xargs basename | sed 's/^[^-]*-//; s/\.bincode$//')
if diff "$INPUT" "$OUTPUT" > /dev/null 2>&1; then
    pass_msg "Single byte file: backup + restore bit-identical"
else
    fail_msg "Single byte file: diff mismatch"
fi
rm -rf "$TEST_DIR"/*

# Test 4: File with only null bytes
dd if=/dev/zero of="$INPUT" bs=1024 count=1 2>/dev/null
$RMRF -- backup "$INPUT" "$STORE" > /dev/null
MANIFEST=$(ls "$STORE/manifests/"*.bincode | head -1)
$RMRF -- restore "$MANIFEST" "$STORE" > /dev/null 2>&1
OUTPUT=$(echo "$MANIFEST" | xargs basename | sed 's/^[^-]*-//; s/\.bincode$//')
if diff "$INPUT" "$OUTPUT" > /dev/null 2>&1; then
    pass_msg "Null bytes (1KB): backup + restore bit-identical"
else
    fail_msg "Null bytes (1KB): diff mismatch"
fi
rm -rf "$TEST_DIR"/*

# Test 5: Repeated pattern file
python3 -c "print('AB' * 5000)" > "$INPUT" 2>/dev/null || printf 'AB%.0s' {1..5000} > "$INPUT"
$RMRF -- backup "$INPUT" "$STORE" > /dev/null
MANIFEST=$(ls "$STORE/manifests/"*.bincode | head -1)
$RMRF -- restore "$MANIFEST" "$STORE" > /dev/null 2>&1
OUTPUT=$(echo "$MANIFEST" | xargs basename | sed 's/^[^-]*-//; s/\.bincode$//')
if diff "$INPUT" "$OUTPUT" > /dev/null 2>&1; then
    pass_msg "Repeated pattern: backup + restore bit-identical"
else
    fail_msg "Repeated pattern: diff mismatch"
fi
rm -rf "$TEST_DIR"/*

# Test 6: Large file (1 MB random)
dd if=/dev/urandom of="$INPUT" bs=1024 count=1024 2>/dev/null
$RMRF -- backup "$INPUT" "$STORE" > /dev/null
MANIFEST=$(ls "$STORE/manifests/"*.bincode | head -1)
$RMRF -- restore "$MANIFEST" "$STORE" > /dev/null 2>&1
OUTPUT=$(echo "$MANIFEST" | xargs basename | sed 's/^[^-]*-//; s/\.bincode$//')
if diff "$INPUT" "$OUTPUT" > /dev/null 2>&1; then
    pass_msg "Large file (1 MB random): backup + restore bit-identical"
else
    fail_msg "Large file (1 MB random): diff mismatch"
fi
rm -rf "$TEST_DIR"/*

echo ""
echo "=========================="
echo "Results: $pass pass, $fail fail"
if [ "$fail" -eq 0 ]; then
    echo -e "${GREEN}All basic tests passed!${NC}"
else
    echo -e "${RED}$fail test(s) failed${NC}"
fi
exit $fail
