#!/usr/bin/env bash
# Build the release .app and install it to /Applications, then relaunch.
# Usage: bun run deploy
set -euo pipefail
cd "$(dirname "$0")/.."

APP=MoleSweep.app
SRC=src-tauri/target/release/bundle/macos/$APP
DEST=/Applications/$APP

# Build first so a failed build leaves the running app alone.
bun run tauri build --bundles app

# Quit the running app and wait for it to exit.
if pgrep -fq "/Applications/$APP/"; then
  osascript -e 'quit app "MoleSweep"' || true
  for _ in $(seq 50); do pgrep -fq "/Applications/$APP/" || break; sleep 0.2; done
  pkill -f "/Applications/$APP/" || true
fi

rm -rf "$DEST"
ditto "$SRC" "$DEST"
open "$DEST"
echo "Deployed $DEST"
