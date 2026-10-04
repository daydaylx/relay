import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { configPath, initializeConfig, loadConfig, relayRuntimeConfigDirectory } from "./settings.js";

test("Relay settings come from Relay's config path and ignore a Pi profile", () => {
  const dir = mkdtempSync(join(tmpdir(), "relay-agent-config-"));
  const previous = { config: process.env.RELAY_AGENT_CONFIG, pi: process.env.PI_CODING_AGENT_DIR };
  try {
    const relayConfig = join(dir, "relay", "agent.json");
    process.env.RELAY_AGENT_CONFIG = relayConfig;
    process.env.PI_CODING_AGENT_DIR = join(dir, "pi-profile-with-hostile-settings");
    initializeConfig();
    assert.equal(configPath(), relayConfig);
    const config = loadConfig();
    assert.equal(config.provider, "openai");
    assert.equal(config.model, "gpt-4o-mini");
    assert.equal(config.flake, "/etc/nixos");
    assert.ok(config.host.length > 0);
    assert.equal(statSync(relayConfig).mode & 0o777, 0o600);
    assert.throws(() => initializeConfig(), /already exists/);
  } finally {
    if (previous.config === undefined) delete process.env.RELAY_AGENT_CONFIG;
    else process.env.RELAY_AGENT_CONFIG = previous.config;
    if (previous.pi === undefined) delete process.env.PI_CODING_AGENT_DIR;
    else process.env.PI_CODING_AGENT_DIR = previous.pi;
    rmSync(dir, { recursive: true, force: true });
  }
});

test("invalid config and model selectors fail closed", () => {
  const dir = mkdtempSync(join(tmpdir(), "relay-agent-invalid-config-"));
  const previous = process.env.RELAY_AGENT_CONFIG;
  try {
    const path = join(dir, "agent.json");
    process.env.RELAY_AGENT_CONFIG = path;
    writeFileSync(path, "[]");
    assert.throws(() => loadConfig(), /JSON object/);
    writeFileSync(path, JSON.stringify({ provider: "openai;sh", model: "gpt-4o-mini" }));
    assert.throws(() => loadConfig(), /unsupported/);
    writeFileSync(path, JSON.stringify({ provider: "toString", model: "gpt-4o-mini" }));
    assert.throws(() => loadConfig(), /unsupported/);
  } finally {
    if (previous === undefined) delete process.env.RELAY_AGENT_CONFIG;
    else process.env.RELAY_AGENT_CONFIG = previous;
    rmSync(dir, { recursive: true, force: true });
  }
});

test("Relay Pi runtime config avoids a personal Pi directory used as XDG_CONFIG_HOME", () => {
  const userHome = "/home/alice";
  assert.equal(
    relayRuntimeConfigDirectory(join(userHome, ".pi", "agent"), userHome),
    join(userHome, ".config", "relay"),
  );
});

test("Relay Pi runtime config honors an independent absolute XDG_CONFIG_HOME", () => {
  const userHome = "/home/alice";
  assert.equal(
    relayRuntimeConfigDirectory(join(userHome, ".config", "custom"), userHome),
    join(userHome, ".config", "custom", "relay"),
  );
});
