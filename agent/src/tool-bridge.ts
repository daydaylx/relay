import { createServer, type Server, type Socket } from "node:net";
import { mkdtempSync, chmodSync, lstatSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AgentTool } from "@earendil-works/pi-agent-core";
import type { TaskService } from "./task-service.js";

const MAX_RECORD_BYTES = 256 * 1024;
const MAX_TOOL_CALLS = 40;

export class RelayToolBridge {
  readonly directory: string;
  readonly socketPath: string;
  readonly extensionPath: string;
  private readonly tools: Map<string, AgentTool>;
  private readonly server: Server;
  private closed = false;
  private activeCalls = 0;
  private resolveReady!: () => void;
  private readonly ready: Promise<void>;

  private constructor(directory: string, tools: AgentTool[], private readonly tasks: TaskService) {
    this.directory = directory;
    this.socketPath = join(directory, "t.sock");
    this.extensionPath = join(directory, "relay-extension.mjs");
    this.tools = new Map(tools.map((tool) => [tool.name, tool]));
    this.server = createServer((socket) => this.accept(socket));
    this.ready = new Promise<void>((resolve) => { this.resolveReady = resolve; });
  }

  static async start(tools: AgentTool[], tasks: TaskService): Promise<RelayToolBridge> {
    const directory = mkdtempSync(join(tmpdir(), `relay-${process.getuid?.() ?? 0}-`));
    chmodSync(directory, 0o700);
    const stat = lstatSync(directory);
    if (stat.isSymbolicLink() || stat.uid !== (process.getuid?.() ?? stat.uid) || (stat.mode & 0o077) !== 0) {
      rmSync(directory, { recursive: true, force: true });
      throw new Error("Relay tool bridge directory is not private");
    }
    const bridge = new RelayToolBridge(directory, tools, tasks);
    bridge.writeExtension();
    await new Promise<void>((resolve, reject) => {
      bridge.server.once("error", reject);
      bridge.server.listen(bridge.socketPath, () => {
        bridge.server.removeListener("error", reject);
        chmodSync(bridge.socketPath, 0o600);
        resolve();
      });
    });
    return bridge;
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    await new Promise<void>((resolve) => this.server.close(() => resolve()));
    rmSync(this.directory, { recursive: true, force: true });
  }

