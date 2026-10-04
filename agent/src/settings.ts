import { mkdirSync, readFileSync, writeFileSync, chmodSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { hostname, homedir } from "node:os";

export interface AgentConfig {
  provider: string;
  model: string;
  flake: string;
  host: string;
}

function defaultConfig(): AgentConfig {
  return {
    provider: "openai",
    model: "gpt-4o-mini",
    flake: process.env.RELAY_AGENT_FLAKE || "/etc/nixos",
    host: process.env.RELAY_AGENT_HOST || hostname(),
  };
}

export function configPath(): string {
  const configHome = process.env.XDG_CONFIG_HOME || join(homedir(), ".config");
  return process.env.RELAY_AGENT_CONFIG || join(configHome, "relay", "agent.json");
}

export function relayRuntimeConfigDirectory(requestedConfigHome = process.env.XDG_CONFIG_HOME, userHome = homedir()): string {
  const defaultConfigHome = join(userHome, ".config");
  const configHome = requestedConfigHome && isAbsolute(requestedConfigHome) ? resolve(requestedConfigHome) : defaultConfigHome;
  const personalPiDirectory = resolve(userHome, ".pi");
  const relativeToPi = relative(personalPiDirectory, configHome);
  const isInsidePersonalPi = relativeToPi === "" || (!relativeToPi.startsWith("..") && !isAbsolute(relativeToPi));
  return join(isInsidePersonalPi ? defaultConfigHome : configHome, "relay");
}

export function loadConfig(): AgentConfig {
  let fileConfig: Partial<AgentConfig> = {};
  try {
    const value: unknown = JSON.parse(readFileSync(configPath(), "utf8"));
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new Error("config must be a JSON object");
    }
    fileConfig = value as Partial<AgentConfig>;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
      throw new Error(`cannot read Relay agent config: ${error instanceof Error ? error.message : "invalid config"}`);
    }
  }

  const defaults = defaultConfig();
  const config: AgentConfig = {
    provider: process.env.RELAY_AGENT_PROVIDER || fileConfig.provider || defaults.provider,
    model: process.env.RELAY_AGENT_MODEL || fileConfig.model || defaults.model,
    flake: process.env.RELAY_AGENT_FLAKE || fileConfig.flake || defaults.flake,
    host: process.env.RELAY_AGENT_HOST || fileConfig.host || defaults.host,
  };
  if (!["openai", "anthropic", "google"].includes(config.provider) || !/^[a-z0-9_.:/-]{1,128}$/i.test(config.model)) {
    throw new Error("Relay agent provider or model is unsupported");
  }
  if (!config.flake || config.flake.length > 4096 || config.flake.includes("\0") ||
      !config.host || config.host.length > 128 || /[\u0000-\u0020]/.test(config.host)) {
    throw new Error("Relay agent flake path or host is invalid");
  }
  return config;
}

export function initializeConfig(): void {
  const path = configPath();
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  try {
    writeFileSync(path, `${JSON.stringify(defaultConfig(), null, 2)}\n`, { flag: "wx", mode: 0o600 });
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "EEXIST") {
      throw new Error(`Relay agent config already exists: ${path}`);
    }
    throw error;
  }
  chmodSync(path, 0o600);
}
