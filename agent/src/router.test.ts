import assert from "node:assert/strict";
import test from "node:test";
import { diagnosticTopicForRequest, routeRequest } from "./router.js";
import { diagnosisQuestions } from "./diagnosis-corpus.js";

test("routes inspection and diagnosis without granting mutation", () => {
  assert.equal(routeRequest("What is my NixOS version?").route, "INSPECT");
  assert.equal(routeRequest("Why did nginx fail?").route, "DIAGNOSE");
});

test("routes supported changes through typed Relay planning", () => {
  assert.equal(routeRequest("Install ripgrep").route, "RELAY_CHANGE");
  assert.equal(routeRequest("Enable Bluetooth").route, "RELAY_CHANGE");
});

test("blocks protected mutations and unsupported write backends locally", () => {
  assert.equal(routeRequest("Change system.stateVersion").route, "BLOCKED");
  assert.equal(routeRequest("Change my Hyprland config").route, "DEVELOPMENT_REQUIRED");
  assert.equal(routeRequest("Why is Bluetooth not working?").route, "DIAGNOSE");
  assert.equal(routeRequest("Change SecureBoot").route, "BLOCKED");
});

test("the 20-question diagnosis corpus resolves to the expected read-only topic", () => {
  assert.equal(diagnosisQuestions.length, 20);
  for (const item of diagnosisQuestions) {
    assert.equal(diagnosticTopicForRequest(item.question), item.topic, item.question);
    assert.ok(["DIAGNOSE", "INSPECT"].includes(routeRequest(item.question).route), item.question);
  }
});
