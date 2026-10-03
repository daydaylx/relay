export type Route = "INSPECT" | "DIAGNOSE" | "RELAY_CHANGE" | "BLOCKED" | "DEVELOPMENT_REQUIRED";

const protectedRequest = /\b(state[ ._-]?version|partition(?:ing)?|file[ -]?systems?|luks|disk encryption|bootloader|secure[ -]?boot|trusted[ -]?users|nix daemon trust|ssh (?:access|config|foundation|service)|sshd (?:access|service)|user access|authentication|secret management|database migration|release upgrade|upgrade nixos)\b/i;
const developmentRequest = /\b(home[- ]manager|hyprland config|edit .*file|write .*service|network config|networkmanager config)\b/i;
const changeRequest = /\b(install|remove|uninstall|package|set option|enable|disable|configure|change|add|update setting)\b/i;
const diagnosticRequest = /\b(fail|failed|failure|error|broken|crash|not working|diagnos|why|journal|log|services?|unit|bluetooth|network|wifi|interfaces?|hardware|cpu|gpu|disk|block devices?|process(?:es)?|generations?|workspaces?|monitor|drift|hyprland|compositor|health|system state|managed module|configuration)\b/i;

export function routeRequest(text: string): { route: Route; reason: string } {
  if (protectedRequest.test(text)) {
    return { route: "BLOCKED", reason: "This request touches a protected system resource; Relay can inspect or plan it, but the agent will not send it for mutation." };
  }
  if (developmentRequest.test(text)) {
    return { route: "DEVELOPMENT_REQUIRED", reason: "This request needs a read-only backend that Relay Agent V1 does not provide." };
  }
  if (changeRequest.test(text)) {
    return { route: "RELAY_CHANGE", reason: "Supported system changes must be typed, planned and reviewed through Relay Core." };
  }
  if (diagnosticRequest.test(text)) {
    return { route: "DIAGNOSE", reason: "Use structured Relay status, health and service data; diagnosis does not change the system." };
  }
  return { route: "INSPECT", reason: "Read the current system state and explain what Relay can verify." };
}

export type DiagnosticTopic = "system" | "services" | "network" | "bluetooth" | "hardware" | "processes" | "journal" | "desktop" | "configuration" | "generations";

export function diagnosticTopicForRequest(text: string): DiagnosticTopic {
  if (/\bbluetooth\b/i.test(text)) return "bluetooth";
  if (/\b(journal|log|logs)\b/i.test(text)) return "journal";
  if (/\b(process(?:es)?|pid|application|app)\b/i.test(text)) return "processes";
  if (/\b(hardware|cpu|gpu|disk|block devices?|adapter)\b/i.test(text)) return "hardware";
  if (/\b(network|wifi|wi-fi|interfaces?|ethernet|internet)\b/i.test(text)) return "network";
  if (/\b(hyprland|workspaces?|monitor|display|compositor)\b/i.test(text)) return "desktop";
  if (/\b(config|configuration|drift|managed module)\b/i.test(text)) return "configuration";
  if (/\b(generations?|booted|kernel|version|nixos)\b/i.test(text)) return "generations";
  if (/\b(services?|unit|systemd|dock|failed|failure|crash|error|start)\b/i.test(text)) return "services";
  return "system";
}
