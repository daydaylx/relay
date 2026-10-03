import assert from "node:assert/strict";
import test from "node:test";
import { RelayBridge } from "./bridge.js";

const echoCore = `let input="";process.stdin.on("data",d=>input+=d);process.stdin.on("end",()=>{const r=JSON.parse(input);process.stdout.write(JSON.stringify({schema_version:1,id:r.id,ok:true,data:{action:r.action,params:r.params}}))})`;

test("bridge sends only protocol requests and scopes parameters per action", async () => {
  const bridge = new RelayBridge({ flake: "/etc/nixos", host: "nixos" }, process.execPath, ["-e", echoCore]);
  assert.deepEqual(await bridge.request("health"), { action: "health", params: {} });
  assert.deepEqual(await bridge.request("units", { filter: "nginx", limit: 5 }), {
    action: "units",
    params: { filter: "nginx", limit: 5 },
  });
  assert.deepEqual(await bridge.request("status"), { action: "status", params: { flake: "/etc/nixos" } });
  assert.deepEqual(await bridge.request("plan", { intent: { schema: 1, changes: [] } }), {
    action: "plan",
    params: { flake: "/etc/nixos", host: "nixos", intent: { schema: 1, changes: [] } },
  });
});

test("bridge rejects an unlaunchable core with a bounded error", async () => {
  const bridge = new RelayBridge({}, "/relay/nonexistent-core-for-test", ["protocol", "--stdio"], 500);
  await assert.rejects(bridge.request("status"), /Relay Core could not be started/);
});
