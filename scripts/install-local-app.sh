#!/usr/bin/env bash
set -euo pipefail

APP_PATH="${1:-/Users/hiroshi/Applications/Zee.app}"

if [ ! -d "$APP_PATH" ]; then
    echo "Error: Application bundle $APP_PATH not found."
    exit 1
fi

echo "Building release binary for zee-gui (zeeg)..."
cargo build --release -p zee-gui --bin zeeg

echo "Updating $APP_PATH binary..."
cp target/release/zeeg "$APP_PATH/Contents/MacOS/zeeg"
chmod +x "$APP_PATH/Contents/MacOS/zeeg"

echo "Stripping quarantine and extended attributes..."
xattr -cr "$APP_PATH" || true

echo "Applying ad-hoc code signature to bundle..."
codesign --force --deep --sign - "$APP_PATH"

echo "Verifying code signature..."
codesign -vvv --deep "$APP_PATH"

echo "✓ Successfully installed and signed Zee.app at $APP_PATH"
