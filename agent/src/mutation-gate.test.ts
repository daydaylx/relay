import assert from "node:assert/strict";
import test from "node:test";
import { MutationGate } from "./mutation-gate.js";

test("apply needs a reviewed applicable plan and the exact direct-user phrase", () => {
  const gate = new MutationGate();
  assert.equal(gate.requestApply("plan-1"), undefined);
  gate.recordPlan({ id: "plan-1", applicable: false, risk: "live-switchable" });
  assert.equal(gate.requestApply("plan-1"), undefined);
  gate.recordPlan({ id: "plan-1", applicable: true, risk: "live-switchable" });
  assert.equal(gate.requestApply("other-plan"), undefined);
  assert.equal(gate.requestApply("plan-1"), "APPLY plan-1");
  assert.deepEqual(gate.consume("apply plan-1"), { kind: "cancelled" });
  assert.deepEqual(gate.consume("APPLY plan-1"), { kind: "none" });
  gate.requestApply("plan-1");
  assert.deepEqual(gate.consume("APPLY plan-1"), {
    kind: "authorized",
    action: "apply",
    params: { change_id: "plan-1", confirmed: true },
  });
});

test("undo and recovery confirmations bind to the exact preview targets", () => {
  const gate = new MutationGate();
  assert.equal(gate.requestUndo({ change_id: "change-9", review: "review" }), "UNDO change-9");
  assert.deepEqual(gate.consume("UNDO change-8"), { kind: "cancelled" });
  assert.equal(gate.requestRecover([{ id: "plan-2" }, { id: "plan-1" }])?.phrase, "RECOVER plan-2 plan-1");
  assert.deepEqual(gate.consume("RECOVER plan-2 plan-1"), {
    kind: "authorized",
    action: "recover",
    params: { expected_ids: ["plan-2", "plan-1"], confirmed: true },
  });
  assert.equal(gate.requestRecover([{ id: "../etc/passwd" }]), undefined);
  assert.equal(gate.requestRecover([{ id: "plan-1" }, { id: "plan-1" }]), undefined);
});
