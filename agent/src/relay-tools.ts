import type { AgentTool } from "@earendil-works/pi-agent-core";
import { createHash } from "node:crypto";
import { Type } from "typebox";
import { RelayBridge } from "./bridge.js";
import type { TaskService } from "./task-service.js";
import { readSafeConfigFile } from "./safe-read.js";

export interface RelayToolContext {
  task: TaskService;
  configRoot: string;
  confirmMutation: (details: { action: "apply" | "undo" | "recover"; taskId: string; target: string; risk: string; review: string; reviewHash: string }, signal?: AbortSignal) => Promise<boolean>;
}

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
  copyText("config_identity", /^sha256:[A-Za-z0-9+/=]{16,128}$/);
  copyText("configuration_revision", /^[0-9a-fA-F]{7,64}$/);
  copyText("nixpkgs_revision", /^[0-9a-fA-F]{7,64}$/);
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
    return { available: data.available === true, reason: ["permission_denied", "unavailable", "output_limit"].includes(String(data.reason)) ? data.reason : null, contains_message_text: false, records: records.map((item) => ({ timestamp_usec: typeof item.timestamp_usec === "number" && Number.isSafeInteger(item.timestamp_usec) ? item.timestamp_usec : null, unit: typeof item.unit === "string" && /^[A-Za-z0-9_.:@-]{1,128}$/.test(item.unit) ? item.unit : null, priority: typeof item.priority === "number" && Number.isInteger(item.priority) && item.priority >= 0 && item.priority <= 4 ? item.priority : null, message_id: typeof item.message_id === "string" && /^[a-fA-F0-9]{1,64}$/.test(item.message_id) ? item.message_id : null, message_kind: ["permission_denied", "missing_resource", "connection_refused", "address_conflict", "memory_pressure", "timeout", "process_crash", "service_error", "other"].includes(String(item.message_kind)) ? item.message_kind : "other" })) };
  }
  if (topic === "desktop") {
    const monitors = Array.isArray(data.monitors) ? data.monitors.slice(0, 16).map(object).filter((item) => typeof item.name === "string" && /^[A-Za-z0-9_.:-]{1,64}$/.test(item.name)).map((item) => ({ name: item.name, width: safeCount(item.width), height: safeCount(item.height), refresh_hz: typeof item.refresh_hz === "number" && Number.isFinite(item.refresh_hz) && item.refresh_hz >= 0 && item.refresh_hz <= 1000 ? item.refresh_hz : null, focused: item.focused === true, disabled: item.disabled === true })) : [];
    const workspaces = Array.isArray(data.workspaces) ? data.workspaces.slice(0, 64).map(object).filter((item) => typeof item.name === "string" && /^[A-Za-z0-9_.:+-]{0,64}$/.test(item.name)).map((item) => ({ id: Number.isSafeInteger(item.id) ? item.id : null, name: item.name, monitor: typeof item.monitor === "string" && /^[A-Za-z0-9_.:-]{0,64}$/.test(item.monitor) ? item.monitor : null, windows: safeCount(item.windows) })) : [];
    return { available: data.available === true, version: typeof data.version === "string" && /^[A-Za-z0-9_.+-]{1,64}$/.test(data.version) ? data.version : null, monitor_count: safeCount(data.monitor_count), workspace_count: safeCount(data.workspace_count), window_count: safeCount(data.window_count), monitors, workspaces };
  }
  if (topic === "system" || topic === "configuration") return systemSummary(data);
  if (topic === "package") return { name: typeof data.name === "string" && /^[A-Za-z0-9_.+-]{1,128}$/.test(data.name) ? data.name : null, available: data.available === true };
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

