#!/usr/bin/env bash
set -euo pipefail

echo "Building pane in release mode..."
cargo build --release

echo "Installing pane to ~/.cargo/bin..."
cp target/release/pane ~/.cargo/bin/pane

echo "Done! pane installed to ~/.cargo/bin/pane"
