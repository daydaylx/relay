import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { readSafeConfigFile } from "./safe-read.js";

test("safe config reads are bounded, rooted, and refuse secrets and symlinks", () => {
  const root = mkdtempSync(join(process.env.TMPDIR || "/tmp", "relay-safe-read-"));
  try {
    mkdirSync(join(root, "hosts"));
    writeFileSync(join(root, "hosts", "desktop.nix"), "{ services.foo.enable = true; }\n");
    writeFileSync(join(root, "hosts", "secrets.nix"), "{ password = \"not-safe-value\"; }\n");
    writeFileSync(join(root, "hosts", "oversized.md"), "x".repeat(16 * 1024 + 1));
    symlinkSync("desktop.nix", join(root, "hosts", "linked.nix"));
    assert.match(readSafeConfigFile(root, "hosts/desktop.nix").content, /services.foo.enable/);
    assert.throws(() => readSafeConfigFile(root, "../outside.nix"), /outside/);
    assert.throws(() => readSafeConfigFile(root, "hosts/secrets.nix"), /outside/);
    assert.throws(() => readSafeConfigFile(root, "hosts/linked.nix"), /symbolic links/);
    assert.throws(() => readSafeConfigFile(root, "hosts/oversized.md"), /size limit/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test("safe config reads never open the personal Pi directory", () => {
  const personalPi = join(homedir(), ".pi");
  assert.throws(() => readSafeConfigFile(personalPi, "settings.json"), /personal Pi/);
});
