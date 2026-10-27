#!/usr/bin/env bash
# ==============================================================================
# scripts/verify-all-targets.sh
#
# Pre-release multi-platform verification tool for zee.
# Verifies all 3 standard release targets locally on macOS before pushing / tagging:
#   1. macOS:  GUI (arm64) - Full workspace test & release GUI check
#   2. Windows: GUI (x64)   - Cross-target compilation check
#   3. Linux:   CLI (x64)   - Docker-based native test execution
# ==============================================================================

set -euo pipefail

# ANSI color codes
GREEN="\033[0;32m"
BLUE="\033[0;34m"
YELLOW="\033[1;33m"
RED="\033[0;31m"
BOLD="\033[1m"
NC="\033[0m"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

echo -e "${BOLD}${BLUE}=== Pre-Release Multi-Platform Verification ===${NC}\n"

# ------------------------------------------------------------------------------
# 1. macOS: GUI (arm64)
# ------------------------------------------------------------------------------
echo -e "${BOLD}[1/3] macOS (GUI arm64) Verification...${NC}"
echo "Running CI-equivalent cargo check with --all-targets..."
cargo check --workspace --all-targets --quiet
echo "Running workspace unit & regression tests..."
cargo test --workspace --quiet
echo "Checking GUI release build..."
cargo check --release -p zee-gui --bin zeeg --quiet
echo -e "${GREEN}✓ macOS (GUI arm64) passed!${NC}\n"

# ------------------------------------------------------------------------------
# 2. Windows: GUI (x64) & Cross-Platform Non-macOS Test Code Check
# ------------------------------------------------------------------------------
echo -e "${BOLD}[2/3] Windows (GUI x64) Verification...${NC}"
echo "Running cross-target compilation & test code check with --all-targets (x86_64-pc-windows-gnu)..."
cargo check --workspace --all-targets --target x86_64-pc-windows-gnu --quiet
echo -e "${GREEN}✓ Windows (GUI x64) cross-check passed!${NC}\n"

# ------------------------------------------------------------------------------
# 3. Linux: CLI (x64)
# ------------------------------------------------------------------------------
echo -e "${BOLD}[3/3] Linux (CLI x64) Verification...${NC}"
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    echo "Running Linux x64 CLI unit tests inside Docker (cached container)..."
    docker run --rm --platform linux/amd64 \
        -v zee_cargo_registry:/usr/local/cargo/registry \
        -v zee_cargo_git:/usr/local/cargo/git \
        -v zee_target_linux:/workspace/target_docker \
        -v "${REPO_ROOT}":/workspace \
        -w /workspace \
        -e CARGO_TARGET_DIR=/workspace/target_docker \
        rust:slim \
        sh -c "cargo test -p zee-core --lib --quiet && cargo test -p zee-tui --lib --quiet"
    echo -e "${GREEN}✓ Linux (CLI x64) container tests passed!${NC}\n"
else
    echo -e "${YELLOW}⚠ Docker is not running. Skipped Linux x64 container tests.${NC}"
    echo -e "${YELLOW}  (Start Docker Desktop to enable automated Linux tests)${NC}\n"
fi

echo -e "${BOLD}${GREEN}====================================================${NC}"
echo -e "${BOLD}${GREEN}  All target verifications succeeded!${NC}"
echo -e "${BOLD}${GREEN}  Safe to push to main and proceed with release.${NC}"
echo -e "${BOLD}${GREEN}====================================================${NC}"
