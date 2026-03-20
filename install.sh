#!/bin/bash
# Kamuy Wallet Installer
# Usage: curl -sSL https://raw.githubusercontent.com/KristianRadev/KamuyWallet/main/install.sh | bash

set -e

REPO="KristianRadev/KamuyWallet"
INSTALL_DIR="$HOME/.kamuy"
BIN_DIR="$HOME/.local/bin"

echo "🔐 Installing Kamuy Wallet..."

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

# Download binaries from latest release
LATEST_RELEASE="https://github.com/$REPO/releases/latest/download"

echo "📥 Downloading kamuy CLI..."
curl -sSL "$LATEST_RELEASE/kamuy-$OS-$ARCH" -o "$INSTALL_DIR/kamuy"
chmod +x "$INSTALL_DIR/kamuy"

echo "📥 Downloading kamuy-steward..."
curl -sSL "$LATEST_RELEASE/kamuy-steward-$OS-$ARCH" -o "$INSTALL_DIR/kamuy-steward"
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
curl -sSL "https://raw.githubusercontent.com/$REPO/main/pimlico.json" -o "$INSTALL_DIR/pimlico.json" 2>/dev/null || true

echo ""
echo "✅ Kamuy Wallet installed to $INSTALL_DIR"
echo ""
echo "🚀 Quick Start:"
echo "   1. Set your API key: export STEWARD_API_KEY=\"your-secret-key\""
echo "   2. Create wallet: kamuy init --email your@email.com"
echo "   3. Unlock wallet: kamuy unlock"
echo ""
echo "📖 Documentation: https://github.com/$REPO#readme"