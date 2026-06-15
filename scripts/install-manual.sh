#!/usr/bin/env sh
set -eu

prefix="${PREFIX:-$HOME/.local}"
bindir="$prefix/bin"

echo "Installing ASH to $bindir"
cargo build --release --locked
mkdir -p "$bindir"
install -m 0755 target/release/ash "$bindir/ash"

echo
echo "Installed: $bindir/ash"
echo "Run this smoke test:"
echo "  $bindir/ash --no-ashrc --mode command --eval 'printf ash'"
echo
echo "To make ASH your login shell later, add it to /etc/shells and run chsh manually."
