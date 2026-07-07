#!/usr/bin/env bash
set -euo pipefail

RMRF="./target/release/rmrf"
WORK="/tmp/rmrf-bench"
STORE="/tmp/rmrf-bench-store"

rm -rf "$WORK" "$STORE"
mkdir -p "$WORK"

echo "=== RMRF Benchmark ==="
echo ""

# 1. Small file (1KB)
dd if=/dev/urandom bs=1024 count=1 of="$WORK/small.bin" 2>/dev/null
echo "--- 1 KB file ---"
time "$RMRF" backup "$WORK/small.bin" -s "$STORE" 2>&1
rm -rf "$STORE"

# 2. Medium file (10MB)
dd if=/dev/urandom bs=1048576 count=10 of="$WORK/medium.bin" 2>/dev/null
echo "--- 10 MB file ---"
time "$RMRF" backup "$WORK/medium.bin" -s "$STORE" 2>&1
rm -rf "$STORE"

# 3. Large file (100MB)
dd if=/dev/urandom bs=1048576 count=100 of="$WORK/large.bin" 2>/dev/null
echo "--- 100 MB file ---"
time "$RMRF" backup "$WORK/large.bin" -s "$STORE" 2>&1

# 4. Restore
MANIFEST=$(ls "$STORE/manifests/" | head -1)
echo "--- Restore 100 MB ---"
time "$RMRF" restore "$STORE/manifests/$MANIFEST" -s "$STORE" -o "$WORK/restored.bin" 2>&1

# 5. Verify
echo "--- Verify 100 MB ---"
time "$RMRF" verify "$STORE/manifests/$MANIFEST" -s "$STORE" 2>&1

# 6. Verify --rehash
echo "--- Verify --rehash 100 MB ---"
time "$RMRF" verify "$STORE/manifests/$MANIFEST" -s "$STORE" --rehash 2>&1

# 7. Dedup (second backup of same file)
echo "--- Dedup (second backup, same file) ---"
time "$RMRF" backup "$WORK/large.bin" -s "$STORE" 2>&1

# 8. Compression ratio
echo "--- Compression ratio ---"
ORIG=$(stat --format=%s "$WORK/large.bin")
STORE_SIZE=$(du -sb "$STORE/chunks" | cut -f1)
echo "Original: $ORIG bytes"
echo "Store:    $STORE_SIZE bytes"
echo "Ratio:    $(echo "scale=2; $STORE_SIZE * 100 / $ORIG" | bc)%"

# 9. Parallel restore (--jobs 8)
echo "--- Restore 100 MB (--jobs 8) ---"
time "$RMRF" restore "$STORE/manifests/$MANIFEST" -s "$STORE" -o "$WORK/restored2.bin" --jobs 8 2>&1

# 10. GC dry-run
echo "--- GC dry-run ---"
time "$RMRF" gc -s "$STORE" --dry-run 2>&1

rm -rf "$WORK" "$STORE"
echo ""
echo "=== Benchmark complete ==="
