#!/usr/bin/env bash
set -euo pipefail

# Usage: ./update-packages.sh <version>
# Called by release workflow after assets are uploaded

VERSION="${1:-}"
[[ -z "$VERSION" ]] && { echo "Usage: $0 <version>"; exit 1; }

# Strip leading 'v' if present
VERSION="${VERSION#v}"

REPO="johnzfitch/llmx"
RELEASE_URL="https://github.com/$REPO/releases/download/v$VERSION"

info() { printf "\033[1;34m==>\033[0m %s\n" "$*"; }
error() { printf "\033[1;31merror:\033[0m %s\n" "$*" >&2; exit 1; }

# Fetch SHA256 for a release asset
get_sha256() {
    local asset="$1"
    curl -fsSL "$RELEASE_URL/$asset.sha256" | awk '{print $1}'
}

info "Fetching checksums for v$VERSION..."

SHA_LINUX_X86_64=$(get_sha256 "llmx-v$VERSION-linux-x86_64.tar.gz")
SHA_LINUX_AARCH64=$(get_sha256 "llmx-v$VERSION-linux-aarch64.tar.gz")
SHA_MACOS_ARM64=$(get_sha256 "llmx-v$VERSION-macos-arm64.tar.gz")
SHA_MACOS_X86_64=$(get_sha256 "llmx-v$VERSION-macos-x86_64.tar.gz")

info "Updating Homebrew formula..."
sed -e "s/VERSION_PLACEHOLDER/$VERSION/g" \
    -e "s/SHA256_LINUX_X86_64_PLACEHOLDER/$SHA_LINUX_X86_64/g" \
    -e "s/SHA256_LINUX_AARCH64_PLACEHOLDER/$SHA_LINUX_AARCH64/g" \
    -e "s/SHA256_MACOS_ARM64_PLACEHOLDER/$SHA_MACOS_ARM64/g" \
    -e "s/SHA256_MACOS_X86_64_PLACEHOLDER/$SHA_MACOS_X86_64/g" \
    pkg/homebrew/llmx.rb > /tmp/llmx.rb

info "Updating AUR PKGBUILD..."
sed -e "s/VERSION_PLACEHOLDER/$VERSION/g" \
    -e "s/SHA256_LINUX_X86_64_PLACEHOLDER/$SHA_LINUX_X86_64/g" \
    -e "s/SHA256_LINUX_AARCH64_PLACEHOLDER/$SHA_LINUX_AARCH64/g" \
    pkg/aur/PKGBUILD > /tmp/PKGBUILD

# Output paths for workflow to use
echo "homebrew_formula=/tmp/llmx.rb"
echo "aur_pkgbuild=/tmp/PKGBUILD"

info "Done. Generated files:"
echo "  /tmp/llmx.rb"
echo "  /tmp/PKGBUILD"
