# ash

## 0.2.0

### Minor Changes

- [#11](https://github.com/dakdevs/ash/pull/11) [`ba53781`](https://github.com/dakdevs/ash/commit/ba53781532aa95a05bb9863de479c00616e2c74d) Thanks [@dakdevs](https://github.com/dakdevs)! - Add the embedded Anthropic Claude provider, Bun workspace docs site, and a cli-spinners-backed loading indicator for interactive agent turns.

- [#5](https://github.com/dakdevs/ash/pull/5) [`e46bc66`](https://github.com/dakdevs/ash/commit/e46bc660b84447ad157df26bac26017cd04e4810) Thanks [@dakdevs](https://github.com/dakdevs)! - Add first-class AI connector setup commands for Codex, OpenAI-compatible providers, Anthropic, Vercel AI Gateway, OpenRouter, and Ollama.

### Patch Changes

- [#10](https://github.com/dakdevs/ash/pull/10) [`06d9a23`](https://github.com/dakdevs/ash/commit/06d9a2318baadfe920ad5309983d2a22034e6be1) Thanks [@dakdevs](https://github.com/dakdevs)! - Add a native plugin-shaped statusline with built-in pwd, git, Node, Rust, and battery segments.

- [#6](https://github.com/dakdevs/ash/pull/6) [`019dba0`](https://github.com/dakdevs/ash/commit/019dba05e5d7d2c61d743763167cac0ee320f9a0) Thanks [@dakdevs](https://github.com/dakdevs)! - Fix `ash auth codex` discovery when Codex is installed through the macOS app bundle instead of on PATH.

- [#8](https://github.com/dakdevs/ash/pull/8) [`6b01da6`](https://github.com/dakdevs/ash/commit/6b01da626842a1e4aba8c0e1d49cb0f2c7f2f0b1) Thanks [@dakdevs](https://github.com/dakdevs)! - Stream typed Codex JSONL events into the interactive shell, carry recent ASH context into follow-up prompts, and render thinking, tool calls, command output, assistant responses, and token usage as separate minimal TUI sections.

## 0.1.0

### Patch Changes

- Initial agentic shell foundation.
