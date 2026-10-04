import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { resolveOwnership } from "./ownership.js";

test("ownership resolver distinguishes managed NixOS, direct user config and Home Manager outputs", () => {
  const root = mkdtempSync(join(tmpdir(), "relay-ownership-"));
  const previousHome = process.env.HOME;
  const configRoot = join(root, "nixos");
  const home = join(root, "home");
  const configHome = join(home, ".config");
  try {
    process.env.HOME = home;
    mkdirSync(join(configRoot, "relay"), { recursive: true });
    writeFileSync(join(configRoot, "configuration.nix"), "# fixture\n");
    mkdirSync(join(configHome, "hypr"), { recursive: true });
    writeFileSync(join(configHome, "hypr", "hyprland.conf"), "# fixture\n");
    const generated = join(root, "store", "home-manager-files", ".config", "hypr");
    mkdirSync(generated, { recursive: true });
    symlinkSync(generated, join(configHome, "hypr-generated"), "dir");

    const managed = resolveOwnership(configRoot, "relay/managed.nix");
    assert.equal(managed.kind, "RELAY_MANAGED");
    assert.equal(managed.writable, true);

    const nixos = resolveOwnership(configRoot, "configuration.nix");
    assert.equal(nixos.kind, "NIXOS_CONFIGURATION");
    assert.equal(nixos.writable, false);

    const direct = resolveOwnership(configRoot, join(configHome, "hypr", "hyprland.conf"));
    assert.equal(direct.kind, "DIRECT_USER_CONFIG");
    assert.equal(direct.writable, false);

    const homeManager = resolveOwnership(configRoot, join(configHome, "hypr-generated"));
    assert.equal(homeManager.kind, "HOME_MANAGER_GENERATED");
    assert.equal(homeManager.writable, false);
  } finally {
    if (previousHome === undefined) delete process.env.HOME;
    else process.env.HOME = previousHome;
    rmSync(root, { recursive: true, force: true });
  }
});

test("ownership resolver blocks personal Pi, secret-like and out-of-scope paths", () => {
  const root = mkdtempSync(join(tmpdir(), "relay-ownership-"));
  const previousHome = process.env.HOME;
  try {
    process.env.HOME = join(root, "home");
    const configRoot = join(root, "nixos");
    mkdirSync(join(process.env.HOME, ".config", ".pi"), { recursive: true });
    for (const path of ["../outside", join(process.env.HOME, ".config", ".pi", "settings.json"), join(process.env.HOME, ".config", "api-token")]) {
      assert.equal(resolveOwnership(configRoot, path).kind, "BLOCKED");
    }
    assert.equal(resolveOwnership(configRoot, "not-created.nix").kind, "MISSING");
  } finally {
    if (previousHome === undefined) delete process.env.HOME;
    else process.env.HOME = previousHome;
    rmSync(root, { recursive: true, force: true });
  }
});

test("ownership resolver blocks parent symlinks that escape a managed or user-config root", () => {
  const root = mkdtempSync(join(tmpdir(), "relay-ownership-links-"));
  const previousHome = process.env.HOME;
  try {
    const home = join(root, "home");
    const configRoot = join(root, "nixos");
    const outside = join(root, "outside");
    process.env.HOME = home;
    mkdirSync(join(home, ".config"), { recursive: true });
    mkdirSync(outside, { recursive: true });
    mkdirSync(configRoot, { recursive: true });
    writeFileSync(join(outside, "settings.conf"), "fixture\n");
    symlinkSync(outside, join(configRoot, "relay"), "dir");
    symlinkSync(outside, join(home, ".config", "linked"), "dir");

    assert.equal(resolveOwnership(configRoot, "relay/managed.nix").kind, "BLOCKED");
    assert.equal(resolveOwnership(configRoot, join(home, ".config", "linked", "settings.conf")).kind, "EXTERNAL_SYMLINK");
  } finally {
    if (previousHome === undefined) delete process.env.HOME;
    else process.env.HOME = previousHome;
    rmSync(root, { recursive: true, force: true });
  }
});
