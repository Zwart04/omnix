#!/bin/sh
# Install a built Perch binary for the current user, or system-wide with --system.
set -eu

APP=perch
MODE=user
BINARY=

die() {
    printf 'install: %s\n' "$1" >&2
    exit 1
}

usage() {
    cat <<'EOF'
usage: install.sh [--system] [--binary PATH]

  --system      install under /usr/local (needs root or sudo)
  --binary PATH use PATH instead of target/release/perch

Without --system the binary goes to ~/.local/bin/perch.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --system) MODE=system ;;
        --binary)
            [ $# -ge 2 ] || die "--binary needs a path"
            shift
            BINARY=$1
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
[ -x "$BINARY" ] || die "binary is not executable: $BINARY"

ICON=$SCRIPT_DIR/perch.svg
[ -f "$ICON" ] || die "icon not found: $ICON"

if [ "$MODE" = system ]; then
    PREFIX=/usr/local
    BINDIR=$PREFIX/bin
    APPDIR=$PREFIX/share/applications
    ICONDIR=$PREFIX/share/icons/hicolor/scalable/apps
    if [ "$(id -u)" -eq 0 ]; then
        SUDO=
    else
        command -v sudo >/dev/null 2>&1 || die "--system needs root or sudo"
        SUDO=sudo
    fi
    run() {
        if [ -n "$SUDO" ]; then
            sudo "$@"
        else
            "$@"
        fi
    }
else
    [ -n "${HOME:-}" ] || die "HOME is not set"
    BINDIR=$HOME/.local/bin
    APPDIR=$HOME/.local/share/applications
    ICONDIR=$HOME/.local/share/icons/hicolor/scalable/apps
    run() {
        "$@"
    }
fi

run install -d "$BINDIR" "$APPDIR" "$ICONDIR"
run install -m 0755 "$BINARY" "$BINDIR/$APP"
run install -m 0644 "$ICON" "$ICONDIR/$APP.svg"

DESKTOP=$(mktemp)
trap 'rm -f "$DESKTOP"' EXIT HUP INT TERM
cat >"$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=Perch
Comment=Worktrees, agent sessions, and a browser in one workbench
Exec="$BINDIR/$APP"
Icon=$APP
Terminal=false
Categories=Development;
StartupWMClass=perch
EOF
run install -m 0644 "$DESKTOP" "$APPDIR/$APP.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    run update-desktop-database "$APPDIR" >/dev/null 2>&1 || true
fi

printf '\nInstalled %s\n' "$BINDIR/$APP"
if [ "$MODE" = user ]; then
    case ":$PATH:" in
        *":$BINDIR:"*) ;;
        *) printf 'Note: %s is not on PATH. Add this to your shell profile:\n  export PATH="$PATH:%s"\n\n' "$BINDIR" "$BINDIR" ;;
    esac
fi
printf 'Next: %s --help\n' "$BINDIR/$APP"
