import assert from "node:assert/strict";
import test from "node:test";
import { createRelayTools } from "./relay-tools.js";
import type { RelayBridge } from "./bridge.js";
import { diagnosisQuestions } from "./diagnosis-corpus.js";
import { diagnosticTopicForRequest } from "./router.js";

test("model tools exclude mutation and keep paths and exact diff values out of model context", async () => {
  const fake = {
    async request(action: string) {
      if (action === "status") return {
        hostname: "nixos",
        os_name: "NixOS",
        running_system_path: "/nix/store/private-system-path",
        configuration_revision: "private-revision",
        failed_units: [],
        desktop: { active_window_class: "private-app" },
      };
      if (action === "plan") return {
        id: "plan-123",
        risk: "live-switchable",
        applicable: true,
        managed_diff: ["+ secret = \"sample-token-value\";"],
        candidate_system: "/nix/store/private-candidate",
      };
      if (action === "units") return [{ unit: "nginx.service", active: "active", sub: "running" }];
      if (action === "generations") return [{ number: 4, active: true, booted: false, system_path: "/nix/store/private-system" }];
      if (action === "diagnose") return { available: true, contains_message_text: false, records: [{ timestamp_usec: 1, unit: "nginx.service", priority: 3, message_id: "a1", MESSAGE: "private-token" }] };
      return { system_state: "running", unhealthy_units: [] };
    },
  } as unknown as RelayBridge;
  const tools = createRelayTools(fake);
  assert.deepEqual(tools.map((tool) => tool.name), [
    "relay_system_status",
    "relay_system_health",
    "relay_list_units",
    "relay_diagnose",
    "relay_plan_change",
    "relay_show_plan",
  ]);
  const status = await tools[0].execute("s", {}, undefined, undefined);
  const statusText = status.content.map((part) => part.type === "text" ? part.text : "").join("");
  assert.equal(statusText.includes("private-system-path"), false);
  assert.equal(statusText.includes("private-revision"), false);
  assert.equal(statusText.includes("private-app"), false);

  const diagnosis = await tools[3].execute("d", { topic: "journal", unit: "nginx.service" }, undefined, undefined);
  assert.equal(JSON.stringify(diagnosis.content).includes("private-token"), false);
  assert.equal(JSON.stringify(diagnosis.content).includes("MESSAGE"), false);
  const generations = await tools[3].execute("d", { topic: "generations" }, undefined, undefined);
  assert.equal(JSON.stringify(generations.content).includes("/nix/store/private-system"), false);

  const plan = await tools[4].execute("p", { intent: { schema: 1, changes: [] } }, undefined, undefined);
  const planText = plan.content.map((part) => part.type === "text" ? part.text : "").join("");
  assert.equal(planText.includes("sample-token-value"), false);
  assert.equal(planText.includes("private-candidate"), false);
  assert.equal(JSON.stringify(plan.details).includes("sample-token-value"), true);
});

test("20 diagnostic questions call only read-only Relay protocol actions", async () => {
  const actions: string[] = [];
  const fake = {
    async request(action: string) {
      actions.push(action);
      if (action === "status") return { os_name: "NixOS", active_generation: 1, booted_generation: 1 };
      if (action === "generations") return [{ number: 1, active: true, booted: true, system_path: "/nix/store/secret" }];
      if (action === "health") return { system_state: "running", unhealthy_units: [] };
      if (action === "units") return [{ unit: "demo.service", active: "active", sub: "running" }];
      if (action === "diagnose") return { interfaces: [], processes: [], records: [], block_devices: [], pci_adapters: [] };
      throw new Error(`unexpected protocol action: ${action}`);
    },
  } as unknown as RelayBridge;
  const tool = createRelayTools(fake).find((item) => item.name === "relay_diagnose");
  assert.ok(tool);
  for (const item of diagnosisQuestions) {
    assert.equal(diagnosticTopicForRequest(item.question), item.topic);
    await tool.execute("d", { topic: item.topic, ...(item.topic === "journal" && item.question.includes("nginx") ? { unit: "nginx.service" } : {}) }, undefined, undefined);
  }
  assert.equal(actions.some((action) => ["apply", "undo", "recover", "exec"].includes(action)), false);
  assert.ok(actions.includes("diagnose"));
  assert.ok(actions.includes("health"));
  assert.ok(actions.includes("units"));
});