export function createRelayTools(bridge: RelayBridge, context?: RelayToolContext): AgentTool[] {
  const contextTool: AgentTool = {
    name: "relay_system_context",
    label: "Verified System Context",
    description: "Read Relay's structured, locally verified machine identity, current configuration identity, running generation, hardware and desktop summary, capabilities and configuration ownership. No mutation.",
    parameters: Type.Object({}),
    async execute(_id, _params, signal) {
      const [statusValue, generationsValue, hardwareValue, desktopValue] = await Promise.all([
        bridge.request("status", {}, signal),
        bridge.request("generations", {}, signal),
        bridge.request("diagnose", { topic: "hardware" }, signal),
        bridge.request("diagnose", { topic: "desktop" }, signal),
      ]);
      const status = systemSummary(statusValue);
      const generations = Array.isArray(generationsValue) ? generationsValue.slice(-32).map(object)
        .filter((item) => Number.isSafeInteger(item.number) && (item.number as number) >= 0)
        .map((item) => ({ number: item.number, active: item.active === true, booted: item.booted === true })) : [];
      const hardware = diagnosticSummary("hardware", hardwareValue);
      const desktop = diagnosticSummary("desktop", desktopValue);
      return result({
        schema_version: 1,
        observed_at: new Date().toISOString(),
        machine_identity: { hostname: status.hostname ?? null, os_name: status.os_name ?? null, os_version: status.os_version ?? null, architecture: object(hardware).architecture ?? null, kernel: status.kernel ?? null },
        configuration_identity: { config_identity: status.config_identity ?? null, configuration_revision: status.configuration_revision ?? null, nixpkgs_revision: status.nixpkgs_revision ?? null, managed_module: status.managed_module ?? null },
        live_snapshot: { active_generation: status.active_generation ?? null, booted_generation: status.booted_generation ?? null, generations, failed_units: status.failed_units ?? [], desktop_session: status.desktop_session ?? null, desktop, hardware },
        ownership: [
          { scope: "NixOS managed module", owner: "Relay Safety Core", authority: "relay/managed.nix", writable: true, condition: "Only typed Relay plans; candidate, evaluation, build, review, confirmation and verification required." },
          { scope: "Other NixOS, Home Manager, Hyprland and user configuration", owner: "unresolved", authority: "read-only in this runtime", writable: false },
        ],
        capabilities: { observe: ["system status", "generations", "health", "services", "hardware", "network", "Bluetooth", "desktop", "safe config reads", "NixOS option search", "package search"], mutate: ["typed Relay-managed NixOS plans; apply only after direct local confirmation"] },
        knowledge_provenance: "All returned facts originate from local Relay/NixOS adapters; external documentation or web research is not yet configured.",
      });
    },
  };
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
    topic: Type.Union(["system", "services", "network", "bluetooth", "hardware", "processes", "package", "journal", "desktop", "configuration", "generations"].map((topic) => Type.Literal(topic))),
    filter: Type.Optional(Type.String({ maxLength: 64 })),
    unit: Type.Optional(Type.String({ minLength: 1, maxLength: 128 })),
    limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 50 })),
  });
  const diagnoseTool: AgentTool<typeof diagnosticParameters> = {
    name: "relay_diagnose",
    label: "Diagnose System",
    description: "Read one task-specific system context: system/configuration/generations, services, network, Bluetooth, hardware, process names/counts, safe package availability, journal metadata, or desktop counts. Journal message text is never returned. This tool cannot change the system.",
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
    async execute(_id, params, signal) {
      const intent = params.intent;
      if (!intent || typeof intent !== "object" || Array.isArray(intent)) throw new Error("intent must be an object");
      context?.task.planning();
      const data = await bridge.request("plan", { intent }, signal);
      if (context) {
        const plan = object(data);
        if (typeof plan.id !== "string" || typeof plan.risk !== "string" || typeof plan.applicable !== "boolean") {
          throw new Error("Relay Core returned an invalid plan record");
        }
        context.task.planReady(plan.id, plan.risk, plan.applicable);
      }
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
  const searchParameters = Type.Object({ query: Type.String({ minLength: 1, maxLength: 128 }), limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 20 })) });
  const searchOptionTool: AgentTool<typeof searchParameters> = {
    name: "relay_search_option",
    label: "Search NixOS Options",
    description: "Search current host NixOS options by name or phrase. Only option names, types and documentation are returned; defaults and values are excluded.",
    parameters: searchParameters,
    async execute(_id, params, signal) {
      const data = object(await bridge.request("search_option", { query: params.query, limit: params.limit ?? 10 }, signal));
      return result(safeSearchResults(data));
    },
  };
  const searchPackageTool: AgentTool<typeof searchParameters> = {
    name: "relay_search_package",
    label: "Search Nix Packages",
    description: "Search packages in the configured NixOS host's nixpkgs source. Returns package names and descriptions, never performs an installation.",
    parameters: searchParameters,
    async execute(_id, params, signal) {
      const data = object(await bridge.request("search_package", { query: params.query, limit: params.limit ?? 10 }, signal));
      return result(safeSearchResults(data));
    },
  };
  const safeReadParameters = Type.Object({ path: Type.String({ minLength: 1, maxLength: 512 }) });
  const safeReadTool: AgentTool<typeof safeReadParameters> = {
    name: "relay_read_safe_config",
    label: "Read Safe Configuration",
    description: "Read a small NixOS configuration or project documentation file under the configured flake. Refuses symlinks, secret-like filenames/content, hidden paths, personal Pi data and files above 16 KiB. Read only; never writes.",
    parameters: safeReadParameters,
    async execute(_id, params, signal) {
      if (signal?.aborted) throw new Error("safe-read was cancelled");
      return result(readSafeConfigFile(context?.configRoot ?? process.cwd(), params.path));
    },
  };

  const workflowTools: AgentTool[] = [];
  if (context) {
    const applyParameters = Type.Object({ change_id: Type.String({ minLength: 1, maxLength: 128 }) });
    const applyTool: AgentTool<typeof applyParameters> = {
      name: "relay_apply_change",
      label: "Apply Confirmed Change",
      description: "Apply a reviewed change created during this task. Relay shows its exact preview and waits for direct local user confirmation before it invokes the safety core.",
      parameters: applyParameters,
      async execute(_id, params, signal) {
        const task = context.task.current();
        if (!task.change_ids.includes(params.change_id)) throw new Error("change is not part of the active task");
        const planEvent = [...task.events].reverse().find((event) => event.kind === "plan_ready" && event.payload.change_id === params.change_id);
        if (!planEvent || planEvent.payload.applicable !== true || typeof planEvent.payload.risk !== "string") {
          throw new Error("change is missing a current applicable Relay plan");
        }
        const plan = await bridge.request("show", { change_id: params.change_id }, signal);
        const details = object(plan);
        if (typeof details.review !== "string" || !details.review) throw new Error("Relay returned no review preview");
        const reviewHash = createHash("sha256").update(details.review).digest("hex");
        context.task.requestConfirmation(params.change_id, planEvent.payload.risk, reviewHash);
        const approved = await context.confirmMutation({ action: "apply", taskId: task.id, target: params.change_id, risk: planEvent.payload.risk, review: details.review, reviewHash }, signal);
        if (!approved) {
          if (!signal?.aborted && context.task.current().state === "waiting_confirmation") context.task.confirmationDeclined();
          return result({ applied: false, reason: "user_declined" });
        }
        context.task.applying(params.change_id);
        try {
          const applied = object(await bridge.request("apply", { change_id: params.change_id, confirmed: true }, signal));
          const outcome = typeof applied.outcome === "string" ? applied.outcome : "unknown";
          context.task.applySucceeded(params.change_id, outcome);
          return result({ applied: true, outcome, change_id: params.change_id });
        } catch (error) {
          context.task.fail("core_apply_failed");
          throw error;
        }
      },
    };
    workflowTools.push(applyTool);

    const discardParameters = Type.Object({ change_id: Type.String({ minLength: 1, maxLength: 128 }) });
    const discardTool: AgentTool<typeof discardParameters> = {
      name: "relay_discard_change",
      label: "Discard Unapplied Change",
      description: "Discard an unapplied candidate plan created during this task. It cannot activate a system change.",
      parameters: discardParameters,
      async execute(_id, params, signal) {
        if (!context.task.current().change_ids.includes(params.change_id)) throw new Error("change is not part of the active task");
        const discarded = object(await bridge.request("discard", { change_id: params.change_id }, signal));
        context.task.recordAction("discard", params.change_id, typeof discarded.state === "string" ? discarded.state : "discarded");
        return result({ discarded: true, change_id: params.change_id });
      },
    };
    workflowTools.push(discardTool);

    const undoTool: AgentTool<typeof discardParameters> = {
      name: "relay_undo_change",
      label: "Undo Relay Change",
      description: "Undo a completed Relay change owned by this task. Relay presents its exact undo preview and waits for direct local confirmation.",
      parameters: discardParameters,
      async execute(_id, params, signal) {
        if (!context.task.current().change_ids.includes(params.change_id)) throw new Error("change is not part of the active task");
        const preview = object(await bridge.request("undo_preview", {}, signal));
        if (preview.change_id !== params.change_id || typeof preview.review !== "string") throw new Error("Core undo preview does not match this task's change");
        const reviewHash = createHash("sha256").update(preview.review).digest("hex");
        context.task.requestConfirmation(params.change_id, "UNDO", reviewHash);
        if (!await context.confirmMutation({ action: "undo", taskId: context.task.current().id, target: params.change_id, risk: "UNDO", review: preview.review, reviewHash }, signal)) {
          if (!signal?.aborted && context.task.current().state === "waiting_confirmation") context.task.confirmationDeclined();
          return result({ applied: false, reason: "user_declined" });
        }
        context.task.applying(params.change_id);
        const undone = object(await bridge.request("undo", { change_id: params.change_id, confirmed: true }, signal));
        context.task.applySucceeded(params.change_id, typeof undone.outcome === "string" ? undone.outcome : "undone");
        return result({ undone: true, change_id: params.change_id, outcome: undone.outcome ?? "unknown" });
      },
    };
    workflowTools.push(undoTool);

    const recoverTool: AgentTool = {
      name: "relay_recover_change",
      label: "Recover Interrupted Change",
      description: "Inspect pending Relay changes, require direct local confirmation for the exact pending set, and recover using the Core journal. Never guesses whether to switch or roll back.",
      parameters: Type.Object({}),
      async execute(_id, _params, signal) {
        const preview = await bridge.request("recover_preview", {}, signal);
        if (!Array.isArray(preview) || preview.length === 0 || preview.some((item) => !context.task.current().change_ids.includes(String(object(item).id)))) {
          throw new Error("pending recovery set is empty, invalid, or not owned by this task; use Relay's explicit recovery command");
        }
        const target = preview.map((item) => String(object(item).id)).join(" ");
        const review = JSON.stringify(preview, null, 2);
        const reviewHash = createHash("sha256").update(review).digest("hex");
        context.task.requestConfirmation(preview[0] && typeof object(preview[0]).id === "string" ? object(preview[0]).id as string : "", "RECOVERY", reviewHash);
        if (!await context.confirmMutation({ action: "recover", taskId: context.task.current().id, target, risk: "RECOVERY", review, reviewHash }, signal)) {
          if (!signal?.aborted && context.task.current().state === "waiting_confirmation") context.task.confirmationDeclined();
          return result({ recovered: false, reason: "user_declined" });
        }
        context.task.applying(target);
        const recovered = await bridge.request("recover", { expected_ids: preview.map((item) => object(item).id), confirmed: true }, signal);
        context.task.applySucceeded(target, "recovered");
        return result({ recovered: true, count: Array.isArray(recovered) ? recovered.length : 0 });
      },
    };
    workflowTools.push(recoverTool);

    const verifyParameters = Type.Object({
      check: Type.Union([Type.Literal("bluetooth_ready"), Type.Literal("service_active"), Type.Literal("system_healthy"), Type.Literal("package_available")]),
      target: Type.Optional(Type.String({ minLength: 1, maxLength: 128 })),
    });
    const verifyTool: AgentTool<typeof verifyParameters> = {
      name: "relay_verify_goal",
      label: "Verify Task Goal",
      description: "Check an explicit supported outcome after a change. Only a matching structured Relay observation can complete the current task; an unsupported or irrelevant check never marks success.",
      parameters: verifyParameters,
      async execute(_id, params, signal) {
        const goal = context.task.current().goal;
        let passed = false;
        let evidence: Record<string, unknown> = {};
        let matchedGoal = false;
        if (params.check === "bluetooth_ready") {
          matchedGoal = /bluetooth|bluetooth-adapter|bluetooth.adapter/i.test(goal);
          const data = object(await bridge.request("diagnose", { topic: "bluetooth" }, signal));
          const controllerCount = safeCount(data.controller_count);
          const blocked = safeCount(data.soft_blocked_count) + safeCount(data.hard_blocked_count);
          passed = matchedGoal && data.service_active === true && controllerCount > 0 && blocked === 0;
          evidence = { service_active: data.service_active === true, controller_count: controllerCount, blocked_adapter_count: blocked };
        } else if (params.check === "service_active") {
          const target = params.target;
          matchedGoal = typeof target === "string" && goal.includes(target) && /service|dienst/i.test(goal);
          const units = await bridge.request("units", { filter: target ?? "", limit: 100 }, signal);
          const item = Array.isArray(units) ? units.map(object).find((unit) => unit.unit === target) : undefined;
          passed = matchedGoal && item?.active === "active";
          evidence = { unit: target ?? null, active: item?.active ?? "not_found" };
        } else if (params.check === "system_healthy") {
          matchedGoal = /system.*(gesund|health|status)|health.*system|systemzustand/i.test(goal);
          const health = object(await bridge.request("health", {}, signal));
          passed = matchedGoal && health.system_state === "running" && Array.isArray(health.unhealthy_units) && health.unhealthy_units.length === 0;
          evidence = { system_state: health.system_state ?? "unknown", unhealthy_unit_count: Array.isArray(health.unhealthy_units) ? health.unhealthy_units.length : null };
        } else {
          const target = params.target;
          matchedGoal = typeof target === "string" && goal.toLowerCase().includes(target.toLowerCase()) && /install|verfügbar|available|systemweit|package|paket|programm/i.test(goal);
          const data = object(await bridge.request("diagnose", { topic: "package", filter: target ?? "" }, signal));
          passed = matchedGoal && data.name === target && data.available === true;
          evidence = { executable: target ?? null, available: data.available === true };
        }
        const verified = passed && matchedGoal;
        context.task.verification(params.check, verified, evidence);
        if (verified) {
          context.task.complete(params.check);
          return { ...result({ verified: true, objective_matches_check: true, check: params.check, evidence }), terminate: true };
        }
        return result({ verified: false, objective_matches_check: matchedGoal, check: params.check, evidence, next_step: matchedGoal ? "continue_diagnosis" : "choose_goal_relevant_check" });
      },
    };
    workflowTools.push(verifyTool);
  }
  return [contextTool, statusTool, healthTool, unitsTool, diagnoseTool, searchOptionTool, searchPackageTool, ...(context ? [safeReadTool] : []), planTool, showTool, ...workflowTools];
}

function safeSearchResults(value: Record<string, unknown>): Record<string, unknown> {
  const results = Array.isArray(value.results) ? value.results.slice(0, 20).map(object).filter((item) => typeof item.name === "string" && /^[A-Za-z0-9_.+-]{1,256}$/.test(item.name)).map((item) => ({
    name: item.name,
    type: typeof item.type === "string" ? item.type.slice(0, 160) : null,
    description: typeof item.description === "string" ? item.description.slice(0, 1200) : "",
    read_only: item.read_only === true,
  })) : [];
  return { kind: value.kind === "option" || value.kind === "package" ? value.kind : "unknown", results };
}
