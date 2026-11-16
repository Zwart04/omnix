#!/bin/sh
# Build a Debian package for Omnix from an already-built release binary.
set -eu

APP=omnix
MAINTAINER=${OMNIX_MAINTAINER:-Omnix maintainers <maintainers@omnix.invalid>}
BINARY=
VERSION=${OMNIX_VERSION:-}

die() {
    printf 'build-deb: %s\n' "$1" >&2
    exit 1
}

usage() {
    cat <<'EOF'
usage: build-deb.sh [--binary PATH] [--version VERSION]

  --binary PATH      use PATH instead of target/release/omnix
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

command -v dpkg-deb >/dev/null 2>&1 || die "dpkg-deb not found (install dpkg)"
command -v dpkg >/dev/null 2>&1 || die "dpkg not found"

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

ARCH=$(dpkg --print-architecture)
[ -n "$ARCH" ] || die "dpkg --print-architecture returned nothing"

ICON=$SCRIPT_DIR/omnix.svg
[ -f "$ICON" ] || die "icon not found: $ICON"

STAGE=$(mktemp -d)
chmod 0755 "$STAGE"
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM

install -d "$STAGE/DEBIAN" \
    "$STAGE/usr/bin" \
    "$STAGE/usr/share/applications" \
    "$STAGE/usr/share/icons/hicolor/scalable/apps"
install -m 0755 "$BINARY" "$STAGE/usr/bin/$APP"
install -m 0644 "$ICON" "$STAGE/usr/share/icons/hicolor/scalable/apps/$APP.svg"

cat >"$STAGE/usr/share/applications/$APP.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Omnix
Comment=Worktrees, agent sessions, and a browser in one workbench
Exec=/usr/bin/omnix
Icon=omnix
Terminal=false
Categories=Development;
StartupWMClass=omnix
EOF

SIZE=$(du -sk "$STAGE/usr" | cut -f1)
cat >"$STAGE/DEBIAN/control" <<EOF
Package: $APP
Version: $VERSION
Section: devel
Priority: optional
Architecture: $ARCH
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libsoup-3.0-0
Installed-Size: $SIZE
Maintainer: $MAINTAINER
Description: Light workbench for worktrees, agent sessions, and browsers
 Omnix indexes agent transcripts with SQLite FTS5, sleeps and resumes
 worktree sessions, hands work off to a new worktree, and switches between
 the native WebView and the Lightpanda automation engine.
EOF
chmod 0644 "$STAGE/DEBIAN/control" "$STAGE/usr/share/applications/$APP.desktop"

OUT=$SCRIPT_DIR/${APP}_${VERSION}_${ARCH}.deb
dpkg-deb --root-owner-group --build "$STAGE" "$OUT" >/dev/null

printf 'Built %s\n' "$OUT"
printf 'Next: sudo apt install %s\n' "$OUT"
