#!/usr/bin/env bash
#
# Kamuy Wallet Release Build Script
# Builds binaries for multiple platforms and creates distribution packages
#
# Usage: ./scripts/build-release.sh [OPTIONS]
#
# Options:
#   --version VERSION    Version number for release (required)
#   --targets TARGETS    Comma-separated list of targets (default: all)
#   --output DIR         Output directory for release artifacts
#   --skip-strip         Skip stripping debug symbols
#   --skip-checksum      Skip generating checksums
#   --help               Show this help message
#
# Supported targets:
#   linux-x64            x86_64-unknown-linux-gnu
#   linux-arm64          aarch64-unknown-linux-gnu
#   macos-x64            x86_64-apple-darwin
#   macos-arm64          aarch64-apple-darwin
#   all                  Build all targets (default)
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Binary names
CLI_BIN="kamuy"
STEWARD_BIN="kamuy-steward"

# Package names
PACKAGE_NAME="kamuy-wallet"

# Build options
VERSION=""
OUTPUT_DIR="${PROJECT_ROOT}/release"
SKIP_STRIP=false
SKIP_CHECKSUM=false

# Target mappings
declare -A TARGET_MAP=(
    ["linux-x64"]="x86_64-unknown-linux-gnu"
    ["linux-arm64"]="aarch64-unknown-linux-gnu"
    ["macos-x64"]="x86_64-apple-darwin"
    ["macos-arm64"]="aarch64-apple-darwin"
)

# Selected targets (default: all)
SELECTED_TARGETS=("linux-x64" "linux-arm64" "macos-x64" "macos-arm64")

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

log_step() {
    echo -e "${CYAN}[STEP]${NC} $1"
}

# Print banner
print_banner() {
    echo ""
    echo "=============================================="
    echo "    Kamuy Wallet Release Build Script"
    echo "=============================================="
    echo ""
}

# Show help
show_help() {
    cat << EOF
Kamuy Wallet Release Build Script

Usage: ./scripts/build-release.sh [OPTIONS]

Options:
  --version VERSION    Version number for release (required)
  --targets TARGETS    Comma-separated list of targets (default: all)
  --output DIR         Output directory for release artifacts
  --skip-strip         Skip stripping debug symbols
  --skip-checksum      Skip generating checksums
  --help               Show this help message

Supported targets:
  linux-x64            x86_64-unknown-linux-gnu
  linux-arm64          aarch64-unknown-linux-gnu
  macos-x64            x86_64-apple-darwin
  macos-arm64          aarch64-apple-darwin
  all                  Build all targets (default)

Examples:
  ./scripts/build-release.sh --version 2.0.0
  ./scripts/build-release.sh --version 2.0.0 --targets linux-x64,macos-arm64
  ./scripts/build-release.sh --version 2.0.0 --output ./dist

Requirements:
  - Rust toolchain with required targets installed
  - For cross-compilation, appropriate toolchains and linkers

Installing Rust targets:
  rustup target add x86_64-unknown-linux-gnu
  rustup target add aarch64-unknown-linux-gnu
  rustup target add x86_64-apple-darwin
  rustup target add aarch64-apple-darwin

EOF
    exit 0
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --version)
                VERSION="$2"
                shift 2
                ;;
            --targets)
                IFS=',' read -ra SELECTED_TARGETS <<< "$2"
                shift 2
                ;;
            --output)
                OUTPUT_DIR="$2"
                shift 2
                ;;
            --skip-strip)
                SKIP_STRIP=true
                shift
                ;;
            --skip-checksum)
                SKIP_CHECKSUM=true
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

    # Validate required arguments
    if [[ -z "$VERSION" ]]; then
        log_error "Version number is required. Use --version <version>"
        exit 1
    fi

    # Validate selected targets
    for target in "${SELECTED_TARGETS[@]}"; do
        if [[ "$target" == "all" ]]; then
            SELECTED_TARGETS=("linux-x64" "linux-arm64" "macos-x64" "macos-arm64")
            break
        fi
        if [[ ! -v TARGET_MAP[$target] ]]; then
            log_error "Unknown target: $target"
            log_info "Supported targets: ${!TARGET_MAP[*]}"
            exit 1
        fi
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
        if ! command -v shasum &> /dev/null; then
            log_error "Neither sha256sum nor shasum is available"
            exit 1
        fi
    fi

    # Check for tar
    if ! command -v tar &> /dev/null; then
        log_error "tar is not installed"
        exit 1
    fi

    local rust_version
    rust_version=$(rustc --version)
    log_success "Found: $rust_version"

    # Check if required targets are installed
    log_info "Checking installed Rust targets..."
    local installed_targets
    installed_targets=$(rustup target list --installed)

    for target_name in "${SELECTED_TARGETS[@]}"; do
        local rust_target="${TARGET_MAP[$target_name]}"
        if ! echo "$installed_targets" | grep -q "$rust_target"; then
            log_warning "Target $rust_target is not installed. Installing..."
            rustup target add "$rust_target" || {
                log_error "Failed to install target $rust_target"
                log_info "Run: rustup target add $rust_target"
                exit 1
            }
        fi
    done

    log_success "All required targets are available"
}

