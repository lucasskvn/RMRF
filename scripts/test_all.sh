#!/usr/bin/env bash
# test_all.sh — Runner global pour tous les tests
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

overall_pass=0
overall_fail=0

run_script() {
    local script="$1"
    echo -e "${BLUE}━━━ Running: $(basename "$script") ━━━${NC}"
    if bash "$script"; then
        ((overall_pass++))
        echo -e "${GREEN}✓ $(basename "$script") PASSED${NC}"
    else
        ((overall_fail++))
        echo -e "${RED}✗ $(basename "$script") FAILED${NC}"
    fi
    echo ""
}

echo -e "${BLUE}╔══════════════════════════════╗${NC}"
echo -e "${BLUE}║   RMRF Test Suite Runner     ║${NC}"
echo -e "${BLUE}╚══════════════════════════════╝${NC}"
echo ""

# First build everything
echo -e "${BLUE}Building workspace...${NC}"
if cargo build --workspace --manifest-path "$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1; then
    echo -e "${GREEN}Build: OK${NC}"
else
    echo -e "${RED}Build: FAILED${NC}"
    exit 1
fi
echo ""

# Run unit tests
echo -e "${BLUE}━━━ Running: cargo test --workspace ━━━${NC}"
if cargo test --workspace --manifest-path "$PROJECT_DIR/Cargo.toml" 2>&1 | tail -5; then
    echo -e "${GREEN}✓ Unit tests PASSED${NC}"
else
    echo -e "${RED}✗ Unit tests FAILED${NC}"
    ((overall_fail++))
fi
echo ""

# Run integration scripts
run_script "$SCRIPT_DIR/test_basic.sh"
run_script "$SCRIPT_DIR/test_dedup.sh"
run_script "$SCRIPT_DIR/test_edge_cases.sh"
run_script "$SCRIPT_DIR/test_sizes.sh"
run_script "$SCRIPT_DIR/test_errors.sh"

echo -e "${BLUE}╔══════════════════════════════╗${NC}"
echo -e "${BLUE}║        Final Results          ║${NC}"
echo -e "${BLUE}╚══════════════════════════════╝${NC}"
echo -e "Scripts passed: ${GREEN}$overall_pass${NC}"
echo -e "Scripts failed: ${RED}$overall_fail${NC}"

if [ "$overall_fail" -eq 0 ]; then
    echo -e "\n${GREEN}✓ All tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}✗ Some tests failed${NC}"
    exit 1
fi
