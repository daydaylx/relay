import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { EventEmitter } from "node:events";
import { randomUUID } from "node:crypto";

export interface PiRpcConfig {
  provider: string;
  model: string;
  taskId: string;
  relayConfigDirectory: string;
  cwd: string;
  extensionPath?: string;
  toolSocketPath?: string;
  systemPrompt?: string;
  nodeExecutable?: string;
  environment?: NodeJS.ProcessEnv;
}

export interface PiRpcMessage {
  type: string;
  id?: string;
  command?: string;
  success?: boolean;
  [key: string]: unknown;
}

export function resolvePiCliPath(): string {
  const packageEntry = fileURLToPath(import.meta.resolve("@earendil-works/pi-coding-agent"));
  return join(dirname(packageEntry), "bundle", "cli.js");
}

export function createPiRpcLaunch(config: PiRpcConfig): {
  executable: string;
  args: string[];
  cwd: string;
  env: NodeJS.ProcessEnv;
} {
  if (!/^[a-zA-Z0-9_-]{1,128}$/.test(config.taskId)) throw new Error("invalid Relay task id for Pi session");
  if (!/^[a-z0-9-]{1,64}$/.test(config.provider) || !/^[a-zA-Z0-9_.:/-]{1,128}$/.test(config.model)) {
    throw new Error("invalid Pi provider or model selector");
  }
  const piHome = join(config.relayConfigDirectory, "pi");
  const sessionDirectory = join(piHome, "sessions", config.taskId);
  const homeDirectory = join(piHome, "home");
  mkdirSync(sessionDirectory, { recursive: true, mode: 0o700 });
  mkdirSync(homeDirectory, { recursive: true, mode: 0o700 });
  const inherited = config.environment ?? process.env;
  const env: NodeJS.ProcessEnv = {};
  for (const key of ["PATH", "LANG", "LC_ALL", "SSL_CERT_FILE", "SSL_CERT_DIR", "NODE_EXTRA_CA_CERTS", "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY"]) {
    const value = inherited[key];
    if (value !== undefined) env[key] = value;
  }
  const apiKeyByProvider: Record<string, string> = {
    openai: "OPENAI_API_KEY",
    anthropic: "ANTHROPIC_API_KEY",
    google: "GEMINI_API_KEY",
  };
  const providerKey = apiKeyByProvider[config.provider];
  if (providerKey && inherited[providerKey]) env[providerKey] = inherited[providerKey];
  env.HOME = homeDirectory;
  env.XDG_CONFIG_HOME = join(piHome, "xdg", "config");
  env.XDG_DATA_HOME = join(piHome, "xdg", "data");
  env.XDG_CACHE_HOME = join(piHome, "xdg", "cache");
  env.PI_CODING_AGENT_DIR = piHome;
  env.PI_CODING_AGENT_SESSION_DIR = sessionDirectory;
  env.PI_OFFLINE = "1";
  env.PI_SKIP_VERSION_CHECK = "1";
  env.PI_TELEMETRY = "0";
  if (config.toolSocketPath) env.RELAY_TOOL_SOCKET = config.toolSocketPath;

  const args = [
      resolvePiCliPath(), "--mode", "rpc", "--provider", config.provider, "--model", config.model,
      "--session-dir", sessionDirectory,
      "--no-builtin-tools", "--no-context-files", "--no-approve", "--no-skills",
      "--no-prompt-templates", "--no-themes", "--no-extensions", "--offline",
    ];
  if (config.extensionPath) {
    // Pi's explicit CLI extension is Relay-owned and loaded from its private runtime tree.
    // --no-extensions disables discovery from user/project locations.
    args.push("--extension", config.extensionPath);
  }
  if (config.systemPrompt) {
    if (Buffer.byteLength(config.systemPrompt) > 48 * 1024) throw new Error("Relay system context exceeds its size limit");
    args.push("--system-prompt", config.systemPrompt);
  }
  return {
    executable: config.nodeExecutable ?? process.execPath,
    args,
    cwd: config.cwd,
    env,
  };
}

