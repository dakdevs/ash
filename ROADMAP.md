# Roadmap

ASH is built in staged milestones.

## Milestone 1: Usable Agent Shell Foundation

- Agent and command prompt modes.
- Native simple command execution.
- `.ashrc` startup declarations.
- SQLite context store.
- Provider, plugin, permission, parser, evaluator, and session boundaries.

## Milestone 2: Interactive Shell Depth

- Pipelines and redirects.
- Process groups and job control.
- Ctrl-C/Ctrl-Z handling.
- Foreground/background jobs.
- Richer prompt/status rendering.

## Milestone 3: Agent Runtime

- Streaming provider responses.
- Tool-call UI.
- Context compaction.
- Codex subscription integration hardening.
- OpenAI, OpenRouter, Vercel AI Gateway, Anthropic, Ollama, and compatible provider adapters.

## Milestone 4: Plugin Runtime

- Wasm plugin execution.
- Process plugin execution.
- Capability enforcement.
- Plugin lockfile and registry/Git/local install flows.
- Hook coverage for shell, agent, provider, prompt, compaction, and permissions.

## Milestone 5: Compatibility

- POSIX/Bash/Zsh differential tests.
- Shell functions, loops, conditionals, here-docs, arrays, traps, and script compatibility.
