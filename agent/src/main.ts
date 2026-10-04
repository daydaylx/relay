import { Editor, ProcessTerminal, Text, TuiMainScreen, matchesKey, type EditorTheme } from "@earendil-works/pi-tui";
import { stdin, stdout } from "node:process";
import { loadConfig, configPath, initializeConfig, relayRuntimeConfigDirectory } from "./settings.js";
import { join } from "node:path";
import { mkdirSync } from "node:fs";
import { RelayBridge } from "./bridge.js";
import { createRelayTools } from "./relay-tools.js";
import { containsLikelySecret } from "./security.js";
import { diagnosticTopicForRequest, routeRequest } from "./router.js";
import { MutationGate } from "./mutation-gate.js";
import { TaskService } from "./task-service.js";
import { PiRpcClient } from "./pi-rpc.js";
import { RelayToolBridge } from "./tool-bridge.js";
import { renderSystemContext } from "./context-command.js";

const help = `Relay system assistant

Usage:
  relay [--init-config] [--check | --pi-rpc-check]
  relay context [--json]
  relay /tasks
  relay /resume TASK_ID
  relay-agent [--init-config] [--check]

Inspect and plan with Relay. Apply requires reviewing a plan and two direct confirmations.
Type /apply <plan-id> after reviewing a plan preview; then type APPLY <plan-id> exactly.
Configuration: ${configPath()}
`;

