import { closeSync, constants, fstatSync, lstatSync, openSync, readFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { isAbsolute, relative, resolve, sep } from "node:path";
import { containsLikelySecret } from "./security.js";

const MAX_BYTES = 16 * 1024;
const allowedExtensions = new Set([".nix", ".md", ".toml"]);
const sensitiveName = /(?:secret|credential|auth|token|key|password|passwd|shadow|cookie|sops|age|identity)/i;

export interface SafeReadResult { path: string; bytes: number; content: string }

export function readSafeConfigFile(configRoot: string, relativePath: string): SafeReadResult {
  if (!relativePath || Buffer.byteLength(relativePath) > 512 || relativePath.includes("\\") || relativePath.includes("\0") || isAbsolute(relativePath)) {
    throw new Error("safe-read path is invalid");
  }
  const segments = relativePath.split("/");
  if (segments.some((part) => !part || part === "." || part === ".." || part.startsWith(".") || sensitiveName.test(part) || !/^[A-Za-z0-9_+.-]{1,128}$/.test(part))) {
    throw new Error("safe-read path is outside the permitted configuration files");
  }
  const personalPi = resolve(homedir(), ".pi");
  const configuredRoot = resolve(configRoot);
  const configuredRelativeToPi = relative(personalPi, configuredRoot);
  if (!configuredRelativeToPi || (!configuredRelativeToPi.startsWith(`..${sep}`) && configuredRelativeToPi !== ".." && !isAbsolute(configuredRelativeToPi))) {
    throw new Error("safe-read never accesses the personal Pi configuration directory");
  }
  const root = realpathSync(configuredRoot);
  const rootRelativeToPi = relative(personalPi, root);
  if (!rootRelativeToPi || (!rootRelativeToPi.startsWith(`..${sep}`) && rootRelativeToPi !== ".." && !isAbsolute(rootRelativeToPi))) {
    throw new Error("safe-read never accesses the personal Pi configuration directory");
  }

  const filename = segments.at(-1)!;
  const extension = filename.slice(filename.lastIndexOf("."));
  if (!allowedExtensions.has(extension) && filename !== "flake.lock") throw new Error("safe-read supports only Nix, Markdown, TOML and the flake lock file");

  let current = root;
  for (const [index, segment] of segments.entries()) {
    current = resolve(current, segment);
    const stat = lstatSync(current);
    if (stat.isSymbolicLink()) throw new Error("safe-read refuses symbolic links");
    if (index < segments.length - 1 && !stat.isDirectory()) throw new Error("safe-read path component is not a directory");
    if (index === segments.length - 1 && !stat.isFile()) throw new Error("safe-read target is not a regular file");
  }
  const descriptor = openSync(current, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  let content: string;
  try {
    const stat = fstatSync(descriptor);
    if (!stat.isFile() || stat.size > MAX_BYTES) throw new Error("safe-read target exceeds the size limit");
    content = readFileSync(descriptor, "utf8");
  } finally { closeSync(descriptor); }
  if (Buffer.byteLength(content) > MAX_BYTES) throw new Error("safe-read target exceeds the size limit");
  if (containsLikelySecret(content)) throw new Error("safe-read withheld a file that appears to contain secret material");
  return { path: segments.join("/"), bytes: Buffer.byteLength(content), content };
}
