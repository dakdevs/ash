#!/usr/bin/env sh
set -eu

prefix="${PREFIX:-$HOME/.local}"
bindir="$prefix/bin"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to install ASH" >&2
  exit 1
fi

cargo build --release --locked
mkdir -p "$bindir"
install -m 0755 target/release/ash "$bindir/ash"

"$bindir/ash" --no-ashrc --mode command --eval 'printf ash' >/dev/null

echo "Installed ash to $bindir/ash"
