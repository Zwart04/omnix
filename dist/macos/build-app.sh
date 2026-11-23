#!/bin/sh
# Build Omnix.app (optionally universal) inside a drag-to-Applications DMG.
set -eu

APP_NAME=Omnix
BUNDLE_ID=dev.omnix.app
BINARY=
BINARY2=
VERSION=${OMNIX_VERSION:-}

die() {
    printf 'build-app: %s\n' "$1" >&2
    exit 1
}

usage() {
    cat <<'EOF'
usage: build-app.sh [--binary PATH] [--binary PATH] [--version VERSION]

  --binary PATH      use PATH instead of target/release/omnix; pass it twice
                     (arm64 then x64) to build one universal DMG
  --version VERSION  override the version read from Cargo.toml

Env: OMNIX_ARCH=arm64|x64 appends the architecture to the DMG name.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --binary)
            [ $# -ge 2 ] || die "--binary needs a path"
            shift
            if [ -z "$BINARY" ]; then
                BINARY=$1
            else
                BINARY2=$1
            fi
            ;;
        --version)
            [ $# -ge 2 ] || die "--version needs a value"
            shift
            VERSION=$1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*) die "unknown option: $1" ;;
        *) BINARY=$1 ;;
    esac
    shift
done

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
BUILD_DIR=$SCRIPT_DIR/build

if [ -z "$BINARY" ]; then
    BINARY=$ROOT_DIR/target/release/omnix
fi
[ -f "$BINARY" ] || die "binary not found: $BINARY (build it with: cargo build --release)"
if [ -n "$BINARY2" ] && [ ! -f "$BINARY2" ]; then
    die "binary not found: $BINARY2"
fi

if [ -z "$VERSION" ]; then
    VERSION=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1)
fi
[ -n "$VERSION" ] || die "could not determine version; pass --version"

SUFFIX=${OMNIX_ARCH:+-$OMNIX_ARCH}
if [ -n "$BINARY2" ]; then
    command -v lipo >/dev/null 2>&1 || die "lipo is required for a universal build"
    install -d "$BUILD_DIR"
    lipo -create "$BINARY" "$BINARY2" -output "$BUILD_DIR/omnix-universal"
    BINARY=$BUILD_DIR/omnix-universal
    SUFFIX=-universal
fi

APP=$BUILD_DIR/$APP_NAME.app
OUT=$SCRIPT_DIR/$APP_NAME-$VERSION-macos$SUFFIX.dmg

rm -rf "$APP"
install -d "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 0755 "$BINARY" "$APP/Contents/MacOS/omnix"
install -m 0644 "$ROOT_DIR/LICENSE" "$APP/Contents/Resources/LICENSE"

# Icon is optional: drop omnix.icns next to this script to include it.
if [ -f "$SCRIPT_DIR/omnix.icns" ]; then
    install -m 0644 "$SCRIPT_DIR/omnix.icns" "$APP/Contents/Resources/Omnix.icns"
fi

cat >"$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleExecutable</key>
    <string>omnix</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>Omnix</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.developer-tools</string>
</dict>
</plist>
EOF

# Ad-hoc signature keeps Gatekeeper from rejecting the bundle outright; it is
# not notarization and will not satisfy Gatekeeper on another Mac.
if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || printf 'warning: ad-hoc codesign failed\n' >&2
fi

# A DMG with an /Applications symlink is the install step: drag Omnix across.
command -v hdiutil >/dev/null 2>&1 || die "hdiutil is required to build the DMG"
DMG_DIR=$BUILD_DIR/dmg
rm -rf "$DMG_DIR"
install -d "$DMG_DIR"
ditto "$APP" "$DMG_DIR/$APP_NAME.app"
ln -s /Applications "$DMG_DIR/Applications"
rm -f "$OUT"
hdiutil create -volname "$APP_NAME" -srcfolder "$DMG_DIR" -ov -format UDZO "$OUT" >/dev/null
rm -rf "$DMG_DIR"

printf 'Built %s\n' "$OUT"
printf 'Next: open the DMG and drag Omnix into Applications\n'
