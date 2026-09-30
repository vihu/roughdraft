#!/usr/bin/env bash
# Builds target/dist/roughdraft-<version>-<arch>.AppImage for this machine's
# architecture (x86_64 or aarch64).
#
# Downloads appimagetool and the AppImage runtime, pinned and checked
# below, into target/appimage on first use. The app links only glibc
# (everything else is loaded at run time), so the AppImage bundles nothing
# else and runs on any distribution with a glibc at least as new as the
# build machine's.
set -euo pipefail
cd "$(dirname "$0")/../.."

id=io.github.vihu.roughdraft
arch=$(uname -m)
version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml)
tool_version=1.9.1
runtime_version=20251108
case $arch in
x86_64)
  tool_sha256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
  runtime_sha256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
  ;;
aarch64)
  tool_sha256=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
  runtime_sha256=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
  ;;
*)
  echo "no AppImage tooling for $arch" >&2
  exit 1
  ;;
esac

# Downloads $2 to $1 unless it is already there, then checks it against $3.
fetch() {
  if [ ! -f "$1" ]; then
    curl -fsSL -o "$1.part" "$2"
    mv "$1.part" "$1"
  fi
  echo "$3  $1" | sha256sum --check --quiet
}

cache=target/appimage
mkdir -p "$cache"
tool=$cache/appimagetool-$tool_version-$arch.AppImage
runtime=$cache/runtime-$runtime_version-$arch
fetch "$tool" "https://github.com/AppImage/appimagetool/releases/download/$tool_version/appimagetool-$arch.AppImage" "$tool_sha256"
fetch "$runtime" "https://github.com/AppImage/type2-runtime/releases/download/$runtime_version/runtime-$arch" "$runtime_sha256"
chmod +x "$tool"

cargo build --release --locked -p roughdraft-app

appdir=$(mktemp -d)/roughdraft.AppDir
install -Dm755 target/release/roughdraft "$appdir/usr/bin/roughdraft"
install -Dm644 "app/packaging/$id.desktop" "$appdir/usr/share/applications/$id.desktop"
install -Dm644 "app/packaging/$id.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/$id.svg"
# appimagetool looks for the metainfo under its older name.
install -Dm644 "app/packaging/$id.metainfo.xml" "$appdir/usr/share/metainfo/$id.appdata.xml"
ln -s "usr/share/applications/$id.desktop" "$appdir/$id.desktop"
ln -s "usr/share/icons/hicolor/scalable/apps/$id.svg" "$appdir/$id.svg"
ln -s usr/bin/roughdraft "$appdir/AppRun"

mkdir -p target/dist
# Extract-and-run: build machines (CI runners, containers) often lack FUSE.
ARCH=$arch APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --runtime-file "$runtime" \
  "$appdir" "target/dist/roughdraft-$version-$arch.AppImage"
