#!/usr/bin/env bash
set -euo pipefail

REPO="johnzfitch/llmx"
BINARY_NAME="llmx-mcp"

# Platform detection
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS-$ARCH" in
    Linux-x86_64)   PLATFORM="linux-x86_64" ;;
    Linux-aarch64)  PLATFORM="linux-aarch64" ;;
    Darwin-x86_64)  PLATFORM="macos-x86_64" ;;
    Darwin-arm64)   PLATFORM="macos-arm64" ;;
    MINGW*|MSYS*)   PLATFORM="windows-x86_64" ;;
    *) PLATFORM="" ;;
esac

# Install directory (platform-specific defaults)
get_install_dir() {
    case "$OS" in
        Linux)
            # XDG spec: prefer XDG_BIN_HOME, fallback to ~/.local/bin
            echo "${XDG_BIN_HOME:-$HOME/.local/bin}"
            ;;
        Darwin)
            # macOS: ~/.local/bin (user-local, no sudo needed)
            echo "$HOME/.local/bin"
            ;;
        MINGW*|MSYS*)
            # Windows Git Bash: use cargo bin or user local
            echo "${CARGO_HOME:-$HOME/.cargo}/bin"
            ;;
        *)
            echo "${CARGO_HOME:-$HOME/.cargo}/bin"
            ;;
    esac
}

INSTALL_DIR="${INSTALL_DIR:-$(get_install_dir)}"

info()  { printf "\033[1;34m==>\033[0m %s\n" "$*"; }
warn()  { printf "\033[1;33mwarn:\033[0m %s\n" "$*"; }
error() { printf "\033[1;31merror:\033[0m %s\n" "$*" >&2; exit 1; }

try_binary() {
    [[ -z "$PLATFORM" ]] && return 1

    info "Checking for pre-built binary ($PLATFORM)..."

    LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null |
             grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/') || return 1

    [[ -z "$LATEST" ]] && return 1

    ARCHIVE_EXT="tar.gz"
    [[ "$PLATFORM" == windows-* ]] && ARCHIVE_EXT="zip"

    ASSET_URL="https://github.com/$REPO/releases/download/$LATEST/llmx-${LATEST}-${PLATFORM}.${ARCHIVE_EXT}"

    if curl -fsSL --head "$ASSET_URL" >/dev/null 2>&1; then
        info "Downloading llmx $LATEST..."
        TMP_DIR=$(mktemp -d)
        trap "rm -rf '$TMP_DIR'" EXIT

        curl -fsSL "$ASSET_URL" -o "$TMP_DIR/archive"

        if [[ "$ARCHIVE_EXT" == "zip" ]]; then
            unzip -q "$TMP_DIR/archive" -d "$TMP_DIR"
        else
            tar -xzf "$TMP_DIR/archive" -C "$TMP_DIR"
        fi

        # Find and install binaries
        find "$TMP_DIR" -name "llmx-mcp" -o -name "llmx" | while read -r bin; do
            cp "$bin" "$INSTALL_DIR/"
            chmod +x "$INSTALL_DIR/$(basename "$bin")"
        done

        return 0
    fi
    return 1
}

build_from_source() {
    info "Building from source..."

    command -v cargo >/dev/null || error "Rust not installed. Get it from https://rustup.rs"

    if [[ -f "ingestor-core/Cargo.toml" ]]; then
        # In repo checkout
        cargo install --locked --path ingestor-core --bin llmx-mcp --features mcp
    else
        # Install from crates.io
        cargo install --locked llmx-mcp
    fi
}

main() {
    mkdir -p "$INSTALL_DIR"

    if [[ "${1:-}" == "--source" ]]; then
        build_from_source
    elif try_binary; then
        :
    else
        info "No pre-built binary available, building from source..."
        build_from_source
    fi

    info "Installed to $INSTALL_DIR"

    # Check PATH
    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
        warn "Add to your PATH:"
        case "$OS" in
            Linux)  echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc" ;;
            Darwin) echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc" ;;
            *)      echo "  export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
        esac
    fi
}

main "$@"
