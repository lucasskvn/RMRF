#!/usr/bin/env bash
# test_errors.sh — Error handling and edge cases
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

cleanup() { rm -rf "$TEST_DIR"; }
trap cleanup EXIT

TEST_DIR=$(mktemp -d)
STORE="$TEST_DIR/store"

# Test 1: Unknown command
if ($RMRF -- foobar 2>&1 || true) | grep -qi "unknown"; then
    pass_msg "Unknown command: prints error"
else
    fail_msg "Unknown command: should print error"
fi

# Test 2: Missing required argument
if ! $RMRF -- backup 2>&1; then
    pass_msg "Missing path argument: fails with non-zero exit"
else
    fail_msg "Missing path argument: should fail"
fi

# Test 3: Non-existent source file
if ($RMRF -- backup /tmp/does_not_exist_rmrf_test.bin "$STORE" 2>&1 || true) | grep -qi "error"; then
    pass_msg "Non-existent source: prints error"
else
    fail_msg "Non-existent source: should print error"
fi

# Test 4: Non-existent manifest
if ($RMRF -- restore /tmp/does_not_exist_manifest.bincode "$STORE" 2>&1 || true) | grep -qi "error"; then
    pass_msg "Non-existent manifest: prints error"
else
    fail_msg "Non-existent manifest: should print error"
fi

# Test 5: Corrupted manifest
echo "not a valid bincode file" > "$TEST_DIR/corrupt.bincode"
if ($RMRF -- restore "$TEST_DIR/corrupt.bincode" "$STORE" 2>&1 || true) | grep -qi "error"; then
    pass_msg "Corrupted manifest: prints error"
else
    fail_msg "Corrupted manifest: should print error"
fi

# Test 6: Non-writable store
mkdir -p "$TEST_DIR/readonly"
chmod 444 "$TEST_DIR/readonly"
if ($RMRF -- backup "$TEST_DIR/corrupt.bincode" "$TEST_DIR/readonly" 2>&1 || true) | grep -qiE "error|permission"; then
    pass_msg "Non-writable store: prints error"
else
    fail_msg "Non-writable store: should print error"
fi
chmod 755 "$TEST_DIR/readonly"

# Test 7: Missing chunk (backup then delete chunks, try restore)
echo "test content" > "$TEST_DIR/test.txt"
$RMRF -- backup "$TEST_DIR/test.txt" "$STORE" > /dev/null
find "$STORE/chunks" -type f -delete 2>/dev/null || true
MANIFEST=$(ls -t "$STORE/manifests/"*.bincode | head -1)
if ($RMRF -- restore "$MANIFEST" "$STORE" 2>&1 || true) | grep -qiE "not found|error"; then
    pass_msg "Missing chunk: prints error"
else
    fail_msg "Missing chunk: should print error"
fi
rm -rf "${STORE:?}" 2>/dev/null || true
mkdir -p "$STORE"

# Test 8: No args prints usage
if ($RMRF 2>&1 || true) | grep -qiE "error|usage|backup|restore"; then
    pass_msg "No args: prints error/usage"
else
    fail_msg "No args: should print error/usage"
fi

echo ""
echo "=========================="
echo "Results: $pass pass, $fail fail"
if [ "$fail" -eq 0 ]; then
    echo -e "${GREEN}All error handling tests passed!${NC}"
else
    echo -e "${RED}$fail test(s) failed${NC}"
fi
exit $fail
