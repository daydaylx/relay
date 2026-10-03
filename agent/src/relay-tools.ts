import type { AgentTool } from "@earendil-works/pi-agent-core";
import { Type } from "typebox";
import { RelayBridge } from "./bridge.js";

function result(modelData: unknown, details: unknown = modelData) {
  const text = JSON.stringify(modelData);
  if (Buffer.byteLength(text) > 24 * 1024) {
    return { content: [{ type: "text" as const, text: "Relay returned too much data. Narrow the request and try again." }], details: {} };
  }
  return { content: [{ type: "text" as const, text }], details };
}

function object(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

function systemSummary(value: unknown): Record<string, unknown> {
  const data = object(value);
  const summary: Record<string, unknown> = {};
  const copyText = (key: string, pattern: RegExp) => { if (typeof data[key] === "string" && pattern.test(data[key] as string)) summary[key] = data[key]; };
  const copyNumber = (key: string) => { if (Number.isSafeInteger(data[key]) && (data[key] as number) >= 0) summary[key] = data[key]; };
  copyText("hostname", /^[A-Za-z0-9_.-]{1,128}$/);
  if (data.os_name === "NixOS") summary.os_name = data.os_name;
  copyText("os_version", /^[0-9A-Za-z.+_-]{1,64}$/);
  copyText("kernel", /^[0-9A-Za-z.+_-]{1,128}$/);
  copyNumber("active_generation");
  copyNumber("booted_generation");
  if (Array.isArray(data.failed_units)) summary.failed_units = data.failed_units.filter((unit): unit is string => typeof unit === "string" && /^[A-Za-z0-9_.:@-]{1,128}$/.test(unit)).slice(0, 64);
  copyText("desktop_session", /^[A-Za-z0-9_.:-]{1,128}$/);
  if (["in-sync", "diverged", "unpublished"].includes(String(data.managed_module))) summary.managed_module = data.managed_module;
  if (typeof data.unresolved_change === "string" && /^[A-Za-z0-9._:-]{1,128}$/.test(data.unresolved_change)) summary.unresolved_change = data.unresolved_change;
  return summary;
}

function planSummary(value: unknown): Record<string, unknown> {
  const data = object(value);
  return Object.fromEntries(["id", "risk", "applicable", "reboot_components", "inhibitors"]
    .filter((key) => key in data).map((key) => [key, data[key]]));
}

function diagnosticSummary(topic: string, value: unknown): unknown {
  const data = object(value);
  if (topic === "network") {
    const interfaces = Array.isArray(data.interfaces) ? data.interfaces.slice(0, 32).map(object) : [];
    return { rfkill_available: data.rfkill_available === true, wlan_soft_blocked_count: safeCount(data.wlan_soft_blocked_count), wlan_hard_blocked_count: safeCount(data.wlan_hard_blocked_count), interfaces: interfaces.filter((item) => typeof item.name === "string" && /^[A-Za-z0-9_.-]{1,64}$/.test(item.name)).map((item) => ({ name: item.name, type: typeof item.type === "string" && /^\d{1,5}$/.test(item.type) ? item.type : "unknown", state: ["up", "down", "dormant", "notpresent", "lowerlayerdown", "testing", "unknown"].includes(String(item.state)) ? item.state : "unknown", carrier: typeof item.carrier === "boolean" ? item.carrier : null })) };
  }
  if (topic === "bluetooth") return { service_active: typeof data.service_active === "boolean" ? data.service_active : null, rfkill_available: data.rfkill_available === true, rfkill_adapter_count: safeCount(data.rfkill_adapter_count), controller_count: safeCount(data.controller_count), soft_blocked_count: safeCount(data.soft_blocked_count), hard_blocked_count: safeCount(data.hard_blocked_count) };
  if (topic === "hardware") {
    const pci = Array.isArray(data.pci_adapters) ? data.pci_adapters.slice(0, 32).map(object).filter((item) => ["display", "network"].includes(String(item.category)) && typeof item.vendor_id === "string" && /^0x[0-9a-fA-F]{4}$/.test(item.vendor_id) && typeof item.device_id === "string" && /^0x[0-9a-fA-F]{4}$/.test(item.device_id)).map((item) => ({ category: item.category, vendor_id: item.vendor_id, device_id: item.device_id })) : [];
    const blocks = Array.isArray(data.block_devices) ? data.block_devices.slice(0, 32).map(object).filter((item) => typeof item.name === "string" && /^[A-Za-z0-9_.-]{1,64}$/.test(item.name)).map((item) => ({ name: item.name, size_bytes: safeCount(item.size_bytes), read_only: item.read_only === true })) : [];
    return { architecture: typeof data.architecture === "string" && /^[A-Za-z0-9_-]{1,32}$/.test(data.architecture) ? data.architecture : "unknown", cpu_vendor_id: typeof data.cpu_vendor_id === "string" && /^[A-Za-z0-9_-]{1,32}$/.test(data.cpu_vendor_id) ? data.cpu_vendor_id : null, cpu_family: safeCount(data.cpu_family), cpu_model_id: safeCount(data.cpu_model_id), cpu_count: safeCount(data.cpu_count), pci_adapters: pci, block_devices: blocks };
  }
  if (topic === "processes") return { processes: Array.isArray(data.processes) ? data.processes.slice(0, 64).map(object).filter((item) => typeof item.name === "string" && /^[A-Za-z0-9_.-]{1,15}$/.test(item.name)).map((item) => ({ name: item.name, count: safeCount(item.count) })) : [] };
  if (topic === "journal") {
    const records = Array.isArray(data.records) ? data.records.slice(0, 50).map(object) : [];
    return { available: data.available === true, reason: ["permission_denied", "unavailable", "output_limit"].includes(String(data.reason)) ? data.reason : null, contains_message_text: false, records: records.map((item) => ({ timestamp_usec: typeof item.timestamp_usec === "number" && Number.isSafeInteger(item.timestamp_usec) ? item.timestamp_usec : null, unit: typeof item.unit === "string" && /^[A-Za-z0-9_.:@-]{1,128}$/.test(item.unit) ? item.unit : null, priority: typeof item.priority === "number" && Number.isInteger(item.priority) && item.priority >= 0 && item.priority <= 4 ? item.priority : null, message_id: typeof item.message_id === "string" && /^[a-fA-F0-9]{1,64}$/.test(item.message_id) ? item.message_id : null })) };
  }
  if (topic === "desktop") return { available: data.available === true, monitor_count: safeCount(data.monitor_count), workspace_count: safeCount(data.workspace_count), window_count: safeCount(data.window_count) };
  if (topic === "system" || topic === "configuration") return systemSummary(data);
  if (topic === "services") {
    const unhealthy = Array.isArray(data.unhealthy_units) ? data.unhealthy_units.filter((unit): unit is string => typeof unit === "string" && /^[A-Za-z0-9_.:@-]{1,128}$/.test(unit)).slice(0, 64) : [];
    const units = Array.isArray(data.units) ? data.units.slice(0, 50).map(object).filter((item) => typeof item.unit === "string" && /^[A-Za-z0-9_.:@-]{1,128}\.service$/.test(item.unit)).map((item) => ({ unit: item.unit, active: ["active", "reloading", "inactive", "failed", "activating", "deactivating", "maintenance", "unknown"].includes(String(item.active)) ? item.active : "unknown", sub: typeof item.sub === "string" && /^[A-Za-z0-9_-]{1,32}$/.test(item.sub) ? item.sub : "unknown" })) : [];
    return { system_state: ["running", "degraded", "starting", "stopping", "maintenance", "initializing", "offline", "unknown"].includes(String(data.system_state)) ? data.system_state : "unknown", unhealthy_units: unhealthy, units };
  }
  if (topic === "generations") return systemSummary(data);
  return {};
}

function safeCount(value: unknown): number {
  return Number.isSafeInteger(value) && (value as number) >= 0 ? value as number : 0;
}

function safeUnits(value: unknown): unknown[] {
  return Array.isArray(value) ? value.slice(0, 100).map(object)
    .filter((item) => typeof item.unit === "string" && /^[A-Za-z0-9_.:@-]{1,128}\.service$/.test(item.unit))
    .map((item) => ({ unit: item.unit, load: ["loaded", "not-found", "error", "masked", "stub", "merged", "unknown"].includes(String(item.load)) ? item.load : "unknown", active: ["active", "reloading", "inactive", "failed", "activating", "deactivating", "maintenance", "unknown"].includes(String(item.active)) ? item.active : "unknown", sub: typeof item.sub === "string" && /^[A-Za-z0-9_-]{1,32}$/.test(item.sub) ? item.sub : "unknown" })) : [];
}

export function createRelayTools(bridge: RelayBridge): AgentTool[] {
  const statusTool: AgentTool = {
    name: "relay_system_status",
    label: "System Status",
    description: "Read the current Relay system summary. This tool cannot change the system.",
    parameters: Type.Object({}),
    async execute(_id, _params, signal) {
      const data = await bridge.request("status", {}, signal);
      return result(systemSummary(data), data);
    },
  };
  const healthTool: AgentTool = {
    name: "relay_system_health",
    label: "System Health",
    description: "Read failed system units and system health. This tool cannot change the system.",
    parameters: Type.Object({}),
    async execute(_id, _params, signal) {
      return result(diagnosticSummary("services", await bridge.request("health", {}, signal)));
    },
  };
  const unitParameters = Type.Object({
    filter: Type.Optional(Type.String({ maxLength: 128 })),
    limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 100 })),
  });
  const unitsTool: AgentTool<typeof unitParameters> = {
    name: "relay_list_units",
    label: "List Services",
    description: "List systemd service names and machine-readable states. Does not expose command lines or change services.",
    parameters: unitParameters,
    async execute(_id, params, signal) {
      return result(safeUnits(await bridge.request("units", { filter: params.filter ?? "", limit: params.limit ?? 50 }, signal)));
    },
  };
  const diagnosticParameters = Type.Object({
    topic: Type.Union(["system", "services", "network", "bluetooth", "hardware", "processes", "journal", "desktop", "configuration", "generations"].map((topic) => Type.Literal(topic))),
    filter: Type.Optional(Type.String({ maxLength: 64 })),
    unit: Type.Optional(Type.String({ minLength: 1, maxLength: 128 })),
    limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 50 })),
  });
  const diagnoseTool: AgentTool<typeof diagnosticParameters> = {
    name: "relay_diagnose",
    label: "Diagnose System",
    description: "Read one task-specific system context: system/configuration/generations, services, network, Bluetooth, hardware, processes, journal metadata, or desktop counts. Journal message text is never returned. This tool cannot change the system.",
    parameters: diagnosticParameters,
    async execute(_id, params, signal) {
      const topic = params.topic;
      if (topic === "system" || topic === "configuration") {
        return result(diagnosticSummary(topic, await bridge.request("status", {}, signal)));
      }
      if (topic === "generations") {
        const [status, generations] = await Promise.all([bridge.request("status", {}, signal), bridge.request("generations", {}, signal)]);
        const safeGenerations = Array.isArray(generations) ? generations.slice(-64).map(object).filter((item) => Number.isSafeInteger(item.number) && (item.number as number) >= 0).map((item) => ({ number: item.number, active: item.active === true, booted: item.booted === true })) : [];
        return result({ system: systemSummary(status), generations: safeGenerations });
      }
      if (topic === "services") {
        const [health, units] = await Promise.all([bridge.request("health", {}, signal), bridge.request("units", { limit: 50 }, signal)]);
        const data = object(health);
        const selected = safeUnits(units).slice(0, 50);
        return result({ system_state: data.system_state, unhealthy_units: data.unhealthy_units, units: selected });
      }
      const data = await bridge.request("diagnose", {
        topic,
        ...(params.filter ? { filter: params.filter } : {}),
        ...(params.unit ? { unit: params.unit } : {}),
        limit: params.limit ?? 20,
      }, signal);
      return result(diagnosticSummary(topic, data));
    },
  };
  const planParameters = Type.Object({ intent: Type.Unknown() });
  const planTool: AgentTool<typeof planParameters> = {
    name: "relay_plan_change",
    label: "Plan Relay Change",
    description: "Create a safe, unapplied Relay plan from schema-1 typed changes. The plan is not applied.",
    parameters: planParameters,
    async execute(_id, params) {
      const intent = params.intent;
      if (!intent || typeof intent !== "object" || Array.isArray(intent)) throw new Error("intent must be an object");
      const data = await bridge.request("plan", { intent });
      return result(planSummary(data), data);
    },
  };
  const showParameters = Type.Object({ change_id: Type.String({ minLength: 1, maxLength: 128 }) });
  const showTool: AgentTool<typeof showParameters> = {
    name: "relay_show_plan",
    label: "Show Plan Review",
    description: "Read the full preview, risk classification, and recovery information for an existing Relay plan.",
    parameters: showParameters,
    async execute(_id, params, signal) {
      const data = await bridge.request("show", { change_id: params.change_id }, signal);
      return result({ change_id: params.change_id, review_available: true }, data);
    },
  };
  return [statusTool, healthTool, unitsTool, diagnoseTool, planTool, showTool];
}
