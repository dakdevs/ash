# Versioning and Releases

ASH uses semantic versioning and Changesets.

## Version Policy

- `patch`: bug fixes, documentation fixes, small compatible improvements.
- `minor`: new compatible shell, agent, provider, or plugin capabilities.
- `major`: breaking changes to CLI behavior, `.ashrc` syntax, plugin APIs, provider APIs, or persisted storage.

Before `1.0.0`, minor versions may include breaking changes, but they must be called out clearly in the changelog.

## Contributor Flow

For user-visible changes:

```sh
bun run changeset
```

Commit the generated `.changeset/*.md` file with your pull request.

## Maintainer Version Flow

1. Merge feature and fix PRs into `main`.
2. Let the Version Packages workflow create or update the release PR.
3. Review the release PR. It should update:
   - `package.json`
   - `CHANGELOG.md`
   - `Cargo.toml`
   - `Cargo.lock`
4. Merge the release PR.
5. Create and push the release tag:

   ```sh
   bun run release:tag
   ```

6. Confirm the GitHub Release workflow publishes:
   - `ash-linux-x86_64.tar.gz`
   - `ash-macos-aarch64.tar.gz`
   - `ash-macos-x86_64.tar.gz`
   - matching `.sha256` files

## Manual Version Repair

If package and Cargo versions drift:

```sh
bun scripts/sync-cargo-version.mjs
cargo update --workspace
```

Then rerun:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