# Create output directories
create_output_dirs() {
    log_info "Creating output directories..."

    mkdir -p "${OUTPUT_DIR}"
    mkdir -p "${OUTPUT_DIR}/binaries"
    mkdir -p "${OUTPUT_DIR}/packages"

    log_success "Created output directory: ${OUTPUT_DIR}"
}

# Build for a specific target
build_target() {
    local target_name=$1
    local rust_target="${TARGET_MAP[$target_name]}"

    log_step "Building for ${target_name} (${rust_target})..."

    # Build CLI binary
    log_info "Building ${CLI_BIN} for ${rust_target}..."
    cargo build --release --package kamuy-cli --target "$rust_target"
    log_success "Built ${CLI_BIN} for ${rust_target}"

    # Build Steward binary
    log_info "Building ${STEWARD_BIN} for ${rust_target}..."
    cargo build --release --package kamuy-steward --target "$rust_target"
    log_success "Built ${STEWARD_BIN} for ${rust_target}"

    # Get binary paths
    local cli_src="${PROJECT_ROOT}/target/${rust_target}/release/${CLI_BIN}"
    local steward_src="${PROJECT_ROOT}/target/${rust_target}/release/${STEWARD_BIN}"

    # Create target-specific output directory
    local target_output="${OUTPUT_DIR}/binaries/${target_name}"
    mkdir -p "$target_output"

    # Copy binaries
    cp "$cli_src" "${target_output}/${CLI_BIN}"
    cp "$steward_src" "${target_output}/${STEWARD_BIN}"

    # Strip binaries (if not skipped and on the same platform type)
    if [[ "$SKIP_STRIP" == false ]]; then
        strip_binary "${target_output}/${CLI_BIN}" "$rust_target"
        strip_binary "${target_output}/${STEWARD_BIN}" "$rust_target"
    fi

    log_success "Copied binaries to ${target_output}"
}

# Strip binary for size optimization
strip_binary() {
    local binary=$1
    local target=$2

    log_info "Stripping ${binary}..."

    # Determine strip command based on target
    local strip_cmd="strip"

    case "$target" in
        *-linux-*)
            # May need cross-platform strip tools
            if command -v "${target}-strip" &> /dev/null; then
                strip_cmd="${target}-strip"
            fi
            ;;
        *-darwin-*)
            # macOS binaries can only be stripped on macOS
            if [[ "$(uname -s)" != "Darwin" ]]; then
                log_warning "Cannot strip macOS binaries on $(uname -s), skipping..."
                return
            fi
            ;;
    esac

    if $strip_cmd "$binary" 2>/dev/null; then
        local size_before size_after
        size_before=$(stat -f%z "$binary" 2>/dev/null || stat -c%s "$binary" 2>/dev/null)
        size_after=$(stat -f%z "$binary" 2>/dev/null || stat -c%s "$binary" 2>/dev/null)
        log_success "Stripped binary: ${size_before} -> ${size_after} bytes"
    else
        log_warning "Could not strip binary (this is normal for cross-compiled targets)"
    fi
}

