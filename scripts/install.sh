#!/usr/bin/env bash
#
# Kamuy Wallet Installation Script
# Builds CLI and Steward binaries and packages them into the skill directory
#
# Usage: ./scripts/install.sh [--release] [--target <target>]
#
# Options:
#   --release      Build in release mode (default: true)
#   --target       Build for specific target triple
#   --skip-steward Skip building the steward binary
#   --help         Show this help message
#
# Requirements:
#   - Rust toolchain (rustc, cargo)
#   - sha256sum (for checksum generation)
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SKILL_DIR="${PROJECT_ROOT}/skills/kamuy-wallet"

# Binary names
CLI_BIN="kamuy"
STEWARD_BIN="kamuy-steward"

# Build options
RELEASE_MODE=true
TARGET=""
SKIP_STEWARD=false

# Print colored message
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Print banner
print_banner() {
    echo ""
    echo "=============================================="
    echo "     Kamuy Wallet Installation Script"
    echo "=============================================="
    echo ""
}

# Show help
show_help() {
    cat << EOF
Kamuy Wallet Installation Script

Usage: ./scripts/install.sh [OPTIONS]

Options:
  --release        Build in release mode (default: true)
  --debug          Build in debug mode
  --target TARGET  Build for specific target triple
  --skip-steward   Skip building the steward binary
  --help           Show this help message

Examples:
  ./scripts/install.sh                    # Build release binaries
  ./scripts/install.sh --debug            # Build debug binaries
  ./scripts/install.sh --skip-steward     # Build only CLI binary

Requirements:
  - Rust toolchain (rustc, cargo)
  - sha256sum (for checksum generation)

EOF
    exit 0
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --release)
                RELEASE_MODE=true
                shift
                ;;
            --debug)
                RELEASE_MODE=false
                shift
                ;;
            --target)
                TARGET="$2"
                shift 2
                ;;
            --skip-steward)
                SKIP_STEWARD=true
                shift
                ;;
            --help|-h)
                show_help
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                ;;
        esac
    done
}

# Check for required dependencies
check_dependencies() {
    log_info "Checking dependencies..."

    # Check for Rust/Cargo
    if ! command -v rustc &> /dev/null; then
        log_error "rustc is not installed. Please install Rust from https://rustup.rs"
        exit 1
    fi

    if ! command -v cargo &> /dev/null; then
        log_error "cargo is not installed. Please install Rust from https://rustup.rs"
        exit 1
    fi

    # Check for sha256sum
    if ! command -v sha256sum &> /dev/null; then
        log_warning "sha256sum not found, using shasum (macOS)"
        if ! command -v shasum &> /dev/null; then
            log_error "Neither sha256sum nor shasum is available"
            exit 1
        fi
    fi

    local rust_version
    rust_version=$(rustc --version)
    log_success "Found: $rust_version"

    local cargo_version
    cargo_version=$(cargo --version)
    log_success "Found: $cargo_version"
}

# Build binary
build_binary() {
    local package=$1
    local bin_name=$2

    log_info "Building ${bin_name}..."

    local build_args=("--package" "${package}")

    if [[ "$RELEASE_MODE" == true ]]; then
        build_args+=("--release")
    fi

    if [[ -n "$TARGET" ]]; then
        build_args+=("--target" "$TARGET")
    fi

    pushd "$PROJECT_ROOT" > /dev/null
    cargo build "${build_args[@]}"
    popd > /dev/null

    log_success "Built ${bin_name}"
}

# Get binary path based on build mode
get_binary_path() {
    local bin_name=$1
    local base_path="${PROJECT_ROOT}/target"

    if [[ -n "$TARGET" ]]; then
        base_path="${base_path}/${TARGET}"
    fi

    if [[ "$RELEASE_MODE" == true ]]; then
        echo "${base_path}/release/${bin_name}"
    else
        echo "${base_path}/debug/${bin_name}"
    fi
}

# Generate SHA256 checksum
generate_checksum() {
    local file=$1

    if command -v sha256sum &> /dev/null; then
        sha256sum "$file" | cut -d' ' -f1
    else
        shasum -a 256 "$file" | cut -d' ' -f1
    fi
}

