import { lstatSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { isAbsolute, relative, resolve, sep } from "node:path";
import { containsLikelySecret } from "./security.js";

export type OwnershipKind =
  | "RELAY_MANAGED"
  | "NIXOS_CONFIGURATION"
  | "DIRECT_USER_CONFIG"
  | "HOME_MANAGER_GENERATED"
  | "GENERATED_NIX_STORE"
  | "GENERATED_RUNTIME"
  | "USER_CONFIG_SYMLINK"
  | "EXTERNAL_SYMLINK"
  | "UNKNOWN"
  | "MISSING"
  | "BLOCKED";

export interface OwnershipResult {
  schema_version: 1;
  requested_path: string;
  path: string | null;
  resolved_path: string | null;
  kind: OwnershipKind;
  owner: string;
  authority: string;
  writable: boolean;
  evidence: string[];
}

function inside(path: string, root: string): boolean {
  const rel = relative(root, path);
  return rel === "" || (!rel.startsWith(`..${sep}`) && rel !== ".." && !isAbsolute(rel));
}

function block(requested: string, path: string | null, evidence: string): OwnershipResult {
  return { schema_version: 1, requested_path: requested, path, resolved_path: null, kind: "BLOCKED", owner: "unknown", authority: "none", writable: false, evidence: [evidence] };
}

/** Classifies file ownership from path and filesystem metadata only. It never reads file contents. */
export function resolveOwnership(configRoot: string, requestedPath: string): OwnershipResult {
  const configuredRoot = resolve(configRoot);
  const root = existingRealpath(configuredRoot) ?? configuredRoot;
  const configuredHome = resolve(process.env.HOME || homedir());
  const home = existingRealpath(configuredHome) ?? configuredHome;
  const requested = requestedPath.trim();
  const secretPath = /(^|[\\/._-])(secret|secrets|credential|credentials|token|tokens|password|passwords|id_rsa|id_ed25519)([\\/._-]|$)/i.test(requested);
  if (!requested || requested.length > 512 || requested.includes("\0") || containsLikelySecret(requested) || secretPath) {
    return block(requested.slice(0, 512), null, "empty, oversized, or secret-like path refused");
  }

  const candidate = resolve(isAbsolute(requested) ? requested : resolve(root, requested));
  const configHome = resolve(home, ".config");
  const systemRuntime = "/run/current-system";
  if (candidate.split(sep).some((part) => part === ".pi") ||
      ![root, configHome, systemRuntime].some((allowedRoot) => inside(candidate, allowedRoot))) {
    return block(requested, candidate, "path is outside Relay's inspectable configuration, user config, or generated runtime roots");
  }

  const rel = relative(root, candidate).split(sep).join("/");
  if (rel === "relay/managed.nix") {
    let info: ReturnType<typeof lstatSync> | undefined;
    try { info = lstatSync(candidate); } catch { /* the managed file may not have been initialized yet */ }
    if (info?.isSymbolicLink() || (info && !info.isFile())) {
      return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: null, kind: "UNKNOWN", owner: "conflict", authority: "none", writable: false, evidence: ["managed target exists with an unexpected type"] };
    }
    const managedParent = existingRealpath(resolve(candidate, ".."));
    if (managedParent && !inside(managedParent, root)) {
      return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: managedParent, kind: "BLOCKED", owner: "unknown", authority: "none", writable: false, evidence: ["managed target parent resolves outside the configured NixOS source tree"] };
    }
    return {
      schema_version: 1, requested_path: requested, path: candidate, resolved_path: candidate,
      kind: "RELAY_MANAGED", owner: "Relay", authority: "Relay Core typed candidate workflow",
      writable: true,
      evidence: ["exact managed boundary path", info ? "regular file" : "managed target does not exist yet"],
    };
  }

  let info: ReturnType<typeof lstatSync>;
  try { info = lstatSync(candidate); }
  catch {
    return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: null, kind: "MISSING", owner: "unknown", authority: "none", writable: false, evidence: ["path does not exist"] };
  }

  let resolvedPath: string;
  try { resolvedPath = realpathSync(candidate); }
  catch {
    return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: null, kind: "UNKNOWN", owner: "broken symlink", authority: "none", writable: false, evidence: ["path cannot be resolved"] };
  }
  const isLink = info.isSymbolicLink();
  const generatedStorePath = inside(resolvedPath, "/nix/store");
  const homeManagerPath = resolvedPath.includes(`${sep}home-manager-files${sep}`) || resolvedPath.includes(`${sep}home-manager-generation${sep}`);
  const escapesConfigRoot = inside(candidate, root) && !inside(resolvedPath, root);
  const escapesUserConfig = inside(candidate, configHome) && !inside(resolvedPath, configHome) && !generatedStorePath && !homeManagerPath;
  if (escapesConfigRoot || escapesUserConfig) {
    return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: resolvedPath, kind: "EXTERNAL_SYMLINK", owner: "external target", authority: "unresolved", writable: false, evidence: ["path traverses a symlink outside its configured ownership root"] };
  }
  if (inside(candidate, systemRuntime) || inside(resolvedPath, systemRuntime)) {
    return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: resolvedPath, kind: "GENERATED_RUNTIME", owner: "NixOS system generation", authority: "NixOS configuration source", writable: false, evidence: ["path belongs to the active system runtime"] };
  }
  if (inside(candidate, configHome) && homeManagerPath) {
    return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: resolvedPath, kind: "HOME_MANAGER_GENERATED", owner: "Home Manager output", authority: "source unresolved", writable: false, evidence: ["Home Manager output marker in resolved path"] };
  }
  if (inside(candidate, configHome) && generatedStorePath) {
    return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: resolvedPath, kind: "GENERATED_NIX_STORE", owner: "Nix store output", authority: "source unresolved", writable: false, evidence: ["user config resolves into the immutable Nix store"] };
  }
  if (isLink) {
    if (inside(resolvedPath, root)) {
      return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: resolvedPath, kind: "USER_CONFIG_SYMLINK", owner: "configuration source at resolved path", authority: "source owner unresolved", writable: false, evidence: ["symlink points into the configured NixOS source tree"] };
    }
    return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: resolvedPath, kind: "EXTERNAL_SYMLINK", owner: "external target", authority: "unresolved", writable: false, evidence: ["symlink target is outside the inspected configuration roots"] };
  }
  if (inside(candidate, root)) {
    return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: resolvedPath, kind: "NIXOS_CONFIGURATION", owner: "NixOS flake", authority: "read-only outside relay/managed.nix", writable: false, evidence: [info.isFile() ? "regular source file" : info.isDirectory() ? "source directory" : "non-regular source entry"] };
  }
  if (inside(candidate, configHome)) {
    const uid = process.getuid?.();
    const owner = uid !== undefined && info.uid === uid ? "current user" : `uid:${info.uid}`;
    return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: resolvedPath, kind: "DIRECT_USER_CONFIG", owner, authority: "File Transaction Layer not yet available", writable: false, evidence: [info.isFile() ? "regular user config file" : info.isDirectory() ? "user config directory" : "non-regular user config entry", "persistent writes are disabled until file transactions authorize this path"] };
  }
  return { schema_version: 1, requested_path: requested, path: candidate, resolved_path: resolvedPath, kind: "UNKNOWN", owner: "unknown", authority: "none", writable: false, evidence: ["no ownership rule matched"] };
}

function existingRealpath(path: string): string | undefined {
  try { return realpathSync(path); }
  catch { return undefined; }
}
