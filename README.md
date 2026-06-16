# ASH

ASH is the Agentic Shell: a Rust login-shell project where agent prompts are first-class and native shell execution is built intentionally from the ground up.

The project is early, but the foundation is real: ASH has explicit agent/command modes, a native simple-command evaluator, `.ashrc` startup declarations, Diesel-backed SQLite context storage, provider boundaries, permission rules, and plugin contracts.

## Status

ASH is pre-1.0. It is suitable for development and experimentation, not as your daily login shell yet.

Current milestone:

- `>` agent mode and `$` command mode.
- Empty-line Tab input (`\t`) toggles modes.
- Native simple command execution without delegating to another shell.
- Native plugin-shaped statusline segments for pwd, git status, Node version, Rust version, and battery.
- `.ashrc` startup declarations for modes, providers, plugins, and permissions.
- Local SQLite context logging through Diesel typed queries and embedded migrations.
- Provider, plugin, permission, parser, evaluator, and session boundaries.

Next milestones are pipelines, redirects, process groups, job control, richer `.ashrc` execution, plugin runtime execution, provider adapters, compaction, and differential Bash/Zsh/POSIX compatibility tests.

## Install

See [INSTALL.md](INSTALL.md) for the full install guide.

### Agent Install

Use this path for automation, CI, or another coding agent. It is non-interactive and installs into `$PREFIX/bin`, defaulting to `$HOME/.local/bin`.

```sh
PREFIX="$HOME/.local" ./scripts/install-agent.sh
```

Then ensure the install directory is on your `PATH`:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

### Manual Install

Build and install from source:

```sh
cargo build --release --locked
install -d "$HOME/.local/bin"
install -m 0755 target/release/ash "$HOME/.local/bin/ash"
```

Run a smoke test:

```sh
ash --no-ashrc --mode command --eval 'printf ash'
```

To experiment with ASH as a login shell, first add the absolute path to `/etc/shells`, then use `chsh`. Do this only after you have another working shell available.

```sh
command -v ash
sudo sh -c 'command -v ash >> /etc/shells'
chsh -s "$(command -v ash)"
```

The name `ash` has historical meaning on some Unix systems, especially BusyBox/Almquist shell environments. Check `command -v ash` before replacing or installing globally.

## Usage

Evaluate one command-mode line:

```sh
ash --no-ashrc --mode command --eval 'pwd'
```

Evaluate one agent-mode line:

```sh
ash --no-ashrc --eval 'what is the git status?'
```

Interactive mode:

```sh
ash
```

The prototype reads `~/.ashrc` by default. Example:

```ash
set default_mode agent
set command_mode persistent
provider default codex
permission bash "git status*" allow
plugin wasm prompt ~/.config/ash/plugins/prompt.wasm ui prompt
plugin process my-statusline ~/.config/ash/plugins/statusline statusline
```

The interactive prompt renders a native right-side statusline. Built-in statusline segments include `pwd`, `git`, `node`, `rust`, and `battery`; plugins can declare the `statusline` capability to contribute future external segments through the plugin runtime.

### AI Connector Setup

Common provider setup is available through `ash` commands, so users do not need to hand-edit `.ashrc` for normal connector flows.

Codex subscription:

```sh
ash auth codex
ash provider add codex
ash provider default codex
```

API-key providers store environment variable references in `.ashrc`, not secret values:

```sh
export OPENAI_API_KEY="..."
ash provider add openai
ash provider add openrouter
ash provider add anthropic
ash provider add vercel-ai-gateway
ash provider default openai
```

Local providers:

```sh
ash provider add ollama
```

Inspect setup:

```sh
ash provider list
ash provider doctor
```

## Architecture

ASH is split around a small kernel:

- `session`: prompt mode, routing, status, and context recording.
- `config`: `.ashrc` declaration evaluation.
- `shell`: native lexing, parsing, expansion, and execution.
- `permissions`: opencode-style `allow | ask | deny` rules.
- `plugins`: manifests, sources, capabilities, and event types.
- `providers`: AI provider adapter contracts.
- `context`: SQLite-backed shell and agent history using Diesel typed queries.

More detail lives in [docs/architecture.md](docs/architecture.md).

## Development

Required toolchain:

- Rust `1.94.1`
- Node `25.9.0` and npm `11.12.1` for Changesets/release tooling

Install development tooling:

```sh
npm install
```

Run checks:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Add a changeset for user-visible changes:

```sh
npm run changeset
```

## Releases

ASH uses Changesets for version/changelog PRs and GitHub Actions for release artifacts.

- Pull requests with user-visible changes should include a `.changeset/*.md` file.
- Merging changes to `main` runs the Version Packages workflow.
- Maintainers merge the generated version PR.
- Create and push a tag with `npm run release:tag`.
- Pushing `v*` tags builds Linux/macOS release assets and publishes a GitHub Release.

See [VERSIONING.md](VERSIONING.md) for the full versioning and release process.

## Contributing

Contributions are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md), especially the testing, changeset, and release rules.

## License

MIT. See [LICENSE](LICENSE).
