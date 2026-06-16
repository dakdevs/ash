import { runBridge, type BridgeEvent, type BridgeRequest } from "./bridge";

function emit(event: BridgeEvent): void {
  process.stdout.write(`${JSON.stringify(event)}\n`);
}

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString("utf8");
}

async function main(): Promise<void> {
  const source = await readStdin();
  const request = JSON.parse(source) as BridgeRequest;
  await runBridge(request, emit);
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  emit({ type: "error", message });
  process.exitCode = 1;
});
