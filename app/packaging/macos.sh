#!/usr/bin/env bash
# Builds target/dist/roughdraft-<version>-macos-universal.zip: roughdraft.app
# for Apple silicon and Intel in one bundle, ad-hoc signed.
#
# Runs on macOS with both Rust targets installed (rustup target add
# aarch64-apple-darwin x86_64-apple-darwin) and rsvg-convert for the icon
# (brew install librsvg). Ad-hoc signing seals the bundle: a download then
# gets macOS's "could not verify" prompt, which Privacy & Security > Open
# Anyway clears, instead of being reported as damaged.
set -euo pipefail
cd "$(dirname "$0")/../.."

id=io.github.vihu.roughdraft
version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml)
export MACOSX_DEPLOYMENT_TARGET=11.0

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release --locked -p roughdraft-app --target "$target"
done

work=$(mktemp -d)
app=$work/roughdraft.app
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/roughdraft" \
  target/aarch64-apple-darwin/release/roughdraft \
  target/x86_64-apple-darwin/release/roughdraft

iconset=$work/roughdraft.iconset
mkdir "$iconset"
for size in 16 32 128 256 512; do
  rsvg-convert -w "$size" -h "$size" "app/packaging/$id.svg" \
    -o "$iconset/icon_${size}x${size}.png"
  rsvg-convert -w "$((size * 2))" -h "$((size * 2))" "app/packaging/$id.svg" \
    -o "$iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns -o "$app/Contents/Resources/roughdraft.icns" "$iconset"

cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>roughdraft</string>
  <key>CFBundleExecutable</key><string>roughdraft</string>
  <key>CFBundleIconFile</key><string>roughdraft</string>
  <key>CFBundleIdentifier</key><string>$id</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>roughdraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
  <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
plutil -lint "$app/Contents/Info.plist"

codesign --force --sign - "$app"
codesign --verify --strict "$app"

mkdir -p target/dist
ditto -c -k --keepParent "$app" "target/dist/roughdraft-$version-macos-universal.zip"
