import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createRelayTools } from "./relay-tools.js";
import type { RelayBridge } from "./bridge.js";
import { TaskService } from "./task-service.js";
import { TaskStore } from "./task.js";

test("multi-step goal flows through Core planning, local confirmation, apply and structured verification", async () => {
  const root = mkdtempSync(join(tmpdir(), "relay-task-flow-"));
  try {
    const task = new TaskService(new TaskStore(join(root, "tasks")));
    const initial = task.start("Install vlc so it is available system-wide");
    const calls: string[] = [];
    let confirmation: unknown;
    const bridge = {
      async request(action: string, params: Record<string, unknown> = {}) {
        calls.push(action);
        if (action === "plan") return { id: "plan-vlc", risk: "LIVE_SWITCHABLE", applicable: true, managed_diff: ["+ vlc"], closure_diff: [] };
        if (action === "show") return { review: "Add vlc; recovery is available." };
        if (action === "apply") return { outcome: "switched", change_id: params.change_id };
        if (action === "diagnose" && params.topic === "package") return { name: params.filter, available: true };
        throw new Error(`unexpected Core action: ${action}`);
      },
    } as unknown as RelayBridge;
    const tools = createRelayTools(bridge, {
      task,
      configRoot: root,
      async confirmMutation(details) { confirmation = details; return true; },
    });
    assert.equal(task.current().id, initial.id);
    assert.equal(tools.some((tool) => tool.name === "shell" || tool.name === "sudo"), false);
    const planTool = tools.find((tool) => tool.name === "relay_plan_change")!;
    await planTool.execute("p", { intent: { schema: 1, changes: [{ op: "add_package", package: "vlc" }] } }, undefined, undefined);
    const applyTool = tools.find((tool) => tool.name === "relay_apply_change")!;
    await applyTool.execute("a", { change_id: "plan-vlc" }, undefined, undefined);
    const verifyTool = tools.find((tool) => tool.name === "relay_verify_goal")!;
    const verified = await verifyTool.execute("v", { check: "package_available", target: "vlc" }, undefined, undefined);
    assert.equal(task.current().state, "completed");
    assert.equal(task.current().verified, true);
    assert.deepEqual(calls, ["plan", "show", "apply", "diagnose"]);
    assert.equal((confirmation as { reviewHash?: string })?.reviewHash?.length, 64);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test("declined task apply returns to investigation without calling Core apply", async () => {
  const root = mkdtempSync(join(tmpdir(), "relay-task-decline-"));
  try {
    const task = new TaskService(new TaskStore(join(root, "tasks")));
    task.start("Install vlc");
    const calls: string[] = [];
    const bridge = {
      async request(action: string) {
        calls.push(action);
        if (action === "plan") return { id: "plan-vlc", risk: "LIVE_SWITCHABLE", applicable: true };
        if (action === "show") return { review: "Add vlc" };
        throw new Error(`unexpected Core action: ${action}`);
      },
    } as unknown as RelayBridge;
    const tools = createRelayTools(bridge, { task, configRoot: root, async confirmMutation() { return false; } });
    await tools.find((tool) => tool.name === "relay_plan_change")!.execute("p", { intent: { schema: 1, changes: [{ op: "add_package", package: "vlc" }] } }, undefined, undefined);
    await tools.find((tool) => tool.name === "relay_apply_change")!.execute("a", { change_id: "plan-vlc" }, undefined, undefined);
    assert.deepEqual(calls, ["plan", "show"]);
    assert.equal(task.current().state, "investigating");
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test("failed goal verification keeps the same task open for a second plan and successful verification", async () => {
  const root = mkdtempSync(join(tmpdir(), "relay-task-continue-"));
  try {
    const task = new TaskService(new TaskStore(join(root, "tasks")));
    task.start("Install vlc so it is available system-wide");
    const calls: string[] = [];
    let applies = 0;
    const bridge = {
      async request(action: string, params: Record<string, unknown> = {}) {
        calls.push(action);
        if (action === "plan") return { id: `plan-vlc-${calls.filter((name) => name === "plan").length}`, risk: "LIVE_SWITCHABLE", applicable: true };
        if (action === "show") return { review: `Add vlc (${params.change_id})` };
        if (action === "apply") return { outcome: "switched", change_id: params.change_id };
        if (action === "diagnose" && params.topic === "package") return { name: params.filter, available: applies > 1 };
        throw new Error(`unexpected Core action: ${action}`);
      },
    } as unknown as RelayBridge;
    const tools = createRelayTools(bridge, {
      task,
      configRoot: root,
      async confirmMutation() { applies += 1; return true; },
    });
    const planTool = tools.find((tool) => tool.name === "relay_plan_change")!;
    const applyTool = tools.find((tool) => tool.name === "relay_apply_change")!;
    const verifyTool = tools.find((tool) => tool.name === "relay_verify_goal")!;
    await planTool.execute("p1", { intent: { schema: 1, changes: [{ op: "add_package", package: "vlc" }] } }, undefined, undefined);
    await applyTool.execute("a1", { change_id: "plan-vlc-1" }, undefined, undefined);
    const first = await verifyTool.execute("v1", { check: "package_available", target: "vlc" }, undefined, undefined);
    assert.equal(task.current().state, "investigating");
    assert.equal(task.current().verified, false);
    assert.equal((first.details as { verified: boolean }).verified, false);

    await planTool.execute("p2", { intent: { schema: 1, changes: [{ op: "add_package", package: "vlc" }] } }, undefined, undefined);
    await applyTool.execute("a2", { change_id: "plan-vlc-2" }, undefined, undefined);
    await verifyTool.execute("v2", { check: "package_available", target: "vlc" }, undefined, undefined);
    assert.equal(task.current().state, "completed");
    assert.equal(task.current().verified, true);
    assert.deepEqual(task.current().change_ids, ["plan-vlc-1", "plan-vlc-2"]);
    assert.equal(calls.filter((name) => name === "apply").length, 2);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
