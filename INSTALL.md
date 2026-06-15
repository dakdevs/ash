# Installing ASH

ASH is pre-1.0. Install it for development and testing before using it as a login shell.

## Agent Install

For automation and coding agents:

```sh
PREFIX="$HOME/.local" ./scripts/install-agent.sh
```

This builds with `cargo build --release --locked`, installs `ash` into `$PREFIX/bin`, and runs a smoke test.

## Manual Install

```sh
cargo build --release --locked
install -d "$HOME/.local/bin"
install -m 0755 target/release/ash "$HOME/.local/bin/ash"
```

Smoke test:

```sh
$HOME/.local/bin/ash --no-ashrc --mode command --eval 'printf ash'
```

## Login Shell Setup

Only do this after confirming the installed binary works and another shell is available.

```sh
command -v ash
sudo sh -c 'command -v ash >> /etc/shells'
chsh -s "$(command -v ash)"
```

If your system already has an `ash` binary, install this project under a user prefix first and avoid replacing system shell binaries.