  waitUntilLoaded(timeoutMs = 10_000): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("Pi did not load Relay's isolated tool extension")), timeoutMs);
      this.ready.then(() => { clearTimeout(timer); resolve(); }, (error: unknown) => { clearTimeout(timer); reject(error); });
    });
  }

  private writeExtension(): void {
    const definitions = [...this.tools.values()].map((tool) => ({
      name: tool.name,
      label: tool.label,
      description: tool.description,
      parameters: tool.parameters,
    }));
    const source = `import net from "node:net";
import { randomUUID } from "node:crypto";
const socketPath = process.env.RELAY_TOOL_SOCKET;
if (!socketPath) throw new Error("Relay tool socket is missing");
const definitions = ${JSON.stringify(definitions)};
function callRelay(tool, params, signal) {
  return new Promise((resolve, reject) => {
    if (signal.aborted) return reject(new Error("Relay tool call cancelled"));
    const socket = net.createConnection(socketPath);
    const id = randomUUID(); let buffer = ""; let settled = false;
    const finish = (error, result) => { if (settled) return; settled = true; signal.removeEventListener("abort", abort); socket.destroy(); error ? reject(error) : resolve(result); };
    const abort = () => finish(new Error("Relay tool call cancelled"));
    signal.addEventListener("abort", abort, { once: true });
    socket.setTimeout(120000, () => finish(new Error("Relay tool call timed out")));
    socket.once("error", (error) => finish(error));
    socket.on("data", (chunk) => {
      buffer += chunk.toString("utf8");
      if (Buffer.byteLength(buffer) > ${MAX_RECORD_BYTES}) return finish(new Error("Relay tool response exceeds size limit"));
      const newline = buffer.indexOf("\\n"); if (newline < 0) return;
      let response; try { response = JSON.parse(buffer.slice(0, newline)); } catch { return finish(new Error("Invalid Relay tool response")); }
      if (response.id !== id) return finish(new Error("Relay tool response id mismatch"));
      if (!response.ok) return finish(new Error(response.error || "Relay tool failed"));
      finish(undefined, response.result);
    });
    socket.once("connect", () => socket.write(JSON.stringify({ id, tool, params }) + "\\n"));
  });
}
export default function(pi) {
  for (const definition of definitions) pi.registerTool({
    ...definition,
    async execute(toolCallId, params, signal) {
      try { return await callRelay(definition.name, params, signal); }
      catch (error) { throw new Error(error instanceof Error ? error.message : "Relay tool failed"); }
    },
  });
  pi.on("session_start", async () => {
    await callRelay("__relay_ready", {}, { aborted: false, addEventListener() {}, removeEventListener() {} });
  });
}
`;
    writeFileSync(this.extensionPath, source, { mode: 0o600, flag: "wx" });
  }

  private accept(socket: Socket): void {
    socket.setTimeout(10_000, () => socket.destroy());
    let buffer = Buffer.alloc(0);
    socket.on("data", (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);
      if (buffer.length > MAX_RECORD_BYTES) {
        this.respond(socket, "", false, "Relay tool request exceeds size limit");
        return;
      }
      const newline = buffer.indexOf(10);
      if (newline < 0) return;
      const line = buffer.subarray(0, newline).toString("utf8");
      buffer = Buffer.alloc(0);
      void this.dispatch(socket, line);
    });
  }

  private async dispatch(socket: Socket, line: string): Promise<void> {
    let request: unknown;
    try { request = JSON.parse(line); } catch { this.respond(socket, "", false, "Invalid Relay tool request"); return; }
    if (!request || typeof request !== "object" || Array.isArray(request)) { this.respond(socket, "", false, "Invalid Relay tool request"); return; }
    const { id, tool, params } = request as Record<string, unknown>;
    if (tool === "__relay_ready" && typeof id === "string" && /^[a-f0-9-]{36}$/.test(id)) {
      this.resolveReady();
      this.respond(socket, id, true, undefined, { ready: true });
      return;
    }
    if (typeof id !== "string" || !/^[a-f0-9-]{36}$/.test(id) || typeof tool !== "string" || !this.tools.has(tool) || !params || typeof params !== "object" || Array.isArray(params)) {
      this.respond(socket, typeof id === "string" ? id : "", false, "Relay refused an invalid or unknown tool call"); return;
    }
    if (++this.activeCalls > MAX_TOOL_CALLS) { this.activeCalls--; this.respond(socket, id, false, "Relay task reached its tool-call limit"); return; }
    let began = false;
    const controller = new AbortController();
    socket.once("close", () => controller.abort());
    try {
      this.tasks.beginTool(tool); began = true;
      const result = await this.tools.get(tool)!.execute(id, params as never, controller.signal);
      this.tasks.toolResult(tool, "ok", { summary: "completed" });
      this.respond(socket, id, true, undefined, result);
    } catch (error) {
      if (began) {
        try { this.tasks.toolResult(tool, "error", { summary: "failed" }); } catch { /* task may have closed */ }
      }
      this.respond(socket, id, false, error instanceof Error ? error.message.slice(0, 500) : "Relay tool failed");
    } finally { this.activeCalls--; }
  }

  private respond(socket: Socket, id: string, ok: boolean, error?: string, result?: unknown): void {
    if (socket.destroyed) return;
    const record = JSON.stringify({ id, ok, ...(error ? { error } : {}), ...(result === undefined ? {} : { result }) }) + "\n";
    if (Buffer.byteLength(record) > MAX_RECORD_BYTES) socket.end(JSON.stringify({ id, ok: false, error: "Relay tool result exceeds size limit" }) + "\n");
    else socket.end(record);
  }
}
