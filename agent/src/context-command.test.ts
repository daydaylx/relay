import assert from "node:assert/strict";
import { test } from "node:test";
import { renderSystemContext } from "./context-command.js";

test("human context output summarizes verified facts and does not print raw paths", () => {
  const output = renderSystemContext({
    observed_at: "2026-10-04T10:00:00.000Z",
    machine_identity: { hostname: "workstation", os_name: "NixOS", os_version: "26.05", architecture: "x86_64", kernel: "6.12.3" },
    configuration_identity: { configuration_revision: "aabbccdd", nixpkgs_revision: "11223344", managed_module: "in-sync" },
    live_snapshot: { active_generation: 14, booted_generation: 13, failed_units: [], desktop_session: "wayland:Hyprland", desktop: { version: "0.55.1" } },
    ownership: [{ kind: "RELAY_MANAGED", owner: "Relay", writable: true, path: "/etc/nixos/relay/managed.nix" }, { kind: "DIRECT_USER_CONFIG", owner: "current user", writable: false, path: "/home/user/.config/hypr" }],
  });
  assert.match(output, /workstation/);
  assert.match(output, /active 14 · booted 13/);
  assert.match(output, /Hyprland 0\.55\.1/);
  assert.match(output, /Relay Core typed changes only/);
  assert.match(output, /DIRECT_USER_CONFIG: current user · read-only/);
  assert.doesNotMatch(output, /\/etc\/nixos|\/home\/user/);
});

test("human context output handles unavailable facts explicitly", () => {
  const output = renderSystemContext({});
  assert.match(output, /unknown host/);
  assert.match(output, /active unknown · booted unknown/);
  assert.match(output, /no failed units reported/);
});
