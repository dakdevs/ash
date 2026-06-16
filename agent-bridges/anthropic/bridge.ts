import { query } from "@anthropic-ai/claude-agent-sdk";

export type BridgeRequest = {
  prompt: string;
  cwd: string;
  model?: string | null;
  claudePath?: string | null;
};

export type BridgeEvent =
  | { type: "status"; status: string }
  | { type: "assistant_text"; text: string }
  | { type: "tool_started"; command: string }
  | { type: "tool_output"; output: string }
  | { type: "tool_completed"; exit_code?: number | null }
  | {
      type: "usage";
      input_tokens: number;
      output_tokens: number;
      cache_read_input_tokens?: number | null;
      cache_creation_input_tokens?: number | null;
    }
  | { type: "result"; text: string }
  | { type: "error"; message: string; code?: string };

type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

type SdkContentBlock = {
  type?: string;
  text?: string;
  name?: string;
  input?: JsonValue;
  content?: JsonValue;
  tool_use_id?: string;
  is_error?: boolean;
};

type SdkMessage = {
  type?: string;
  subtype?: string;
  status?: string;
  result?: string;
  errors?: string[];
  error?: string;
  message?: {
    content?: SdkContentBlock[];
    usage?: {
      input_tokens?: number;
      output_tokens?: number;
      cache_read_input_tokens?: number;
      cache_creation_input_tokens?: number;
    };
  };
  usage?: {
    input_tokens?: number;
    output_tokens?: number;
    cache_read_input_tokens?: number;
    cache_creation_input_tokens?: number;
  };
};

export async function runBridge(
  request: BridgeRequest,
  emit: (event: BridgeEvent) => void,
): Promise<string> {
  let resultText = "";

  const options: Record<string, unknown> = {
    cwd: request.cwd,
    settingSources: ["user", "project", "local"],
  };
  if (request.model) {
    options.model = request.model;
  }
  if (request.claudePath) {
    options.pathToClaudeCodeExecutable = request.claudePath;
  }

  for await (const rawMessage of query({
    prompt: request.prompt,
    options,
  })) {
    const message = rawMessage as SdkMessage;
    for (const event of normalizeMessage(message)) {
      if (event.type === "result") {
        resultText = event.text;
      }
      emit(event);
    }
  }

  return resultText;
}

export function normalizeMessage(message: SdkMessage): BridgeEvent[] {
  const events: BridgeEvent[] = [];

  if (message.type === "system" && message.subtype === "init") {
    events.push({ type: "status", status: "started" });
  }

  if (message.type === "system" && typeof message.status === "string") {
    events.push({ type: "status", status: message.status });
  }

  if (message.type === "assistant") {
    for (const block of message.message?.content ?? []) {
      if (block.type === "text" && block.text) {
        events.push({ type: "assistant_text", text: block.text });
      } else if (block.type === "tool_use") {
        events.push({
          type: "tool_started",
          command: displayToolUse(block),
        });
      }
    }
  }

  if (message.type === "user") {
    for (const block of message.message?.content ?? []) {
      if (block.type === "tool_result") {
        const output = displayToolResult(block);
        if (output) {
          events.push({ type: "tool_output", output });
        }
        events.push({ type: "tool_completed", exit_code: null });
      }
    }
  }

  if (message.type === "result") {
    const usage = usageEvent(message.usage);
    if (usage) {
      events.push(usage);
    }

    if (message.subtype === "success") {
      events.push({ type: "result", text: message.result ?? "" });
    } else {
      events.push({
        type: "error",
        message:
          message.errors?.join("\n") ??
          message.error ??
          `Claude Agent SDK returned ${message.subtype ?? "an error"}`,
      });
    }
  }

  return events;
}

function displayToolUse(block: SdkContentBlock): string {
  if (block.name === "Bash") {
    const command = objectValue(block.input, "command");
    if (typeof command === "string") {
      return command;
    }
  }

  const input = block.input == null ? "" : ` ${JSON.stringify(block.input)}`;
  return `${block.name ?? "tool"}${input}`;
}

function displayToolResult(block: SdkContentBlock): string {
  const content = block.content;
  if (typeof content === "string") {
    return content;
  }
  if (Array.isArray(content)) {
    return content
      .map((item) =>
        typeof item === "object" &&
        item != null &&
        "text" in item &&
        typeof item.text === "string"
          ? item.text
          : JSON.stringify(item),
      )
      .join("\n");
  }
  return content == null ? "" : JSON.stringify(content);
}

function usageEvent(
  usage:
    | {
        input_tokens?: number;
        output_tokens?: number;
        cache_read_input_tokens?: number;
        cache_creation_input_tokens?: number;
      }
    | undefined,
): BridgeEvent | null {
  if (usage?.input_tokens == null || usage.output_tokens == null) {
    return null;
  }

  return {
    type: "usage",
    input_tokens: usage.input_tokens,
    output_tokens: usage.output_tokens,
    cache_read_input_tokens: usage.cache_read_input_tokens ?? null,
    cache_creation_input_tokens: usage.cache_creation_input_tokens ?? null,
  };
}

function objectValue(value: JsonValue | undefined, key: string): JsonValue | undefined {
  if (typeof value !== "object" || value == null || Array.isArray(value)) {
    return undefined;
  }
  return value[key];
}
