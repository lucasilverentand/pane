#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
REPO_ROOT="$(dirname "$PROJECT_DIR")"

cd "$REPO_ROOT"

echo "==> Initializing submodules..."
git submodule update --init --recursive

echo "==> Checking for zig..."
if ! command -v zig &> /dev/null; then
    echo "Error: zig is not installed."
    echo "Install via: brew install zig"
    exit 1
fi

GHOSTTY_DIR="$PROJECT_DIR/ghostty"
GHOSTTY_SHA="$(git -C "$GHOSTTY_DIR" rev-parse HEAD)"
CACHE_ROOT="${PANE_GHOSTTYKIT_CACHE_DIR:-$HOME/.cache/pane/ghosttykit}"
CACHE_DIR="$CACHE_ROOT/$GHOSTTY_SHA"
CACHE_XCFRAMEWORK="$CACHE_DIR/GhosttyKit.xcframework"
LOCAL_XCFRAMEWORK="$GHOSTTY_DIR/macos/GhosttyKit.xcframework"
LOCAL_SHA_STAMP="$LOCAL_XCFRAMEWORK/.ghostty_sha"
LOCK_DIR="$CACHE_ROOT/$GHOSTTY_SHA.lock"

mkdir -p "$CACHE_ROOT"

echo "==> Ghostty submodule commit: $GHOSTTY_SHA"

# Simple file lock to prevent concurrent builds
LOCK_TIMEOUT=300
LOCK_START=$SECONDS
while ! mkdir "$LOCK_DIR" 2>/dev/null; do
    if (( SECONDS - LOCK_START > LOCK_TIMEOUT )); then
        echo "==> Lock stale (>${LOCK_TIMEOUT}s), removing and retrying..."
        rmdir "$LOCK_DIR" 2>/dev/null || rm -rf "$LOCK_DIR"
        continue
    fi
    echo "==> Waiting for GhosttyKit cache lock for $GHOSTTY_SHA..."
    sleep 1
done
trap 'rmdir "$LOCK_DIR" >/dev/null 2>&1 || true' EXIT

if [ -d "$CACHE_XCFRAMEWORK" ]; then
    echo "==> Reusing cached GhosttyKit.xcframework"
else
    LOCAL_SHA=""
    if [ -f "$LOCAL_SHA_STAMP" ]; then
        LOCAL_SHA="$(cat "$LOCAL_SHA_STAMP")"
    fi

    if [ -d "$LOCAL_XCFRAMEWORK" ] && [ "$LOCAL_SHA" = "$GHOSTTY_SHA" ]; then
        echo "==> Seeding cache from existing local GhosttyKit.xcframework (SHA matches)"
    else
        echo "==> Building GhosttyKit.xcframework (this may take a few minutes)..."
        (
            cd "$GHOSTTY_DIR"
            zig build -Demit-xcframework=true -Dxcframework-target=universal -Doptimize=ReleaseFast
        )
        echo "$GHOSTTY_SHA" > "$LOCAL_SHA_STAMP"
    fi

    if [ ! -d "$LOCAL_XCFRAMEWORK" ]; then
        echo "Error: GhosttyKit.xcframework not found at $LOCAL_XCFRAMEWORK"
        exit 1
    fi

    TMP_DIR="$(mktemp -d "$CACHE_ROOT/.ghosttykit-tmp.XXXXXX")"
    mkdir -p "$CACHE_DIR"
    cp -R "$LOCAL_XCFRAMEWORK" "$TMP_DIR/GhosttyKit.xcframework"
    rm -rf "$CACHE_XCFRAMEWORK"
    mv "$TMP_DIR/GhosttyKit.xcframework" "$CACHE_XCFRAMEWORK"
    rmdir "$TMP_DIR"
    echo "==> Cached GhosttyKit.xcframework at $CACHE_XCFRAMEWORK"
fi

echo "==> Creating symlink for GhosttyKit.xcframework..."
ln -sfn "$CACHE_XCFRAMEWORK" "$PROJECT_DIR/GhosttyKit.xcframework"

# Copy ghostty.h for the bridging header
echo "==> Copying ghostty.h..."
cp "$GHOSTTY_DIR/include/ghostty.h" "$PROJECT_DIR/ghostty.h"

echo "==> Setup complete!"
echo ""
echo "Next steps:"
echo "  cd pane-app && tuist generate"
echo "  xcodebuild -scheme Pane -configuration Debug -destination 'platform=macOS' build"
