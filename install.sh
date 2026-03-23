#!/bin/bash
# Kamuy Wallet Installer
#
# Usage:
#   1. Download: curl -sSL https://raw.githubusercontent.com/KristianRadev/KamuyWallet/master/install.sh -o install.sh
#   2. Review:   cat install.sh
#   3. Run:      bash install.sh

set -e

REPO="KristianRadev/KamuyWallet"
INSTALL_DIR="$HOME/.kamuy"
BIN_DIR="$HOME/.local/bin"

echo "🔐 Installing Kamuy Wallet..."

# Check for GitHub token (required for private repos)
if [ -n "$GITHUB_TOKEN" ]; then
    AUTH_HEADER="-H \"Authorization: token $GITHUB_TOKEN\""
    CURL_AUTH="-H \"Authorization: token $GITHUB_TOKEN\""
    echo "   Using authenticated access (private repo)"
else
    AUTH_HEADER=""
    CURL_AUTH=""
    echo "   Using public access"
fi

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case $ARCH in
    x86_64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) echo "❌ Unsupported architecture: $ARCH"; exit 1 ;;
esac

case $OS in
    linux) OS="linux" ;;
    darwin) OS="macos" ;;
    *) echo "❌ Unsupported OS: $OS"; exit 1 ;;
esac

# Create directories
mkdir -p "$INSTALL_DIR"
mkdir -p "$BIN_DIR"

# Build download URL with auth if needed
download_file() {
    local url="$1"
    local output="$2"

    if [ -n "$GITHUB_TOKEN" ]; then
        curl -sSL -H "Authorization: token $GITHUB_TOKEN" "$url" -o "$output"
    else
        curl -sSL "$url" -o "$output"
    fi
}

# Fetch latest release version from GitHub API
echo "🔍 Fetching latest release..."
if [ -n "$GITHUB_TOKEN" ]; then
    LATEST_VERSION=$(curl -sSL -H "Authorization: token $GITHUB_TOKEN" \
        "https://api.github.com/repos/$REPO/releases/latest" | \
        grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
else
    LATEST_VERSION=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" | \
        grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
fi

if [ -z "$LATEST_VERSION" ]; then
    echo "❌ Failed to fetch latest release version"
    exit 1
fi

echo "   Latest version: $LATEST_VERSION"

# Download binaries from release
RELEASE_URL="https://github.com/$REPO/releases/download/$LATEST_VERSION"

echo "📥 Downloading kamuy CLI..."
download_file "$RELEASE_URL/kamuy" "$INSTALL_DIR/kamuy"
chmod +x "$INSTALL_DIR/kamuy"

echo "📥 Downloading kamuy-steward..."
download_file "$RELEASE_URL/kamuy-steward" "$INSTALL_DIR/kamuy-steward"
chmod +x "$INSTALL_DIR/kamuy-steward"

# Create symlinks in bin
ln -sf "$INSTALL_DIR/kamuy" "$BIN_DIR/kamuy"
ln -sf "$INSTALL_DIR/kamuy-steward" "$BIN_DIR/kamuy-steward"

# Add to PATH if needed
if ! echo "$PATH" | grep -q "$BIN_DIR"; then
    echo ""
    echo "⚠️  Add this to your ~/.bashrc or ~/.zshrc:"
    echo "   export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
    echo "Then run: source ~/.bashrc  (or source ~/.zshrc)"
fi

# Download default config
if [ -n "$GITHUB_TOKEN" ]; then
    curl -sSL -H "Authorization: token $GITHUB_TOKEN" "https://raw.githubusercontent.com/$REPO/main/pimlico.json" -o "$INSTALL_DIR/pimlico.json" 2>/dev/null || true
else
    curl -sSL "https://raw.githubusercontent.com/$REPO/main/pimlico.json" -o "$INSTALL_DIR/pimlico.json" 2>/dev/null || true
fi

echo ""
echo "✅ Kamuy Wallet installed to $INSTALL_DIR"
echo ""
echo "🚀 Quick Start:"
echo "   kamuy init --email your@email.com"
echo ""
echo "   That's it! Your wallet will be created, Steward started, and unlocked."
echo ""
echo "📖 Documentation: https://github.com/$REPO#readme"