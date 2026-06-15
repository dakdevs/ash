# Changesets

Changesets record user-visible release intent.

Create one before opening a pull request with user-facing behavior, install, config, release, or documentation changes:

```sh
npm run changeset
```

Maintainers should use the generated Version Packages pull request to update `package.json`, `CHANGELOG.md`, `Cargo.toml`, and `Cargo.lock`.
