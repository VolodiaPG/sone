#!/usr/bin/env bash
# Usage: ./build-scripts/build/macos.sh
# Output: ./dist/macos/SONE.app and ./dist/macos/SONE_<version>_<arch>.dmg
#
# Builds natively on the Mac that runs it: Docker cannot produce macOS apps.
# The app links to the Homebrew GStreamer in /opt/homebrew (or /usr/local on
# Intel), so it runs only on a Mac that has `brew install gstreamer`.
set -euo pipefail

cd "$(dirname "$0")/../.."

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "ERROR: this script builds the macOS app and must run on macOS."
    exit 1
fi

OUTDIR="dist/macos"
BUNDLE_DIR="src-tauri/target/release/bundle"

echo "=== Building macOS app ==="
echo ""

missing=()
command -v brew > /dev/null || missing+=("Homebrew (https://brew.sh)")
command -v cargo > /dev/null || missing+=("Rust (https://rustup.rs)")
command -v pnpm > /dev/null || missing+=("pnpm (brew install pnpm)")
command -v pkg-config > /dev/null || missing+=("pkg-config (brew install pkg-config)")
if command -v pkg-config > /dev/null && ! pkg-config --exists gstreamer-1.0 gstreamer-app-1.0; then
    missing+=("GStreamer (brew install gstreamer)")
fi
if [[ ${#missing[@]} -gt 0 ]]; then
    echo "ERROR: missing build dependencies:"
    printf '  - %s\n' "${missing[@]}"
    exit 1
fi

echo "GStreamer $(pkg-config --modversion gstreamer-1.0) from $(brew --prefix gstreamer)"
echo ""

pnpm install --frozen-lockfile
pnpm tauri build --bundles app,dmg

rm -rf "$OUTDIR"
mkdir -p "$OUTDIR"
cp -R "$BUNDLE_DIR/macos/SONE.app" "$OUTDIR/"
cp "$BUNDLE_DIR"/dmg/SONE_*.dmg "$OUTDIR/"

# Post-build check: the binary must link to GStreamer.
echo ""
BIN="$OUTDIR/SONE.app/Contents/MacOS/sone"
if [[ ! -x "$BIN" ]]; then
    echo "ERROR: no executable found at $BIN after build."
    exit 1
fi
echo "GStreamer libraries linked:"
otool -L "$BIN" | grep -i gstreamer || {
    echo "ERROR: $BIN does not link to GStreamer."
    exit 1
}

echo ""
echo "=== Build complete ==="
ls -lh "$OUTDIR"