# Copy binaries to skill directory
copy_binaries() {
    log_info "Copying binaries to skill directory..."

    # Create bin directory in skill folder
    mkdir -p "${SKILL_DIR}/bin"

    # Copy CLI binary
    local cli_src
    cli_src=$(get_binary_path "$CLI_BIN")

    if [[ -f "$cli_src" ]]; then
        cp "$cli_src" "${SKILL_DIR}/bin/${CLI_BIN}"
        chmod +x "${SKILL_DIR}/bin/${CLI_BIN}"
        log_success "Copied ${CLI_BIN} to ${SKILL_DIR}/bin/"
    else
        log_error "CLI binary not found at ${cli_src}"
        exit 1
    fi

    # Copy Steward binary (if not skipped)
    if [[ "$SKIP_STEWARD" == false ]]; then
        local steward_src
        steward_src=$(get_binary_path "$STEWARD_BIN")

        if [[ -f "$steward_src" ]]; then
            cp "$steward_src" "${SKILL_DIR}/bin/${STEWARD_BIN}"
            chmod +x "${SKILL_DIR}/bin/${STEWARD_BIN}"
            log_success "Copied ${STEWARD_BIN} to ${SKILL_DIR}/bin/"
        else
            log_error "Steward binary not found at ${steward_src}"
            exit 1
        fi
    fi
}

# Generate installation manifest
generate_manifest() {
    log_info "Generating installation manifest..."

    local manifest_path="${SKILL_DIR}/install-manifest.yaml"
    local build_date
    build_date=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    local build_mode="debug"
    if [[ "$RELEASE_MODE" == true ]]; then
        build_mode="release"
    fi

    local rust_version
    rust_version=$(rustc --version | cut -d' ' -f2)

    local target_triple
    if [[ -n "$TARGET" ]]; then
        target_triple="$TARGET"
    else
        target_triple=$(rustc -vV | grep host | cut -d' ' -f2)
    fi

    # Generate checksums
    local cli_checksum
    cli_checksum=$(generate_checksum "${SKILL_DIR}/bin/${CLI_BIN}")

    local steward_checksum=""
    if [[ "$SKIP_STEWARD" == false ]]; then
        steward_checksum=$(generate_checksum "${SKILL_DIR}/bin/${STEWARD_BIN}")
    fi

    cat > "$manifest_path" << EOF
# Kamuy Wallet Installation Manifest
# Generated by install.sh on ${build_date}

build:
  mode: ${build_mode}
  date: ${build_date}
  rust_version: ${rust_version}
  target: ${target_triple}

binaries:
  cli:
    name: ${CLI_BIN}
    path: bin/${CLI_BIN}
    checksum:
      algorithm: sha256
      value: ${cli_checksum}
EOF

    if [[ "$SKIP_STEWARD" == false ]]; then
        cat >> "$manifest_path" << EOF
  steward:
    name: ${STEWARD_BIN}
    path: bin/${STEWARD_BIN}
    checksum:
      algorithm: sha256
      value: ${steward_checksum}
EOF
    fi

    cat >> "$manifest_path" << EOF

installation:
  method: script
  script_version: "1.0.0"

openclaw:
  install_command: "openclaw skill install ./kamuy-wallet"
  supported_commands:
    - "openclaw skill install ./kamuy-wallet"
    - "openclaw skill install /path/to/kamuy-wallet"
EOF

    log_success "Generated install manifest: ${manifest_path}"
}

# Print success message
print_success() {
    echo ""
    echo "=============================================="
    echo -e "${GREEN}  Installation Complete!${NC}"
    echo "=============================================="
    echo ""
    echo "Binaries installed to:"
    echo "  ${SKILL_DIR}/bin/${CLI_BIN}"
    if [[ "$SKIP_STEWARD" == false ]]; then
        echo "  ${SKILL_DIR}/bin/${STEWARD_BIN}"
    fi
    echo ""
    echo "Manifest:"
    echo "  ${SKILL_DIR}/install-manifest.yaml"
    echo ""
    echo "Next steps:"
    echo ""
    echo "  1. Initialize the wallet:"
    echo "     ${SKILL_DIR}/bin/${CLI_BIN} init"
    echo ""
    echo "  2. Install with OpenClaw:"
    echo "     cd ${PROJECT_ROOT}"
    echo "     openclaw skill install ./kamuy-wallet"
    echo ""
    echo "  3. Verify installation:"
    echo "     ${SKILL_DIR}/bin/${CLI_BIN} --version"
    if [[ "$SKIP_STEWARD" == false ]]; then
        echo "     ${SKILL_DIR}/bin/${STEWARD_BIN} --version"
    fi
    echo ""
    echo "For more information, see:"
    echo "  ${SKILL_DIR}/skill.md"
    echo ""
}

# Main function
main() {
    print_banner
    parse_args "$@"
    check_dependencies

    # Build CLI binary
    build_binary "kamuy-cli" "$CLI_BIN"

    # Build Steward binary (if not skipped)
    if [[ "$SKIP_STEWARD" == false ]]; then
        build_binary "kamuy-steward" "$STEWARD_BIN"
    fi

    copy_binaries
    generate_manifest
    print_success
}

# Run main
main "$@"