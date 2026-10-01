#!/bin/bash
# Build fglaunch and fgcmd in release mode and copy them into a directory on PATH.
# Copies rather than links, so `cargo clean` can't break OpenDeck's fgcmd buttons.
#
#   ./install.sh [dest]      (default: ~/flightgear)
set -euo pipefail

REPO="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")"
DEST="${1:-$HOME/flightgear}"

mkdir -p "$DEST"
for crate in fglaunch fgcmd; do
    cargo build --release --quiet --manifest-path "$REPO/$crate/Cargo.toml"
    # Replace links or older copies; install writes a fresh file, so a running copy is unaffected.
    rm -f "$DEST/$crate"
    install -m 755 "$REPO/$crate/target/release/$crate" "$DEST/$crate"
    echo "installed $DEST/$crate"
done
