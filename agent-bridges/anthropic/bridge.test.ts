import { expect, test } from "bun:test";
import { normalizeMessage } from "./bridge";

test("assistant text maps to assistant event", () => {
  expect(
    normalizeMessage({
      type: "assistant",
      message: {
        content: [{ type: "text", text: "hello" }],
      },
    }),
  ).toEqual([{ type: "assistant_text", text: "hello" }]);
});

test("tool use and tool result map to ASH tool events", () => {
  expect(
    normalizeMessage({
      type: "assistant",
      message: {
        content: [
          {
            type: "tool_use",
            name: "Bash",
            input: { command: "git status --short" },
          },
        ],
      },
    }),
  ).toEqual([{ type: "tool_started", command: "git status --short" }]);

  expect(
    normalizeMessage({
      type: "user",
      message: {
        content: [
          {
            type: "tool_result",
            content: " M src/main.rs\n",
          },
        ],
      },
    }),
  ).toEqual([
    { type: "tool_output", output: " M src/main.rs\n" },
    { type: "tool_completed", exit_code: null },
  ]);
});

test("result usage maps to ASH usage and result events", () => {
  expect(
    normalizeMessage({
      type: "result",
      subtype: "success",
      result: "done",
      usage: {
        input_tokens: 12,
        output_tokens: 3,
        cache_read_input_tokens: 4,
        cache_creation_input_tokens: 5,
      },
    }),
  ).toEqual([
    {
      type: "usage",
      input_tokens: 12,
      output_tokens: 3,
      cache_read_input_tokens: 4,
      cache_creation_input_tokens: 5,
    },
    { type: "result", text: "done" },
  ]);
});

test("assistant usage is ignored because result usage is authoritative", () => {
  expect(
    normalizeMessage({
      type: "assistant",
      message: {
        content: [{ type: "text", text: "hello" }],
        usage: {
          input_tokens: 12,
          output_tokens: 3,
        },
      },
    }),
  ).toEqual([{ type: "assistant_text", text: "hello" }]);
});

test("result errors map to structured error events", () => {
  expect(
    normalizeMessage({
      type: "result",
      subtype: "error_during_execution",
      errors: ["authentication failed"],
    }),
  ).toEqual([{ type: "error", message: "authentication failed" }]);
});
