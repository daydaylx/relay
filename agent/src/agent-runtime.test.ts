import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createAssistantMessageEventStream, type AssistantMessage, type JsonObject, type Model } from "@earendil-works/pi-ai";
import type { StreamFn } from "@earendil-works/pi-agent-core";
import { createTaskAgent, relaySystemPrompt } from "./agent-runtime.js";
import { RelayBridge } from "./bridge.js";
import { createRelayTools } from "./relay-tools.js";
import { TaskService } from "./task-service.js";
import { TaskStore } from "./task.js";

test("agent prompt describes the OBSERVE command boundary without implying host access", () => {
  assert.match(relaySystemPrompt, /read-only Bubblewrap sandbox/);
  assert.match(relaySystemPrompt, /no host shell, host filesystem, network/);
  assert.match(relaySystemPrompt, /There is no host file write, MCP/);
});

test("Pi Agent keeps one task open across package search, plan, confirmed apply and post-apply verification", async () => {
  const root = mkdtempSync(join(tmpdir(), "relay-agent-loop-"));
  try {
    const tasks = new TaskService(new TaskStore(join(root, "tasks")));
    tasks.start("Install vlc so it is available system-wide");
    const calls: string[] = [];
    const core = {
      async request(action: string, params: Record<string, unknown> = {}) {
        calls.push(action);
        if (action === "search_package") return { kind: "package", results: [{ name: "vlc", description: "Video player", type: null, read_only: false }] };
        if (action === "plan") return { id: "vlc-plan", risk: "LIVE_SWITCHABLE", applicable: true, managed_diff: [], closure_diff: [] };
        if (action === "show") return { review: "Add vlc to the managed system packages." };
        if (action === "apply") return { outcome: "switched", change_id: params.change_id };
        if (action === "diagnose" && params.topic === "package") return { name: params.filter, available: true };
        throw new Error(`unexpected Core action ${action}`);
      },
    } as unknown as RelayBridge;
    const tools = createRelayTools(core, {
      task: tasks,
      configRoot: root,
      async confirmMutation() { return true; },
    });
    const callsFromModel: { name: string; arguments: JsonObject }[] = [
      { name: "relay_search_package", arguments: { query: "vlc" } },
      { name: "relay_plan_change", arguments: { intent: { schema: 1, changes: [{ op: "add_package", package: "vlc" }] } } },
      { name: "relay_apply_change", arguments: { change_id: "vlc-plan" } },
      { name: "relay_verify_goal", arguments: { check: "package_available", target: "vlc" } },
    ];
    const streamFn: StreamFn = () => {
      const next = callsFromModel.shift();
      assert.ok(next, "agent asked for more provider turns than expected");
      const isTool = true;
      const message: AssistantMessage = {
        role: "assistant",
        content: isTool ? [{ type: "toolCall", id: `call-${callsFromModel.length}`, name: next.name, arguments: next.arguments }] : [{ type: "text", text: "Task complete." }],
        api: "openai-completions", provider: "openai", model: "test-model",
        usage: { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
        stopReason: isTool ? "toolUse" : "stop", timestamp: Date.now(),
      };
      const stream = createAssistantMessageEventStream();
      queueMicrotask(() => {
        stream.push({ type: "start", partial: { ...message, content: [], stopReason: "pending" } });
        stream.push({ type: "done", reason: isTool ? "toolUse" : "stop", message });
      });
      return stream;
    };
    const runtime = createTaskAgent({ model: {} as Model<any>, streamFn, tools, tasks });
    await runtime.agent.prompt("Install vlc and verify it is available");

    assert.equal(tasks.current().state, "completed");
    assert.equal(tasks.current().verified, true);
    assert.deepEqual(calls, ["search_package", "plan", "show", "apply", "diagnose"]);
    assert.deepEqual(callsFromModel, []);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
