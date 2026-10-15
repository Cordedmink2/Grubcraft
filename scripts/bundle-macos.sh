#!/usr/bin/env bash
# Build Grubcraft.app as a universal (Apple Silicon + Intel) binary, ad-hoc
# signed for personal distribution, and zip it to target/Grubcraft-macOS.zip.
# First launch on a friend's Mac: right-click the app, choose Open, confirm
# (the zip's "Read me.txt" says so).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="gc_game"))')

export MACOSX_DEPLOYMENT_TARGET=12.0
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release -p gc_game --target "$target"
done

APP=target/Grubcraft.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create \
  target/aarch64-apple-darwin/release/grubcraft \
  target/x86_64-apple-darwin/release/grubcraft \
  -output "$APP/Contents/MacOS/grubcraft"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Grubcraft</string>
  <key>CFBundleDisplayName</key><string>Grubcraft</string>
  <key>CFBundleIdentifier</key><string>com.mariothapliyal.grubcraft</string>
  <key>CFBundleExecutable</key><string>grubcraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.strategy-games</string>
  <!-- LAN play (M2): macOS asks once for local-network permission. -->
  <key>NSLocalNetworkUsageDescription</key>
  <string>Grubcraft finds and joins games hosted by friends on your network.</string>
  <key>NSBonjourServices</key><array><string>_grubcraft._udp</string></array>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP"
# The zip holds the app and a read-me side by side.
STAGE=target/macos-package
rm -rf "$STAGE"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/Grubcraft.app"
cp packaging/readme-macos.txt "$STAGE/Read me.txt"
(cd "$STAGE" && rm -f ../Grubcraft-macOS.zip && ditto -c -k . ../Grubcraft-macOS.zip)
echo "Built target/Grubcraft-macOS.zip ($(lipo -archs "$APP/Contents/MacOS/grubcraft"))"
