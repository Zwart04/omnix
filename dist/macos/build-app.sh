#!/bin/sh
# Build a Perch.app bundle around the release binary, then tar it up.
set -eu

APP_NAME=Perch
BUNDLE_ID=dev.perch.app
BINARY=
VERSION=${PERCH_VERSION:-}

die() {
    printf 'build-app: %s\n' "$1" >&2
    exit 1
}

usage() {
    cat <<'EOF'
usage: build-app.sh [--binary PATH] [--version VERSION]

  --binary PATH      use PATH instead of target/release/perch
  --version VERSION  override the version read from Cargo.toml
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
    BINARY=$ROOT_DIR/target/release/perch
fi
[ -f "$BINARY" ] || die "binary not found: $BINARY (build it with: cargo build --release)"

if [ -z "$VERSION" ]; then
    VERSION=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1)
fi
[ -n "$VERSION" ] || die "could not determine version; pass --version"

BUILD_DIR=$SCRIPT_DIR/build
APP=$BUILD_DIR/$APP_NAME.app
OUT=$SCRIPT_DIR/$APP_NAME-$VERSION-macos.tar.gz

rm -rf "$APP"
install -d "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 0755 "$BINARY" "$APP/Contents/MacOS/perch"

# Icon is optional: drop perch.icns next to this script to include it.
if [ -f "$SCRIPT_DIR/perch.icns" ]; then
    install -m 0644 "$SCRIPT_DIR/perch.icns" "$APP/Contents/Resources/Perch.icns"
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
    <string>perch</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>Perch</string>
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
printf 'Next: open %s/Contents/MacOS/perch (unsigned; Gatekeeper may block it)\n' "$APP"
