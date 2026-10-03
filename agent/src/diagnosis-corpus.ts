import type { DiagnosticTopic } from "./router.js";

export const diagnosisQuestions: ReadonlyArray<{ question: string; topic: DiagnosticTopic }> = [
  { question: "Which NixOS version and kernel are running?", topic: "generations" },
  { question: "Which services are currently failed?", topic: "services" },
  { question: "Why did nginx fail to start?", topic: "services" },
  { question: "Is the Bluetooth service active?", topic: "bluetooth" },
  { question: "Is a Bluetooth radio blocked?", topic: "bluetooth" },
  { question: "Which network interfaces are up?", topic: "network" },
  { question: "Does the network interface have carrier?", topic: "network" },
  { question: "What CPU family and architecture does this host use?", topic: "hardware" },
  { question: "Which display and network adapter IDs are present?", topic: "hardware" },
  { question: "What block devices and capacities are detected?", topic: "hardware" },
  { question: "Which processes named sshd are running?", topic: "processes" },
  { question: "How many system processes match systemd?", topic: "processes" },
  { question: "Show recent warning journal metadata for nginx.service.", topic: "journal" },
  { question: "When were the last journal warnings recorded?", topic: "journal" },
  { question: "Does Relay send journal message text to the model?", topic: "journal" },
  { question: "How many Hyprland monitors and workspaces are active?", topic: "desktop" },
  { question: "Is the compositor reachable?", topic: "desktop" },
  { question: "Is the Relay managed module in sync with the running system?", topic: "configuration" },
  { question: "Which NixOS generations are active and booted?", topic: "generations" },
  { question: "What is the current system health state?", topic: "system" },
];
