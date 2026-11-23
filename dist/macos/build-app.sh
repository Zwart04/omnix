#!/bin/sh
# Build a Omnix.app bundle around the release binary, then tar it up.
set -eu

APP_NAME=Omnix
BUNDLE_ID=dev.omnix.app
BINARY=
VERSION=${OMNIX_VERSION:-}

die() {
    printf 'build-app: %s\n' "$1" >&2
    exit 1
}

usage() {
    cat <<'EOF'
usage: build-app.sh [--binary PATH] [--version VERSION]

  --binary PATH      use PATH instead of target/release/omnix
  --version VERSION  override the version read from Cargo.toml

Env: OMNIX_ARCH=arm64|x64 appends the architecture to the tarball name.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --binary)
            [ $# -ge 2 ] || die "--binary needs a path"
            shift
            BINARY=$1
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

if [ -z "$BINARY" ]; then
    BINARY=$ROOT_DIR/target/release/omnix
fi
[ -f "$BINARY" ] || die "binary not found: $BINARY (build it with: cargo build --release)"

if [ -z "$VERSION" ]; then
    VERSION=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1)
fi
[ -n "$VERSION" ] || die "could not determine version; pass --version"

BUILD_DIR=$SCRIPT_DIR/build
APP=$BUILD_DIR/$APP_NAME.app
SUFFIX=${OMNIX_ARCH:+-$OMNIX_ARCH}
OUT=$SCRIPT_DIR/$APP_NAME-$VERSION-macos$SUFFIX.tar.gz

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

rm -f "$OUT"
(cd "$BUILD_DIR" && tar -czf "$OUT" "$APP_NAME.app")

printf 'Built %s\n' "$OUT"
printf 'Next: open %s/Contents/MacOS/omnix (unsigned; Gatekeeper may block it)\n' "$APP"