export class PiRpcClient extends EventEmitter {
  private readonly child: ChildProcessWithoutNullStreams;
  private buffer = "";
  private stderr = "";
  private readonly pending = new Map<string, { resolve: (message: PiRpcMessage) => void; reject: (error: Error) => void; timer: NodeJS.Timeout }>();
  private closed = false;

  private constructor(child: ChildProcessWithoutNullStreams) {
    super();
    this.child = child;
    child.stdout.on("data", (chunk: Buffer) => this.onStdout(chunk));
    child.stderr.on("data", (chunk: Buffer) => {
      this.stderr = (this.stderr + chunk.toString("utf8")).slice(-16_384);
      this.emit("stderr", chunk.toString("utf8"));
    });
    child.once("error", (error) => this.failAll(error));
    child.once("exit", (code, signal) => {
      this.closed = true;
      this.failAll(new Error(`Pi RPC exited (${signal ?? code}): ${this.stderr.slice(-2_000)}`));
      this.emit("exit", { code, signal });
    });
  }

  static async start(config: PiRpcConfig): Promise<PiRpcClient> {
    const launch = createPiRpcLaunch(config);
    const client = new PiRpcClient(spawn(launch.executable, launch.args, {
      cwd: launch.cwd,
      env: launch.env,
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    }));
    try {
      await client.request("get_state", {}, 20_000);
      return client;
    } catch (error) {
      await client.close();
      throw error;
    }
  }

  request(command: string, payload: Record<string, unknown> = {}, timeoutMs = 15_000): Promise<PiRpcMessage> {
    if (this.closed || this.child.stdin.destroyed) return Promise.reject(new Error("Pi RPC process is closed"));
    const id = randomUUID();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`Pi RPC command timed out: ${command}`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      const record = JSON.stringify({ ...payload, id, type: command }) + "\n";
      this.child.stdin.write(record, "utf8", (error) => {
        if (!error) return;
        const pending = this.pending.get(id);
        if (!pending) return;
        clearTimeout(pending.timer);
        this.pending.delete(id);
        pending.reject(error);
      });
    });
  }

  prompt(message: string): Promise<void> {
    return this.request("prompt", { message }, 30_000).then(() => undefined);
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    this.child.stdin.end();
    this.child.kill("SIGTERM");
    await new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        this.child.kill("SIGKILL");
        resolve();
      }, 2_000);
      this.child.once("exit", () => {
        clearTimeout(timer);
        resolve();
      });
    });
    this.failAll(new Error("Pi RPC client closed"));
  }

  private onStdout(chunk: Buffer): void {
    this.buffer += chunk.toString("utf8");
    if (this.buffer.length > 8 * 1024 * 1024 && !this.buffer.includes("\n")) {
      this.failAll(new Error("Pi RPC emitted an oversized JSONL record"));
      this.child.kill("SIGKILL");
      return;
    }
    let newline = this.buffer.indexOf("\n");
    while (newline !== -1) {
      const raw = this.buffer.slice(0, newline).replace(/\r$/, "");
      this.buffer = this.buffer.slice(newline + 1);
      if (raw.length > 8 * 1024 * 1024) {
        this.failAll(new Error("Pi RPC emitted an oversized JSONL record"));
        this.child.kill("SIGKILL");
        return;
      }
      if (raw) this.onRecord(raw);
      newline = this.buffer.indexOf("\n");
    }
  }

  private onRecord(record: string): void {
    let message: PiRpcMessage;
    try {
      const parsed: unknown = JSON.parse(record);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed) || typeof (parsed as { type?: unknown }).type !== "string") {
        throw new Error("RPC JSONL record is not a typed object");
      }
      message = parsed as PiRpcMessage;
    } catch (error) {
      this.emit("protocol_error", error instanceof Error ? error : new Error("invalid Pi RPC JSONL record"));
      return;
    }
    if (message.type === "response" && typeof message.id === "string") {
      const pending = this.pending.get(message.id);
      if (pending) {
        clearTimeout(pending.timer);
        this.pending.delete(message.id);
        if (message.success === false) pending.reject(new Error(`Pi RPC ${message.command ?? "command"} failed`));
        else pending.resolve(message);
        return;
      }
    }
    this.emit("event", message);
  }

  private failAll(error: Error): void {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pending.clear();
  }
}
