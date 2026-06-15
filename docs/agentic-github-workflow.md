# Agentic GitHub Workflow

This repository includes GitHub Actions for a small agent work loop:

1. Issue intake adds or updates issues in a GitHub Project and uses Codex with `$grill-with-docs` to decide whether the issue is ready.
2. The scheduled worker periodically selects one `Ready for Agent` issue, moves it to `In Progress`, runs Codex, and opens a draft PR when there are file changes.
3. If Codex needs more information, the workflow comments on the issue and moves it back to `Clarifying`.

## Project Setup

Create or choose a GitHub Project v2 with a single-select field named `Status`.

Default status options:

- `Inbox`
- `Clarifying`
- `Ready for Agent`
- `In Progress`
- `PR Open`
- `Blocked`
- `Done`

The workflows only require the first six values. You can rename them by setting repository variables.

## Repository Variables

Required:

- `AGENT_PROJECT_NUMBER`: the Project v2 number from the project URL.

Optional:

- `AGENT_PROJECT_OWNER`: project owner. Defaults to the repository owner.
- `AGENT_PROJECT_OWNER_TYPE`: `organization`, `user`, or `auto`. Defaults to `auto`.
- `AGENT_PROJECT_STATUS_FIELD`: defaults to `Status`.
- `AGENT_PROJECT_STATUS_INBOX`: defaults to `Inbox`.
- `AGENT_PROJECT_STATUS_CLARIFYING`: defaults to `Clarifying`.
- `AGENT_PROJECT_STATUS_READY`: defaults to `Ready for Agent`.
- `AGENT_PROJECT_STATUS_IN_PROGRESS`: defaults to `In Progress`.
- `AGENT_PROJECT_STATUS_PR_OPEN`: defaults to `PR Open`.
- `AGENT_PROJECT_STATUS_BLOCKED`: defaults to `Blocked`.

## Secrets

Required:

- `OPENAI_API_KEY`: used by `openai/codex-action`.
- `PROJECT_TOKEN`: token used for GitHub Projects GraphQL reads and mutations.

For organization projects, prefer a GitHub App installation token with project access. For user projects, a personal access token with project scope is the practical option. `GITHUB_TOKEN` is not enough for GitHub Projects access.

## GitHub Actions Settings

In repository settings, go to Actions > General and enable workflow permission to create pull requests. The worker uses `GITHUB_TOKEN` to push a branch and open a draft PR.

## Workflow Behavior

`agent-issue-intake.yml` runs when issues are opened, edited, reopened, or when a human comments. Bot comments are ignored to prevent loops.

`agent-project-worker.yml` runs every four hours at minute 17 UTC and can also be started manually. It handles one issue per run so work is easier to audit.

Both workflows use structured JSON output schemas under `.github/codex/schemas/`. The Node helper in `scripts/github-project-agent.mjs` owns project mutations and issue comments; Codex owns triage and code changes.

The skills invoked by the prompts are checked in under `.agents/skills/` so GitHub-hosted runners do not depend on a developer's local Codex home directory.

## Security Notes

Issue text is treated as untrusted input in the prompts. Keep that property when editing the prompts.

Keep `PROJECT_TOKEN` scoped only to the project access needed by this workflow. Keep `OPENAI_API_KEY` available only to the Codex action steps.

The worker opens draft PRs. A human should review, run checks, and merge.
