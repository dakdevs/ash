# ASH Architecture

ASH is split around a small kernel:

- `session`: prompt mode, routing, status, and context recording.
- `config`: `.ashrc` declaration evaluation.
- `shell`: native lexing, parsing, expansion, and execution.
- `permissions`: opencode-style `allow | ask | deny` rules.
- `plugins`: manifests, sources, capabilities, and event types.
- `providers`: AI provider adapter contracts.
- `context`: SQLite-backed shell and agent history using Diesel typed queries.

The current shell intentionally supports only simple commands and a small builtin set. The parser, AST, expander, and evaluator are separate so POSIX/Bash/Zsh features can be added without collapsing everything into process spawning code.

Bundled features should use the same event and capability contracts as third-party plugins as those runtimes come online.

Database schema changes live in `migrations/`; Rust code should use Diesel schema modules and query builders rather than hand-written SQL strings for runtime reads and writes.
