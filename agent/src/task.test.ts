import assert from "node:assert/strict";
import test from "node:test";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { TaskStore } from "./task.js";
import { TaskService } from "./task-service.js";

test("task events persist independently and only complete after a passing verification", () => {
  const root = mkdtempSync(join(tmpdir(), "relay-task-store-"));
  try {
    const store = new TaskStore(join(root, "tasks"));
    const created = store.create("Bluetooth funktioniert nicht");
    assert.equal(created.state, "created");
    assert.equal(statSync(join(root, "tasks")).mode & 0o777, 0o700);
    assert.equal(statSync(join(root, "tasks", `${created.id}.jsonl`)).mode & 0o777, 0o600);

    store.append(created.id, "status_summary", "investigating", { topic: "bluetooth", controller_count: 0 });
    store.append(created.id, "plan_ready", "planning", { change_id: "change-1", risk: "LIVE_SWITCHABLE" });
    store.append(created.id, "confirmation_required", "waiting_confirmation", { change_id: "change-1" });
    store.append(created.id, "applying", "applying", { change_id: "change-1" });
    store.append(created.id, "verification", "verifying", { check: "controller_present", passed: false });
    store.append(created.id, "task_continuing", "continuing", { reason: "goal_not_reached" });
    store.append(created.id, "status_summary", "investigating", { topic: "bluetooth", controller_count: 1 });
    store.append(created.id, "verification", "verifying", { check: "controller_present", passed: true });
    const completed = store.append(created.id, "completed", "completed", { evidence: "controller_present" });

    assert.equal(completed.state, "completed");
    assert.equal(completed.verified, true);
    assert.deepEqual(completed.change_ids, ["change-1"]);
    assert.deepEqual(store.list().map((task) => task.id), [created.id]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("task store rejects false completion, illegal transitions, and secret-bearing goals/events", () => {
  const root = mkdtempSync(join(tmpdir(), "relay-task-invalid-"));
  try {
    const store = new TaskStore(join(root, "tasks"));
    assert.throws(() => store.create("api_key=sk-12345678901234567890"), /secret/);
    const task = store.create("Check Bluetooth");
    assert.throws(() => store.append(task.id, "completed", "completed", {}), /invalid task transition/);
    store.append(task.id, "status_summary", "investigating", { topic: "bluetooth" });
    assert.throws(() => store.append(task.id, "tool_result", "investigating", { note: "password=very-secret-value" }), /secret/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("verified system context is journaled, restored on resume, and rejects secret-like values", () => {
  const root = mkdtempSync(join(tmpdir(), "relay-task-context-"));
  try {
    const store = new TaskStore(join(root, "tasks"));
    const tasks = new TaskService(store);
    const task = tasks.start("Inspect current host");
    const facts = { machine_identity: { os_name: "NixOS", hostname: "test-host" }, live_snapshot: { active_generation: 12 } };
    tasks.recordVerifiedSystemContext(facts);
    assert.throws(() => tasks.recordVerifiedSystemContext({ api_key: "sk-123456789012345678901234" }), /secret/);
    const resumed = new TaskService(store);
    resumed.resume(task.id);
    assert.ok(resumed.contextSummary().includes('"verified_system_context"'));
    assert.ok(resumed.contextSummary().includes('"active_generation":12'));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
