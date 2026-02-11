#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
APP_NAME="Mountaineer"
BUNDLE_DIR="$ROOT_DIR/target/release/$APP_NAME.app"

echo "Building release binary..."
cargo build --release

echo "Creating app bundle at $BUNDLE_DIR..."
mkdir -p "$BUNDLE_DIR/Contents/MacOS"
mkdir -p "$BUNDLE_DIR/Contents/Resources"

cp "$ROOT_DIR/target/release/mountaineer" "$BUNDLE_DIR/Contents/MacOS/$APP_NAME"

cat > "$BUNDLE_DIR/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Mountaineer</string>
    <key>CFBundleDisplayName</key>
    <string>Mountaineer</string>
    <key>CFBundleIdentifier</key>
    <string>com.mountaineer.app</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleExecutable</key>
    <string>Mountaineer</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

echo "Done: $BUNDLE_DIR"
