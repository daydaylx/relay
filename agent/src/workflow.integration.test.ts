import assert from "node:assert/strict";
import test from "node:test";
import { RelayBridge } from "./bridge.js";
import { MutationGate } from "./mutation-gate.js";

const enabled = process.env.RELAY_AGENT_WORKFLOW_TEST === "1";

test("agent confirmation workflow plans, previews, applies and undoes through Relay Core", { skip: !enabled }, async () => {
  const flake = process.env.RELAY_AGENT_FLAKE;
  const host = process.env.RELAY_AGENT_HOST;
  const stateDir = process.env.RELAY_AGENT_STATE_DIR;
  assert.ok(flake && host && stateDir, "workflow test needs an isolated flake, host and state directory");
  const bridge = new RelayBridge({ flake, host, state_dir: stateDir });
  const gate = new MutationGate();
  const plan = await bridge.request("plan", {
    intent: { schema: 1, changes: [{ op: "add_package", package: "hello" }] },
  }) as { id: string; risk: string; applicable: boolean; candidate_system: string };
  assert.equal(plan.risk, "LIVE_SWITCHABLE");
  assert.equal(plan.applicable, true);
  const review = await bridge.request("show", { change_id: plan.id }) as { review: string };
  assert.match(review.review, /hello/);

  gate.recordPlan({ id: plan.id, risk: plan.risk, applicable: plan.applicable });
  const applyPhrase = gate.requestApply(plan.id);
  assert.equal(applyPhrase, `APPLY ${plan.id}`);
  const apply = gate.consume(applyPhrase!);
  assert.equal(apply.kind, "authorized");
  if (apply.kind !== "authorized") throw new Error("Apply confirmation was not authorized");
  const result = await bridge.request(apply.action, apply.params) as { outcome: string };
  assert.equal(result.outcome, "switched");
  gate.clearPlan();

  const undoPreview = await bridge.request("undo_preview") as { change_id: string; review: string };
  assert.equal(undoPreview.change_id, plan.id);
  assert.match(undoPreview.review, /hello/);
  const undoPhrase = gate.requestUndo(undoPreview);
  assert.equal(undoPhrase, `UNDO ${plan.id}`);
  const undo = gate.consume(undoPhrase!);
  assert.equal(undo.kind, "authorized");
  if (undo.kind !== "authorized") throw new Error("Undo confirmation was not authorized");
  const undone = await bridge.request(undo.action, undo.params) as { outcome: string; reason: string };
  assert.equal(undone.outcome, "rolled-back");
  assert.equal(undone.reason, "undo");

  const abandonedPlan = await bridge.request("plan", {
    intent: { schema: 1, changes: [{ op: "add_package", package: "hello" }] },
  }) as { id: string; applicable: boolean; risk: string };
  const recovery = await bridge.request("recover_preview") as Array<{ id: string; state: string }>;
  assert.deepEqual(recovery.map((item) => item.id), [abandonedPlan.id]);
  const recoveryConfirmation = gate.requestRecover(recovery);
  assert.equal(recoveryConfirmation?.phrase, `RECOVER ${abandonedPlan.id}`);
  const recoveryDecision = gate.consume(recoveryConfirmation!.phrase);
  assert.equal(recoveryDecision.kind, "authorized");
  if (recoveryDecision.kind !== "authorized") throw new Error("Recovery confirmation was not authorized");
  const recovered = await bridge.request(recoveryDecision.action, recoveryDecision.params) as Array<{ id: string; summary: string }>;
  assert.deepEqual(recovered.map((item) => item.id), [abandonedPlan.id]);
  assert.match(recovered[0].summary, /abandoned candidate closed/);
  assert.deepEqual(await bridge.request("recover_preview"), []);
});
