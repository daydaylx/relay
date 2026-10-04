import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createConnection } from "node:net";
import { randomUUID } from "node:crypto";
import { test } from "node:test";
import { Type } from "typebox";
import { createPiRpcLaunch, PiRpcClient } from "./pi-rpc.js";
import { RelayToolBridge } from "./tool-bridge.js";
import { TaskService } from "./task-service.js";
import { TaskStore } from "./task.js";

test("Pi RPC receives only Relay-owned configuration, home and session paths", () => {
  const root = mkdtempSync(join(tmpdir(), "relay-pi-rpc-"));
  try {
    const config = createPiRpcLaunch({
      provider: "openai",
      model: "gpt-4o-mini",
      taskId: "task-123",
      relayConfigDirectory: join(root, "relay"),
      cwd: join(root, "workspace"),
      environment: {
        HOME: "/home/user",
        PATH: "/nix/store/bin",
        PI_CODING_AGENT_DIR: "/home/user/.pi/agent",
        PI_CODING_AGENT_SESSION_DIR: "/home/user/.pi/sessions",
        PI_PACKAGE_DIR: "/home/user/.pi/agent/packages",
        OPENAI_API_KEY: "relay-test-key",
        ANTHROPIC_API_KEY: "must-not-leak",
        XDG_CONFIG_HOME: "/home/user/.config",
      },
    });
    assert.equal(config.cwd, join(root, "workspace"));
    assert.equal(config.env.HOME, join(root, "relay", "pi", "home"));
    assert.equal(config.env.PI_CODING_AGENT_DIR, join(root, "relay", "pi"));
    assert.equal(config.env.PI_CODING_AGENT_SESSION_DIR, join(root, "relay", "pi", "sessions", "task-123"));
    assert.equal(config.env.OPENAI_API_KEY, "relay-test-key");
    assert.equal(config.env.ANTHROPIC_API_KEY, undefined);
    assert.equal(config.env.PI_PACKAGE_DIR, undefined);
    assert.equal(config.env.XDG_CONFIG_HOME, join(root, "relay", "pi", "xdg", "config"));
    assert.ok(config.args.includes("--no-context-files"));
    assert.ok(config.args.includes("--no-builtin-tools"));
    assert.ok(config.args.includes("--no-extensions"));
    assert.ok(config.args.includes("--offline"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("Pi RPC starts headlessly with a Relay-owned profile and returns structured state", async () => {
  const root = mkdtempSync(join(tmpdir(), "relay-pi-rpc-live-"));
  const workspace = join(root, "workspace");
  mkdirSync(workspace);
  try {
    const client = await PiRpcClient.start({
      provider: "openai",
      model: "gpt-4o-mini",
      taskId: "rpc-smoke",
      relayConfigDirectory: join(root, "config", "relay"),
      cwd: workspace,
      environment: { PATH: process.env.PATH, HOME: "/home/unused", PI_CODING_AGENT_DIR: "/home/unused/.pi" },
    });
    try {
      const response = await client.request("get_state");
      const data = response.data as { model?: { provider?: string }; isStreaming?: boolean; sessionFile?: string };
      assert.equal(response.success, true);
      assert.equal(data.model?.provider, "openai");
      assert.equal(data.isStreaming, false);
      assert.ok(data.sessionFile?.startsWith(join(root, "config", "relay", "pi", "sessions", "rpc-smoke")));
      assert.equal(data.sessionFile?.includes("/.pi/"), false);
    } finally {
      await client.close();
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("Pi RPC loads only the task-scoped Relay extension and reaches its local tool gateway", async () => {
  const root = mkdtempSync(join(tmpdir(), "relay-pi-tools-"));
  const tasks = new TaskService(new TaskStore(join(root, "tasks")));
  tasks.start("inspect test bridge");
  const toolBridge = await RelayToolBridge.start([{
    name: "relay_test_observe", label: "Relay test", description: "A test-only read operation.", parameters: Type.Object({ value: Type.String() }),
    async execute(_id, params) { const value = (params as { value: string }).value; return { content: [{ type: "text", text: `observed:${value}` }], details: { value } }; },
  }], tasks);
  const workspace = join(root, "workspace");
  mkdirSync(workspace);
  try {
    const client = await PiRpcClient.start({
      provider: "openai", model: "gpt-4o-mini", taskId: "rpc-tools", relayConfigDirectory: join(root, "config", "relay"), cwd: workspace,
      extensionPath: toolBridge.extensionPath, toolSocketPath: toolBridge.socketPath,
      environment: { PATH: process.env.PATH, HOME: "/home/unused", PI_CODING_AGENT_DIR: "/home/unused/.pi" },
    });
    try {
      await toolBridge.waitUntilLoaded();
      const response = await new Promise<{ id: string; ok: boolean; result?: { details?: { value?: string } } }>((resolve, reject) => {
        const socket = createConnection(toolBridge.socketPath);
        const id = randomUUID(); let buffer = "";
        socket.once("error", reject);
        socket.on("data", (chunk) => {
          buffer += chunk.toString("utf8");
          const end = buffer.indexOf("\n");
          if (end >= 0) { socket.destroy(); resolve(JSON.parse(buffer.slice(0, end)) as typeof response); }
        });
        socket.once("connect", () => socket.write(JSON.stringify({ id, tool: "relay_test_observe", params: { value: "local" } }) + "\n"));
      });
      assert.equal(response.ok, true);
      assert.equal(response.result?.details?.value, "local");
      assert.ok(tasks.current().events.some((event) => event.kind === "tool_started" && event.payload.name === "relay_test_observe"));
      assert.ok(tasks.current().events.some((event) => event.kind === "tool_result" && event.payload.name === "relay_test_observe"));
    } finally { await client.close(); }
  } finally {
    await toolBridge.close();
    rmSync(root, { recursive: true, force: true });
  }
});
