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
#
# VERIFY THE RELAUNCH. This script used to pkill and then `open` without checking, and on
# 2026-08-06 that `open` failed with LaunchServices -600 while the script reported success - Ozen
# was simply down and nothing said so. `open` exiting 0 is not the same fact as a running process,
# so the process is what gets checked.
pkill -f "Ozen.app/Contents/MacOS" 2>/dev/null || true
if ! open "$APP"; then
    echo "FAILED: open refused to launch $APP" >&2
    echo "  A LaunchServices -600 usually means a stale duplicate bundle claims the bundle id." >&2
    echo "  Check with: lsregister -dump | grep -A3 'ai.orellius.ozen'" >&2
    exit 1
fi

for _ in $(seq 1 20); do
    if pgrep -f "Ozen.app/Contents/MacOS" >/dev/null; then
        echo "launched: $APP (pid $(pgrep -f 'Ozen.app/Contents/MacOS' | head -1))"
        exit 0
    fi
    sleep 0.5
done
echo "FAILED: $APP did not appear as a running process within 10s" >&2
exit 1