async function main(args: string[]): Promise<number> {
  if (args.includes("--help") || args.includes("-h")) {
    stdout.write(help);
    return 0;
  }
  const modes = ["--init-config", "--check", "--pi-rpc-check", "--context"].filter((flag) => args.includes(flag));
  if (args.some((arg) => !["--init-config", "--check", "--pi-rpc-check", "--context", "--json"].includes(arg)) || modes.length > 1 || args.includes("--json") && !args.includes("--context")) {
    stdout.write(help);
    return 2;
  }
  if (args.includes("--init-config")) {
    initializeConfig();
    stdout.write(`Created Relay agent config at ${configPath()}\n`);
    return 0;
  }

  const config = loadConfig();
  const bridge = new RelayBridge({ flake: config.flake, host: config.host });
  const taskService = new TaskService();
  let requestMutationConfirmation: (details: { action: "apply" | "undo" | "recover"; taskId: string; target: string; risk: string; review: string; reviewHash: string }, signal?: AbortSignal) => Promise<boolean> = async () => false;
  const tools = createRelayTools(bridge, { task: taskService, configRoot: config.flake, confirmMutation: (details, signal) => requestMutationConfirmation(details, signal) });
  if (args.includes("--context")) {
    const contextTool = tools.find((tool) => tool.name === "relay_system_context");
    if (!contextTool) throw new Error("Relay verified system context tool is missing");
    const contextResult = await contextTool.execute("context-cli", {}, new AbortController().signal);
    const contextText = contextResult.content.filter((part) => part.type === "text").map((part) => part.text).join("\n");
    const systemContext = JSON.parse(contextText) as Record<string, unknown>;
    stdout.write(args.includes("--json") ? `${JSON.stringify(systemContext, null, 2)}\n` : renderSystemContext(systemContext));
    return 0;
  }
  if (args.includes("--check")) {
    stdout.write(JSON.stringify({
      product: "relay",
      provider: config.provider,
      model: config.model,
      flake: config.flake,
      host: config.host,
      tools: tools.map((tool) => tool.name),
      piConfigLoaded: false,
      modelCanApplyWithoutLocalConfirmation: false,
      taskRuntime: true,
      taskPersistence: true,
    }, null, 2) + "\n");
    return 0;
  }
  if (args.includes("--pi-rpc-check")) {
    const runtimeConfig = relayRuntimeConfigDirectory();
    const piRoot = join(runtimeConfig, "pi");
    const workspace = join(piRoot, "workspaces", "rpc-check");
    mkdirSync(workspace, { recursive: true, mode: 0o700 });
    const rpc = await PiRpcClient.start({
      provider: config.provider,
      model: config.model,
      taskId: `rpc-check-${process.pid}`,
      relayConfigDirectory: runtimeConfig,
      cwd: workspace,
    });
    try {
      const state = await rpc.request("get_state");
      stdout.write(JSON.stringify({
        product: "relay",
        runtime: "pi-rpc",
        provider: config.provider,
        model: config.model,
        piConfigDirectory: piRoot,
        sessionDirectory: join(piRoot, "sessions", `rpc-check-${process.pid}`),
        personalPiConfigLoaded: false,
        tools: "RPC integration check only; no system actions are exposed",
        state: state.data,
      }, null, 2) + "\n");
    } finally {
      await rpc.close();
    }
    return 0;
  }

  if (!stdin.isTTY || !stdout.isTTY) throw new Error("interactive Relay agent requires a terminal");
  const terminal = new ProcessTerminal();
  const tui = new TuiMainScreen(terminal);
  const transcript = new Text("", 0, 1);
  const header = new Text(`Relay · ${config.host} · ${config.provider}/${config.model}\nYour messages and data returned by Relay tools may go to this configured model provider. Likely credentials are blocked before sending.\nInspect, diagnose and plan with Relay. /apply, /undo and /recover show a preview and require direct confirmation.\nCtrl+C or /exit quits.\n`, 0, 1);
  tui.addChild(header);
  tui.addChild(transcript);
  const identity = (value: string) => value;
  const theme: EditorTheme = {
    borderColor: identity,
    selectList: {
      selectedPrefix: identity,
      selectedText: identity,
      description: identity,
      scrollInfo: identity,
      noMatch: identity,
    },
  };
  const editor = new Editor(tui, theme);
  tui.addChild(editor);
  tui.setFocus(editor);
  let transcriptText = "";
  let assistantText = "";
  let busy = false;
  let stopped = false;
  let rpc: PiRpcClient | undefined;
  let rpcTools: RelayToolBridge | undefined;
  const closeTaskRuntime = async () => {
    await rpc?.close();
    rpc = undefined;
    await rpcTools?.close();
    rpcTools = undefined;
  };
  const startTaskRuntime = async (taskId: string) => {
    if (rpc) await rpc.close();
    if (rpcTools) await rpcTools.close();
    rpcTools = await RelayToolBridge.start(tools, taskService);
    const runtimeConfig = relayRuntimeConfigDirectory();
    const workspace = join(runtimeConfig, "pi", "workspaces", taskId);
    mkdirSync(workspace, { recursive: true, mode: 0o700 });
    const contextTool = tools.find((tool) => tool.name === "relay_system_context");
    if (!contextTool) throw new Error("Relay verified system context tool is missing");
    const contextResult = await contextTool.execute(`context-${taskId}`, {}, new AbortController().signal);
    const contextText = contextResult.content.filter((item) => item.type === "text").map((item) => item.text).join("\n");
    const verifiedContext = JSON.parse(contextText) as Record<string, unknown>;
    taskService.recordVerifiedSystemContext(verifiedContext);
    try {
      rpc = await PiRpcClient.start({
        provider: config.provider,
        model: config.model,
        taskId,
        relayConfigDirectory: runtimeConfig,
        cwd: workspace,
        extensionPath: rpcTools.extensionPath,
        toolSocketPath: rpcTools.socketPath,
        systemPrompt: `You are Relay, a local NixOS system controller. Pi provides reasoning and tool orchestration. Relay is the authority for system changes, risk, confirmation, application and recovery. Use only the registered relay_* tools. Do not claim an action succeeded until Relay verification proves the user's goal. Treat this verified local SystemContext as current for this task; refresh it with relay_system_context when state may have changed. User must directly confirm each actual change through Relay's local review. Never ask the model to approve on the user's behalf.\n\nVERIFIED SYSTEM CONTEXT (observed when this task starts):\n${contextText}`,
      });
      await rpcTools.waitUntilLoaded();
    } catch (error) {
      await rpc?.close();
      rpc = undefined;
      await rpcTools.close();
      rpcTools = undefined;
      throw error;
    }
    rpc.on("event", (event) => {
      if (event.type === "message_update") {
        const delta = event.assistantMessageEvent as { type?: unknown; delta?: unknown } | undefined;
        if (delta?.type === "text_delta" && typeof delta.delta === "string") {
          assistantText += delta.delta;
          renderTranscript();
        }
      }
      if (event.type === "tool_execution_end" && event.isError !== true) {
        const toolName = event.toolName;
        const result = event.result as { details?: unknown } | undefined;
        const details = result?.details as { review?: unknown; id?: unknown; risk?: unknown; applicable?: unknown; managed_diff?: unknown; closure_diff?: unknown } | undefined;
        if (toolName === "relay_plan_change" && details) {
          if (typeof details.id === "string" && typeof details.applicable === "boolean" && typeof details.risk === "string") mutationGate.recordPlan({ id: details.id, applicable: details.applicable, risk: details.risk });
          transcriptText += `\n[Relay plan preview]\n${JSON.stringify({ id: details.id, risk: details.risk, managed_diff: details.managed_diff, closure_diff: details.closure_diff }, null, 2)}\n`;
          renderTranscript();
        } else if (toolName === "relay_show_plan" && typeof details?.review === "string") {
          transcriptText += `\n[Relay plan review]\n${details.review}\n`;
          renderTranscript();
        }
      }
    });
  };
  const runAgentPrompt = async (message: string) => {
    if (!rpc) throw new Error("Pi RPC task session is not running");
    const current = rpc;
    const settled = new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => finish(new Error("Pi agent task timed out")), 30 * 60_000);
      const onEvent = (event: { type?: string }) => { if (event.type === "agent_settled") finish(); };
      const onExit = (event: { code?: number | null; signal?: string | null }) => finish(new Error(`Pi RPC exited during task (${event.signal ?? event.code})`));
      const finish = (error?: Error) => {
        clearTimeout(timeout);
        current.removeListener("event", onEvent);
        current.removeListener("exit", onExit);
        error ? reject(error) : resolve();
      };
      current.on("event", onEvent);
      current.once("exit", onExit);
    });
    void settled.catch(() => undefined);
    try { await current.prompt(message); }
    catch (error) { await current.close(); throw error; }
    await settled;
    await rpcTools?.close();
  };
  const mutationGate = new MutationGate();
  let pendingApply: { phrase: string; action: "apply" | "undo" | "recover"; taskId: string; resolve: (approved: boolean) => void; signal?: AbortSignal } | undefined;
  const renderTranscript = () => transcript.setText(transcriptText + (assistantText ? `\nRelay: ${assistantText}` : ""));
  const cancelPendingApply = () => {
    const pending = pendingApply;
    pendingApply = undefined;
    pending?.resolve(false);
  };
  taskService.subscribe((event) => {
    if (["task_started", "confirmation_required", "applying", "verification", "task_continuing", "completed", "blocked", "failed", "cancelled"].includes(event.type)) {
      transcriptText += `\n[Task ${event.task_id.slice(0, 8)} · ${event.state}] ${event.type}\n`;
      renderTranscript();
    }
  });
  requestMutationConfirmation = (details, signal) => {
    if (stopped || signal?.aborted || taskService.current().id !== details.taskId) return Promise.resolve(false);
    let phrase: string | undefined;
    if (details.action === "apply") {
      mutationGate.recordPlan({ id: details.target, applicable: true, risk: details.risk, reviewHash: details.reviewHash });
      phrase = mutationGate.requestApply(details.target);
    } else if (details.action === "undo") {
      phrase = mutationGate.requestUndo({ change_id: details.target, review: details.review, reviewHash: details.reviewHash });
    } else {
      const targets = details.target.split(" ").filter(Boolean);
      phrase = mutationGate.requestRecover(targets.map((id) => ({ id })), details.reviewHash)?.phrase;
    }
    if (!phrase) return Promise.resolve(false);
    transcriptText += `\n[Review · ${details.action} · ${details.risk} · task ${details.taskId.slice(0, 8)}]\n${details.review}\n\nRelay: Diese konkrete Core-Aktion ausführen? Tippe exakt: ${phrase}\n`;
    assistantText = "";
    renderTranscript();
    editor.disableSubmit = false;
    return new Promise<boolean>((resolve) => {
      pendingApply = { phrase, action: details.action, taskId: details.taskId, resolve, signal };
      signal?.addEventListener("abort", () => {
        if (pendingApply?.phrase === phrase) {
          pendingApply = undefined;
          resolve(false);
        }
      }, { once: true });
    });
  };
  const executeConfirmed = (action: "apply" | "undo" | "recover", params: Record<string, unknown>) => {
    busy = true;
    editor.disableSubmit = true;
    transcriptText += `\nRelay: running ${action} through the confirmed Relay Core workflow...\n`;
    renderTranscript();
    tui.stop();
    void bridge.request(action, params).then((data) => {
      transcriptText += `\nRelay Core result: ${JSON.stringify(data)}\n`;
      if (action === "apply" || action === "undo") mutationGate.clearPlan();
      renderTranscript();
    }).catch((error: unknown) => {
      transcriptText += `\nRelay ${action} failed: ${error instanceof Error ? error.message : "unknown error"}\n`;
      renderTranscript();
    }).finally(() => {
      busy = false;
      editor.disableSubmit = false;
      if (!stopped) {
        tui.start();
        tui.setFocus(editor);
      }
    });
  };
  editor.onSubmit = (text) => {
    const input = text.trim();
    if (!input || (busy && !pendingApply)) return;
    editor.addToHistory(input);
    editor.setText("");
    if (pendingApply) {
      const pending = pendingApply;
      pendingApply = undefined;
      const confirmation = mutationGate.consume(input);
      const approved = confirmation.kind === "authorized" && confirmation.action === pending.action && !pending.signal?.aborted && taskService.current().id === pending.taskId;
      if (!approved && confirmation.kind !== "authorized") transcriptText += "\nRelay: Bestätigung abgebrochen; es wurde nichts angewendet.\n";
      pending.resolve(approved);
      editor.disableSubmit = true;
      renderTranscript();
      return;
    }
    if (input === "/exit" || input === "/quit") {
      stopped = true;
      void rpc?.request("abort").catch(() => undefined);
      void closeTaskRuntime();
      taskService.cancel();
      cancelPendingApply();
      tui.stop();
      return;
    }
    if (input === "/tasks") {
      try {
        const tasks = taskService.listTasks().slice(-30).map((task) => ({ id: task.id, state: task.state, goal: task.goal, updated_at: task.events.at(-1)?.timestamp }));
        transcriptText += `\n[Relay tasks]\n${JSON.stringify(tasks, null, 2)}\nUse /resume <task-id> to continue a nonterminal task.\n`;
      } catch (error) {
        transcriptText += `\nRelay task list unavailable: ${error instanceof Error ? error.message : "invalid task journal"}\n`;
      }
      renderTranscript();
      return;
    }
    const resumeCommand = /^\/resume ([0-9a-f-]{36})$/i.exec(input);
    if (resumeCommand) {
      try {
        const task = taskService.resume(resumeCommand[1]);
        busy = true;
        editor.disableSubmit = true;
        transcriptText += `\n[Resuming task ${task.id}]\n`;
        renderTranscript();
        void startTaskRuntime(task.id).then(() => runAgentPrompt(`Resume this Relay task using the persisted, bounded checkpoint. Do not assume any pending confirmation remains valid. If the checkpoint says Core recovery is required, explain that and stop before any mutation.\n${taskService.contextSummary()}`)).then(() => {
          const current = taskService.current();
          if (!["completed", "blocked", "failed", "cancelled"].includes(current.state)) taskService.block("agent_stopped_without_verified_goal");
        }).catch((error: unknown) => {
          taskService.fail("provider_or_task_resume_failed");
          transcriptText += `\nRelay resume failed: ${error instanceof Error ? error.message : "unknown error"}\n`;
        }).finally(() => {
          busy = false;
          editor.disableSubmit = false;
          renderTranscript();
          if (!stopped) tui.setFocus(editor);
        });
      } catch (error) {
        transcriptText += `\nRelay could not resume the task: ${error instanceof Error ? error.message : "invalid task"}\n`;
        renderTranscript();
      }
      return;
    }
    if (mutationGate.hasConfirmation()) {
      const confirmation = mutationGate.consume(input);
      if (confirmation.kind === "cancelled") {
        transcriptText += "\nRelay: operation cancelled; confirmation text did not match.\n";
        renderTranscript();
        return;
      }
      if (confirmation.kind === "authorized") {
        executeConfirmed(confirmation.action, confirmation.params);
      }
      return;
    }
    if (input === "/undo") {
      busy = true;
      editor.disableSubmit = true;
      void bridge.request("undo_preview").then((data) => {
        const preview = data as { change_id?: unknown; review?: unknown };
        const phrase = mutationGate.requestUndo(preview);
        if (!phrase) throw new Error("Relay returned an invalid undo preview");
        transcriptText += `\n[Undo preview]\n${preview.review}\n`;
        transcriptText += `\nRelay: to undo this exact change, type exactly: ${phrase}\n`;
        renderTranscript();
      }).catch((error: unknown) => {
        transcriptText += `\nRelay undo preview failed: ${error instanceof Error ? error.message : "unknown error"}\n`;
        renderTranscript();
      }).finally(() => {
        busy = false;
        editor.disableSubmit = false;
      });
      return;
    }
    if (input === "/recover") {
      busy = true;
      editor.disableSubmit = true;
      void bridge.request("recover_preview").then((data) => {
        if (!Array.isArray(data)) throw new Error("Relay returned an invalid recovery preview");
        if (data.length === 0) {
          transcriptText += "\nRelay: there are no pending Relay changes to recover.\n";
          renderTranscript();
          return;
        }
        const confirmation = mutationGate.requestRecover(data);
        if (!confirmation) throw new Error("Relay returned an invalid recovery target");
        transcriptText += `\n[Recovery preview]\n${JSON.stringify(data, null, 2)}\n`;
        const { phrase } = confirmation;
        transcriptText += `\nRelay: recovery uses recorded source/runtime evidence and may complete a verified change or roll it back. To authorize this exact set, type exactly: ${phrase}\n`;
        renderTranscript();
      }).catch((error: unknown) => {
        transcriptText += `\nRelay recovery preview failed: ${error instanceof Error ? error.message : "unknown error"}\n`;
        renderTranscript();
      }).finally(() => {
        busy = false;
        editor.disableSubmit = false;
      });
      return;
    }
    const applyCommand = /^\/apply ([A-Za-z0-9._:-]{1,128})$/.exec(input);
    if (applyCommand) {
      const changeId = applyCommand[1];
      const phrase = mutationGate.requestApply(changeId);
      const risk = mutationGate.planRisk(changeId);
      if (!phrase || !risk) {
        transcriptText += "\nRelay: that plan is unavailable or is not applicable. Create and review a fresh plan first.\n";
      } else {
        transcriptText += `\nRelay: review the exact preview above. To authorize this ${risk} change, type exactly: ${phrase}\n`;
      }
      renderTranscript();
      return;
    }
    if (containsLikelySecret(input)) {
      transcriptText += "\nRelay: this message looks like it may contain a credential. Remove it before sending; Relay did not contact the model.\n";
      renderTranscript();
      return;
    }
    const route = routeRequest(input);
    const diagnosticTopic = diagnosticTopicForRequest(input);
    transcriptText += `\n[Route hint: ${route.route}] ${route.reason}\n`;
    let currentTask;
    try {
      currentTask = taskService.start(input);
    } catch (error) {
      transcriptText += `Relay: ${error instanceof Error ? error.message : "could not start a task"}\n`;
      renderTranscript();
      return;
    }
    busy = true;
    editor.disableSubmit = true;
    transcriptText += `\n[Task ${currentTask.id}]\nYou: ${input}\nRelay:`;
    assistantText = "";
    renderTranscript();
    void startTaskRuntime(currentTask.id).then(() => runAgentPrompt(`[Relay task ${currentTask.id}; route hint: ${route.route}; ${route.reason} Suggested first diagnostic topic: ${diagnosticTopic}. The route is a hint, not authorization or a hard security decision.] Goal: ${input}`)).then(() => {
      const current = taskService.current();
      if (!["completed", "blocked", "failed", "cancelled"].includes(current.state)) taskService.block("agent_stopped_without_verified_goal");
    }).catch((error: unknown) => {
      try { taskService.fail("provider_or_agent_error"); } catch { /* a terminal task remains authoritative */ }
      transcriptText += `\nRelay request failed: ${error instanceof Error ? error.message : "unknown error"}\n`;
      renderTranscript();
    }).finally(() => {
      busy = false;
      editor.disableSubmit = false;
      if (!stopped) tui.setFocus(editor);
    });
  };
  tui.addInputListener((data) => {
    if (matchesKey(data, "ctrl+c")) {
      stopped = true;
      void rpc?.request("abort").catch(() => undefined);
      void closeTaskRuntime();
      taskService.cancel();
      cancelPendingApply();
      tui.stop();
      return { consume: true };
    }
    return undefined;
  });
  process.once("SIGTERM", () => {
    stopped = true;
    void rpc?.request("abort").catch(() => undefined);
    void closeTaskRuntime();
    tui.stop();
  });
  tui.start();
  await new Promise<void>((resolve) => process.once("exit", () => resolve()));
  return 0;
}

main(process.argv.slice(2)).then(
  (code) => { process.exitCode = code; },
  (error: unknown) => {
    const message = error instanceof Error ? error.message : "Unknown Relay agent error";
    console.error(`relay-agent: ${message}`);
    process.exitCode = 1;
  },
);
