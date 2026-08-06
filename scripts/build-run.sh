#!/usr/bin/env bash
# Build the signed .app and launch it. Each run re-signs with the SAME stable identity
# ("Whissper Local"), so macOS keeps the Microphone + Accessibility grants across rebuilds.
set -euo pipefail
cd "$(dirname "$0")/.."

# studio-cache (2026-07-29) redirects all cargo output; resolve the real target dir.
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps --manifest-path src-tauri/Cargo.toml | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
APP="$TARGET_DIR/release/bundle/macos/Ozen.app"

cargo tauri build

# Verify the signature is the stable identity, not ad-hoc (ad-hoc => TCC re-prompts).
echo "--- signature ---"
codesign -dv --verbose=2 "$APP" 2>&1 | grep -E "Authority|Identifier|Signature" || true

# Relaunch cleanly.
pkill -f "Ozen.app/Contents/MacOS" 2>/dev/null || true
open "$APP"
echo "launched: $APP"
