import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";

export type CoreAction = "status" | "health" | "units" | "generations" | "diagnose" | "plan" | "show" | "apply" | "undo_preview" | "undo" | "recover_preview" | "recover";
type CoreDefaults = { root?: string; flake?: string; host?: string; state_dir?: string };

interface CoreEnvelope {
  schema_version: number;
  id: string;
  ok: boolean;
  data?: unknown;
  error?: { code: string; message: string };
}

export class RelayBridge {
  constructor(
    private readonly defaults: CoreDefaults = {},
    private readonly executable = process.env.RELAY_CORE_PATH || "relay",
    private readonly args = ["protocol", "--stdio"],
    private readonly timeoutMs = 30_000,
  ) {}

  request(action: CoreAction, params: Record<string, unknown> = {}, signal?: AbortSignal): Promise<unknown> {
    if (signal?.aborted) return Promise.reject(new Error("Relay Core request cancelled"));
    const id = randomUUID();
    const allowedDefaults = action === "plan"
      ? this.defaults
      : action === "status"
        ? { root: this.defaults.root, flake: this.defaults.flake, state_dir: this.defaults.state_dir }
        : action === "units"
          ? {}
          : action === "generations"
            ? { root: this.defaults.root }
          : action === "diagnose"
            ? { root: this.defaults.root }
        : action === "show"
          ? { root: this.defaults.root, state_dir: this.defaults.state_dir }
          : action === "undo_preview" || action === "recover_preview"
            ? { root: this.defaults.root, state_dir: this.defaults.state_dir }
        : action === "apply" || action === "undo" || action === "recover"
          ? { root: this.defaults.root, state_dir: this.defaults.state_dir }
        : {};
    const payload = JSON.stringify({ schema_version: 1, id, action, params: { ...allowedDefaults, ...params } }) + "\n";
    if (Buffer.byteLength(payload) > 70 * 1024) return Promise.reject(new Error("Relay Core request exceeds the protocol limit"));

    return new Promise((resolve, reject) => {
      const child = spawn(this.executable, this.args, { stdio: ["pipe", "pipe", "ignore"] });
      let stdout = "";
      let settled = false;
      const finish = (error?: Error, data?: unknown) => {
        if (settled) return;
        settled = true;
        if (timer) clearTimeout(timer);
        signal?.removeEventListener("abort", abort);
        if (error) reject(error);
        else resolve(data);
      };
      const abort = () => {
        child.kill("SIGTERM");
        finish(new Error("Relay Core request cancelled"));
      };
      const timeoutApplies = action === "status" || action === "health" || action === "units" || action === "show" || action === "undo_preview" || action === "recover_preview";
      const timer = timeoutApplies ? setTimeout(() => {
        child.kill("SIGTERM");
        finish(new Error("Relay Core request timed out"));
      }, this.timeoutMs) : undefined;
      signal?.addEventListener("abort", abort, { once: true });

      child.on("error", () => finish(new Error("Relay Core could not be started")));
      child.stdout.setEncoding("utf8");
      child.stdout.on("data", (chunk: string) => {
        stdout += chunk;
        if (Buffer.byteLength(stdout) > 1024 * 1024) {
          child.kill("SIGTERM");
          finish(new Error("Relay Core response exceeds the protocol limit"));
        }
      });
      child.on("close", (code) => {
        if (settled) return;
        if (code !== 0) return finish(new Error("Relay Core exited unexpectedly"));
        const lines = stdout.trim().split("\n");
        if (lines.length !== 1) return finish(new Error("Relay Core returned an invalid response"));
        let response: CoreEnvelope;
        try {
          response = JSON.parse(lines[0]) as CoreEnvelope;
        } catch {
          return finish(new Error("Relay Core returned invalid JSON"));
        }
        if (response.schema_version !== 1 || response.id !== id || typeof response.ok !== "boolean") {
          return finish(new Error("Relay Core response does not match the request"));
        }
        if (!response.ok) {
          const code = response.error?.code;
          const message = response.error?.message;
          return finish(new Error(typeof code === "string" && typeof message === "string"
            ? `${code}: ${message}`
            : "Relay Core rejected the request"));
        }
        finish(undefined, response.data);
      });
      child.stdin.on("error", () => finish(new Error("Relay Core request could not be sent")));
      child.stdin.end(payload);
    });
  }
}
