export function renderSystemContext(context: Record<string, unknown>): string {
  const machine = record(context.machine_identity);
  const configuration = record(context.configuration_identity);
  const live = record(context.live_snapshot);
  const desktop = record(live.desktop);
  const ownership = Array.isArray(context.ownership) ? context.ownership.map(record) : [];
  const lines = [
    `Relay System Context · ${text(machine.hostname) ?? "unknown host"}`,
    `System: ${[text(machine.os_name), text(machine.os_version)].filter(Boolean).join(" ") || "unknown"}`,
    `Architecture: ${text(machine.architecture) ?? "unknown"} · Kernel: ${text(machine.kernel) ?? "unknown"}`,
    `Generation: active ${number(live.active_generation) ?? "unknown"} · booted ${number(live.booted_generation) ?? "unknown"}`,
    `Health: ${Array.isArray(live.failed_units) && live.failed_units.length ? `${live.failed_units.length} failed unit(s)` : "no failed units reported"}`,
    `Desktop: ${text(live.desktop_session) ?? "unknown session"}${text(desktop.version) ? ` · Hyprland ${text(desktop.version)}` : ""}`,
    `Configuration: ${text(configuration.configuration_revision) ?? "revision unknown"} · nixpkgs ${text(configuration.nixpkgs_revision) ?? "revision unknown"}`,
    `Managed module: ${text(configuration.managed_module) ?? "unknown"}`,
    "Ownership:",
    ...ownership.map((item) => `  ${text(item.kind) ?? "UNKNOWN"}: ${text(item.owner) ?? "unknown"} · ${item.writable === true ? "Relay Core typed changes only" : "read-only"}`),
    `Observed: ${text(context.observed_at) ?? "unknown"}`,
  ];
  return `${lines.join("\n")}\n`;
}

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function text(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function number(value: unknown): number | undefined {
  return Number.isSafeInteger(value) && (value as number) >= 0 ? value as number : undefined;
}
