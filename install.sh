#!/bin/sh
# Nodalync Install Script
# Usage: curl -fsSL https://raw.githubusercontent.com/gdgiangi/nodalync-protocol/main/install.sh | sh
#
# This script detects your platform and installs the latest nodalync binary.

set -e

REPO="gdgiangi/nodalync-protocol"
BINARY_NAME="nodalync"
INSTALL_DIR="/usr/local/bin"

# Colors (if terminal supports them)
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() {
    printf "${BLUE}[INFO]${NC} %s\n" "$1"
}

success() {
    printf "${GREEN}[OK]${NC} %s\n" "$1"
}

warn() {
    printf "${YELLOW}[WARN]${NC} %s\n" "$1"
}

error() {
    printf "${RED}[ERROR]${NC} %s\n" "$1"
    exit 1
}

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64)
                    PLATFORM="x86_64-unknown-linux-gnu"
                    ;;
                aarch64|arm64)
                    error "Linux ARM64 binaries not yet available. Please build from source."
                    ;;
                *)
                    error "Unsupported architecture: $ARCH"
                    ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                x86_64)
                    PLATFORM="x86_64-apple-darwin"
                    ;;
                arm64)
                    PLATFORM="aarch64-apple-darwin"
                    ;;
                *)
                    error "Unsupported architecture: $ARCH"
                    ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*)
            PLATFORM="x86_64-pc-windows-msvc"
            BINARY_NAME="nodalync.exe"
            ;;
        *)
            error "Unsupported OS: $OS"
            ;;
    esac

    info "Detected platform: $PLATFORM"
}

# Get latest release version
get_latest_version() {
    info "Fetching latest release..."

    # Try to get the latest CLI release (v* tags, not protocol-v* tags)
    if command -v curl >/dev/null 2>&1; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases" | \
            grep -o '"tag_name": *"v[^"]*"' | \
            head -1 | \
            sed 's/"tag_name": *"//;s/"//')
    elif command -v wget >/dev/null 2>&1; then
        VERSION=$(wget -qO- "https://api.github.com/repos/$REPO/releases" | \
            grep -o '"tag_name": *"v[^"]*"' | \
            head -1 | \
            sed 's/"tag_name": *"//;s/"//')
    else
        error "Neither curl nor wget found. Please install one of them."
    fi

    if [ -z "$VERSION" ]; then
        error "Could not determine latest version"
    fi

    info "Latest version: $VERSION"
}

# Download and install
install() {
    ARCHIVE_EXT="tar.gz"
    if [ "$OS" = "MINGW"* ] || [ "$OS" = "MSYS"* ] || [ "$OS" = "CYGWIN"* ]; then
        ARCHIVE_EXT="zip"
    fi

    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$BINARY_NAME-$PLATFORM.$ARCHIVE_EXT"

    info "Downloading from: $DOWNLOAD_URL"

    # Create temp directory
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT

    # Download
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/nodalync.$ARCHIVE_EXT"
    else
        wget -q "$DOWNLOAD_URL" -O "$TMP_DIR/nodalync.$ARCHIVE_EXT"
    fi

    # Extract
    cd "$TMP_DIR"
    if [ "$ARCHIVE_EXT" = "zip" ]; then
        unzip -q "nodalync.$ARCHIVE_EXT"
    else
        tar -xzf "nodalync.$ARCHIVE_EXT"
    fi

    # Install
    if [ -w "$INSTALL_DIR" ]; then
        mv "$BINARY_NAME" "$INSTALL_DIR/"
        chmod +x "$INSTALL_DIR/$BINARY_NAME"
    else
        info "Need sudo to install to $INSTALL_DIR"
        sudo mv "$BINARY_NAME" "$INSTALL_DIR/"
        sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
    fi

    success "Installed $BINARY_NAME to $INSTALL_DIR/$BINARY_NAME"
}

# Verify installation
verify() {
    if command -v nodalync >/dev/null 2>&1; then
        INSTALLED_VERSION=$(nodalync --version 2>/dev/null || echo "unknown")
        success "nodalync $INSTALLED_VERSION is ready!"
        
        # Check for duplicate binaries in PATH
        NODALYNC_PATH=$(command -v nodalync)
        if [ "$NODALYNC_PATH" != "$INSTALL_DIR/$BINARY_NAME" ]; then
            warn "Found nodalync at $NODALYNC_PATH (not $INSTALL_DIR/$BINARY_NAME)"
            warn "You may have an old version. Remove it with: rm $NODALYNC_PATH"
        fi
    else
        warn "Installation complete, but 'nodalync' not found in PATH"
        warn "You may need to add $INSTALL_DIR to your PATH"
    fi
}

# Print next steps
next_steps() {
    echo ""
    printf "${GREEN}Installation complete!${NC}\n"
    echo ""
    echo "Next steps:"
    echo ""
    echo "  1. Initialize your identity:"
    echo "     export NODALYNC_PASSWORD=\"your-secure-password\""
    echo "     nodalync init --wizard"
    echo ""
    echo "  2. Start your node:"
    echo "     nodalync start"
    echo ""
    echo "  3. Publish content:"
    echo "     nodalync publish my-document.md --title \"My Knowledge\""
    echo ""
    echo "Documentation: https://github.com/$REPO#readme"
    echo ""
}

# Main
main() {
    echo ""
    printf "${BLUE}Nodalync Installer${NC}\n"
    echo "===================="
    echo ""

    detect_platform
    get_latest_version
    install
    verify
    next_steps
}

main
