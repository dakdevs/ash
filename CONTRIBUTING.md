# Contributing to ASH

Thanks for helping build ASH. This project is intentionally early, so high-quality contributions should preserve clear boundaries and leave the shell easier to reason about than before.

## Development Setup

Install the pinned toolchains:

- Rust `1.94.1`
- Bun `1.3.14`

Install workspace tooling:

```sh
bun install --frozen-lockfile
```

Run the Rust checks before opening a pull request:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

## TDD Expectations

Use a vertical red/green/refactor loop:

1. Add one focused test for observable behavior.
2. Implement the smallest change that makes it pass.
3. Refactor with the test suite green.

Prefer tests through public interfaces such as `AshSession`, `ShellExecutor`, `ContextStore`, and the CLI binary. Avoid tests that only lock down private implementation details.

## Code Style

- Keep modules small and responsibility-focused.
- Keep `main.rs` thin; behavior belongs in the library.
- Do not delegate command-mode execution to another shell.
- Store runtime database access behind typed Diesel schema/query builders.
- Keep raw SQL in migrations, not in ordinary Rust query code.
- Use typed domain concepts instead of passing unstructured strings across major boundaries.
- Avoid `unwrap`/`expect` outside tests unless the invariant is obvious and unrecoverable.

## Changesets

User-visible changes need a changeset:

```sh
bun run changeset
```

Choose:

- `patch` for fixes and small compatible improvements.
- `minor` for new compatible capabilities.
- `major` for breaking changes.

The Changesets Version Packages workflow updates `package.json`, `CHANGELOG.md`, `Cargo.toml`, and `Cargo.lock`.

## Pull Request Checklist

- Tests cover the behavior being changed.
- `cargo fmt --check` passes.
- `cargo test` passes.
- `cargo clippy --all-targets -- -D warnings` passes.
- User-visible changes include a changeset.
- Docs are updated when commands, config, install flow, or release behavior changes.

## Release Process

Maintainers:

1. Merge feature PRs into `main`.
2. Let the Version Packages workflow open or update the release PR.
3. Review and merge the release PR.
4. Create and push the version tag:

   ```sh
   bun run release:tag
   ```

5. Confirm the GitHub Release workflow publishes Linux/macOS assets.