# Create tarball package for a target
create_package() {
    local target_name=$1
    local target_output="${OUTPUT_DIR}/binaries/${target_name}"
    local package_name="${PACKAGE_NAME}-${VERSION}-${target_name}"

    log_step "Creating package for ${target_name}..."

    # Create temporary package directory
    local temp_dir
    temp_dir=$(mktemp -d)
    local package_dir="${temp_dir}/${package_name}"
    mkdir -p "${package_dir}/bin"
    mkdir -p "${package_dir}/docs"

    # Copy binaries
    cp "${target_output}/${CLI_BIN}" "${package_dir}/bin/"
    cp "${target_output}/${STEWARD_BIN}" "${package_dir}/bin/"

    # Copy documentation
    if [[ -f "${PROJECT_ROOT}/README.md" ]]; then
        cp "${PROJECT_ROOT}/README.md" "${package_dir}/docs/"
    fi
    if [[ -f "${PROJECT_ROOT}/LICENSE" ]]; then
        cp "${PROJECT_ROOT}/LICENSE" "${package_dir}/"
    fi

    # Create version file
    cat > "${package_dir}/VERSION" << EOF
${PACKAGE_NAME} v${VERSION}
Target: ${target_name}
Built: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
Rust: $(rustc --version)

Binaries:
  - bin/${CLI_BIN}
  - bin/${STEWARD_BIN}
EOF

    # Create installation instructions
    cat > "${package_dir}/INSTALL.md" << EOF
# Kamuy Wallet v${VERSION} - ${target_name}

## Installation

1. Extract this archive
2. Add the \`bin/\` directory to your PATH, or:
   \`\`\`
   sudo cp bin/* /usr/local/bin/
   \`\`\`

## Binaries

- \`kamuy\` - CLI tool for wallet operations
- \`kamuy-steward\` - Policy engine and transaction validator

## Quick Start

\`\`\`
# Initialize a new wallet
kamuy init

# Check wallet status
kamuy status

# Start the steward service
kamuy-steward --config ~/.kamuy/steward.yaml
\`\`\`

## Documentation

See the \`docs/\` directory for more information.

## License

MIT OR Apache-2.0
EOF

    # Create tarball
    local tarball_path="${OUTPUT_DIR}/packages/${package_name}.tar.gz"
    tar -czf "$tarball_path" -C "$temp_dir" "$package_name"

    # Cleanup
    rm -rf "$temp_dir"

    log_success "Created package: ${tarball_path}"
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

# Generate checksums file
generate_checksums() {
    log_step "Generating checksums..."

    local checksums_file="${OUTPUT_DIR}/packages/${PACKAGE_NAME}-${VERSION}-checksums.txt"

    # Clear existing file
    : > "$checksums_file"

    # Add header
    echo "# ${PACKAGE_NAME} v${VERSION} - SHA256 Checksums" >> "$checksums_file"
    echo "# Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")" >> "$checksums_file"
    echo "" >> "$checksums_file"

    # Calculate checksums for all packages
    for target_name in "${SELECTED_TARGETS[@]}"; do
        local package_name="${PACKAGE_NAME}-${VERSION}-${target_name}"
        local tarball_path="${OUTPUT_DIR}/packages/${package_name}.tar.gz"

        if [[ -f "$tarball_path" ]]; then
            local checksum
            checksum=$(generate_checksum "$tarball_path")
            echo "${checksum}  ${package_name}.tar.gz" >> "$checksums_file"
            log_info "${package_name}.tar.gz: ${checksum}"
        fi
    done

    log_success "Generated checksums: ${checksums_file}"
}

# Generate release manifest
generate_release_manifest() {
    log_step "Generating release manifest..."

    local manifest_path="${OUTPUT_DIR}/release-manifest.yaml"
    local build_date
    build_date=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    cat > "$manifest_path" << EOF
# Kamuy Wallet Release Manifest
# Version: ${VERSION}

release:
  version: "${VERSION}"
  date: ${build_date}
  rust_version: "$(rustc --version | cut -d' ' -f2)"

packages:
EOF

    for target_name in "${SELECTED_TARGETS[@]}"; do
        local rust_target="${TARGET_MAP[$target_name]}"
        local package_name="${PACKAGE_NAME}-${VERSION}-${target_name}"
        local tarball_path="${OUTPUT_DIR}/packages/${package_name}.tar.gz"

        if [[ -f "$tarball_path" ]]; then
            local checksum
            checksum=$(generate_checksum "$tarball_path")
            local size
            size=$(stat -f%z "$tarball_path" 2>/dev/null || stat -c%s "$tarball_path" 2>/dev/null)

            cat >> "$manifest_path" << EOF
  - name: ${package_name}
    target: ${target_name}
    rust_target: ${rust_target}
    file: ${package_name}.tar.gz
    checksum:
      algorithm: sha256
      value: ${checksum}
    size: ${size}
EOF
        fi
    done

    cat >> "$manifest_path" << EOF

binaries:
  cli: ${CLI_BIN}
  steward: ${STEWARD_BIN}

installation:
  extract: "tar -xzf ${PACKAGE_NAME}-${VERSION}-<target>.tar.gz"
  add_to_path: "export PATH=\"\$PATH:\$(pwd)/bin\""

openclaw:
  install_command: "openclaw skill install ./kamuy-wallet"
  skill_directory: "${PROJECT_ROOT}/skills/kamuy-wallet"
EOF

    log_success "Generated release manifest: ${manifest_path}"
}

# Print summary
print_summary() {
    echo ""
    echo "=============================================="
    echo -e "${GREEN}  Release Build Complete!${NC}"
    echo "=============================================="
    echo ""
    echo "Version: ${VERSION}"
    echo "Output: ${OUTPUT_DIR}"
    echo ""
    echo "Packages:"
    for target_name in "${SELECTED_TARGETS[@]}"; do
        local package_name="${PACKAGE_NAME}-${VERSION}-${target_name}"
        local tarball_path="${OUTPUT_DIR}/packages/${package_name}.tar.gz"
        if [[ -f "$tarball_path" ]]; then
            local size
            size=$(stat -f%z "$tarball_path" 2>/dev/null || stat -c%s "$tarball_path" 2>/dev/null)
            # Convert to MB
            local size_mb
            size_mb=$(echo "scale=2; ${size} / 1048576" | bc 2>/dev/null || echo "N/A")
            echo "  - ${package_name}.tar.gz (${size_mb} MB)"
        fi
    done
    echo ""
    echo "Files:"
    echo "  - ${OUTPUT_DIR}/packages/*.tar.gz (release packages)"
    echo "  - ${OUTPUT_DIR}/packages/${PACKAGE_NAME}-${VERSION}-checksums.txt (checksums)"
    echo "  - ${OUTPUT_DIR}/release-manifest.yaml (manifest)"
    echo ""
    echo "Next steps:"
    echo "  1. Verify checksums:"
    echo "     cd ${OUTPUT_DIR}/packages && sha256sum -c ${PACKAGE_NAME}-${VERSION}-checksums.txt"
    echo ""
    echo "  2. Test a package:"
    echo "     tar -xzf ${OUTPUT_DIR}/packages/${PACKAGE_NAME}-${VERSION}-linux-x64.tar.gz"
    echo "     ./${PACKAGE_NAME}-${VERSION}-linux-x64/bin/${CLI_BIN} --version"
    echo ""
    echo "  3. Upload to release:"
    echo "     gh release create v${VERSION} ${OUTPUT_DIR}/packages/*.tar.gz"
    echo ""
}

# Main function
main() {
    print_banner
    parse_args "$@"
    check_dependencies
    create_output_dirs

    # Build for each selected target
    for target_name in "${SELECTED_TARGETS[@]}"; do
        build_target "$target_name"
        create_package "$target_name"
    done

    # Generate checksums (if not skipped)
    if [[ "$SKIP_CHECKSUM" == false ]]; then
        generate_checksums
    fi

    generate_release_manifest
    print_summary
}

# Run main
main "$@"