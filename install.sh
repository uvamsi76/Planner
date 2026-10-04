#!/usr/bin/env bash
# Build Planner and install it for the current user (no sudo needed):
#   binary  -> ~/.local/bin/planner
#   icon    -> ~/.local/share/icons/hicolor/scalable/apps/
#   launcher-> ~/.local/share/applications/  (shows up in the app grid)
set -euo pipefail
cd "$(dirname "$0")"

APP_ID=dev.vamsi.Planner
BIN_DIR="$HOME/.local/bin"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"

cargo build --release

install -Dm755 target/release/planner "$BIN_DIR/planner"
install -Dm644 "data/$APP_ID.svg" "$DATA_DIR/icons/hicolor/scalable/apps/$APP_ID.svg"
sed "s|@BINDIR@|$BIN_DIR|" "data/$APP_ID.desktop" > "$DATA_DIR/applications/$APP_ID.desktop"
chmod 644 "$DATA_DIR/applications/$APP_ID.desktop"

gtk4-update-icon-cache -qtf "$DATA_DIR/icons/hicolor" 2>/dev/null || true
update-desktop-database -q "$DATA_DIR/applications" 2>/dev/null || true

echo "Installed. Find \"Planner\" in your app grid (you can pin it to the dock)."
echo "Binary: $BIN_DIR/planner ($(du -h "$BIN_DIR/planner" | cut -f1))"
