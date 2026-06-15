---
name: release
description: Walks maintainers through the canonical ASH release process. Use when the user invokes /release or asks to prepare, cut, tag, publish, or verify an ASH release from this repository.
---

# ASH Release

Guide the user through a release using the process in `VERSIONING.md`, `CONTRIBUTING.md`, `package.json`, and `scripts/create-release-tag.mjs`.

## Start

When invoked, start by inspecting repo state before asking questions:

```sh
git status --short
git branch --show-current
git fetch --tags origin
git tag --sort=-creatordate | head -10
node -p "require('./package.json').version"
sed -n '1,80p' CHANGELOG.md
```

Then summarize branch, cleanliness, package version, expected tag, tag existence, and whether `CHANGELOG.md` has an entry for that version.

## Questions

Ask only the questions that are not already answered by repo inspection. Prefer one short batch of questions, then proceed.

1. Are we creating the next version PR, reviewing/merging an existing version PR, cutting the tag, or verifying a published release?
2. Is the release PR from the Version Packages workflow already merged into `main`?
3. What version should this release publish?
4. Are there breaking changes that must be called out clearly in the changelog?
5. Should I run the full local verification suite before tagging?
6. Once readiness is proven, do you want me to create and push the tag with `npm run release:tag`?

Do not ask question 6 until the tag target and readiness are clear. Pushing a tag publishes release artifacts, so require explicit confirmation in the user's own words.

## Canonical flows

### Contributor changeset flow

Use this when the release is not ready yet and the current work needs release notes:

1. Confirm the user-visible change and bump type: `patch`, `minor`, or `major`.
2. Run `npm run changeset`.
3. Help the user write concise release-note copy.
4. Remind that the generated `.changeset/*.md` file must be committed with the PR.

### Maintainer version PR flow

Use this when feature/fix PRs are merged into `main` but no version PR has been merged:

1. Confirm the work is on `main` and up to date with `origin/main`.
2. Explain that the Version Packages workflow should create or update the release PR.
3. Review the release PR when available. It should update `package.json`, `CHANGELOG.md`, `Cargo.toml`, and `Cargo.lock`.
4. Check that the changelog calls out breaking changes clearly.
5. After the user confirms the release PR is merged, continue to the tag flow.

### Tag flow

Use this only after the version PR has been merged into `main`.

1. Ensure branch is `main` and synchronized with `origin/main`.
2. Ensure working tree is clean.
3. Confirm `package.json`, `Cargo.toml`, and `Cargo.lock` all use the same ASH package version.
4. Confirm `CHANGELOG.md` has an entry for the version.
5. Confirm local and remote tag `v<version>` do not already exist.
6. Run verification if the user wants local checks, or recommend it strongly before publishing:

   ```sh
   cargo fmt --check
   cargo test
   cargo clippy --all-targets -- -D warnings
   ```

7. Ask for explicit confirmation to create and push `v<version>`.
8. Run `npm run release:tag`.

If versions drift, repair canonically with `node scripts/sync-cargo-version.mjs` and `cargo update --workspace`, then rerun verification before tagging.

### Post-tag verification

After `npm run release:tag`, or when asked to verify a release:

1. Confirm the tag exists locally and on `origin`.
2. Check the GitHub Release workflow for the tag.
3. Confirm the GitHub Release publishes `ash-linux-x86_64.tar.gz`, `ash-macos-aarch64.tar.gz`, `ash-macos-x86_64.tar.gz`, and matching `.sha256` files.

Use GitHub CLI or the GitHub connector if available. If not authenticated, say exactly what local checks were completed and what remains to verify on GitHub.

## Guardrails

- Do not publish from a dirty working tree.
- Do not create or push a tag without explicit confirmation.
- Do not bypass Changesets by editing version files manually unless repairing version drift with the documented commands.
- Do not invent release artifacts; verify the actual workflow output.
- Keep the user oriented around the next irreversible action.
