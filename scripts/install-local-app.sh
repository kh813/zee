#!/usr/bin/env bash
set -euo pipefail

# Prefer rustup toolchain over Homebrew rustc so target/ and sccache stay consistent
if [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

PROFILE="release-fast"
APP_PATH="/Users/hiroshi/Applications/Zee.app"

for arg in "$@"; do
    case "$arg" in
        --full|--release)
            PROFILE="release"
            ;;
        *)
            APP_PATH="$arg"
            ;;
    esac
done

if [ ! -d "$APP_PATH" ]; then
    echo "Error: Application bundle $APP_PATH not found."
    exit 1
fi

if [ -z "${RUSTC_WRAPPER:-}" ] && command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER="$(command -v sccache)"
fi

echo "Building zee-gui (zeeg) with profile '${PROFILE}'..."
cargo build --profile "$PROFILE" -p zee-gui --bin zeeg

BIN_PATH="target/${PROFILE}/zeeg"
echo "Updating $APP_PATH binary from $BIN_PATH..."
cp "$BIN_PATH" "$APP_PATH/Contents/MacOS/zeeg"
chmod +x "$APP_PATH/Contents/MacOS/zeeg"

echo "Stripping quarantine and extended attributes..."
xattr -cr "$APP_PATH" || true

echo "Applying ad-hoc code signature to bundle..."
codesign --force --deep --sign - "$APP_PATH"

echo "Verifying code signature..."
codesign -vvv --deep "$APP_PATH"

echo "✓ Successfully installed and signed Zee.app at $APP_PATH"
