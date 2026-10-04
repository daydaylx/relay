import { TaskStore, type TaskEvent, type TaskEventKind, type TaskSnapshot, type TaskState } from "./task.js";
import { containsLikelySecret } from "./security.js";

export type AgentEventKind =
  | "task_started" | "status_summary" | "tool_started" | "tool_result" | "plan_ready"
  | "confirmation_required" | "applying" | "verification" | "task_continuing"
  | "completed" | "blocked" | "failed" | "cancelled";

export interface AgentEvent {
  schema_version: 1;
  task_id: string;
  sequence: number;
  at: string;
  type: AgentEventKind;
  state: TaskState;
  payload: Record<string, unknown>;
}

export type AgentEventListener = (event: AgentEvent) => void;

const eventKinds = new Set<AgentEventKind>([
  "task_started", "status_summary", "tool_started", "tool_result", "plan_ready",
  "confirmation_required", "applying", "verification", "task_continuing", "completed",
  "blocked", "failed", "cancelled",
]);

export class TaskService {
  private readonly listeners = new Set<AgentEventListener>();
  private snapshot: TaskSnapshot | undefined;
  private toolCalls = 0;
  private readonly maxToolCalls: number;

  constructor(private readonly store = new TaskStore(), options: { maxToolCalls?: number } = {}) {
    this.maxToolCalls = options.maxToolCalls ?? 40;
  }

  subscribe(listener: AgentEventListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  start(goal: string): TaskSnapshot {
    if (this.snapshot && !["completed", "blocked", "failed", "cancelled"].includes(this.snapshot.state)) {
      throw new Error("a Relay task is already active");
    }
    this.snapshot = this.store.create(goal);
    this.toolCalls = 0;
    this.emit(this.snapshot.events[0]);
    return this.transition("investigating", "status_summary", { phase: "initial_investigation" });
  }

  resume(id: string): TaskSnapshot {
    const snapshot = this.store.read(id);
    if (["completed", "cancelled"].includes(snapshot.state)) throw new Error("task is already closed");
    if (this.snapshot && !["completed", "blocked", "failed", "cancelled"].includes(this.snapshot.state)) {
      throw new Error("a Relay task is already active");
    }
    this.snapshot = snapshot;
    this.toolCalls = snapshot.events.filter((event) => event.kind === "tool_started").length;
    if (snapshot.state === "waiting_confirmation") this.transition("blocked", "blocked", { reason: "confirmation_expired_after_restart" });
    else if (snapshot.state === "applying") this.transition("blocked", "blocked", { reason: "core_recovery_required_after_restart" });
    else if (snapshot.state === "verifying") this.transition("investigating", "task_continuing", { reason: "verification_restarted" });
    else if (["failed", "blocked", "planning", "created"].includes(snapshot.state)) this.transition("investigating", "task_continuing", { reason: "user_resumed_task" });
    return this.current();
  }

  current(): TaskSnapshot {
    if (!this.snapshot) throw new Error("there is no active Relay task");
    return this.snapshot;
  }

  listTasks(): TaskSnapshot[] { return this.store.list(); }

  beginTool(name: string): void {
    const task = this.current();
    if (["completed", "blocked", "failed", "cancelled"].includes(task.state)) throw new Error("task is not active");
    this.toolCalls += 1;
    if (this.toolCalls > this.maxToolCalls) {
      this.transition("blocked", "blocked", { reason: "tool_iteration_limit", limit: this.maxToolCalls });
      throw new Error("Relay task reached its tool-call limit");
    }
    this.record("tool_started", { name, ordinal: this.toolCalls });
  }

  toolResult(name: string, outcome: "ok" | "error", summary: Record<string, unknown> = {}): void {
    this.record("tool_result", { name, outcome, ...summary });
  }

  recordVerifiedSystemContext(context: Record<string, unknown>): void {
    const serialized = JSON.stringify(context);
    if (Buffer.byteLength(serialized) > 8 * 1024 || containsLikelySecret(serialized)) {
      throw new Error("verified SystemContext is too large or contains secret-like data");
    }
    this.record("status_summary", { phase: "verified_system_context", context });
  }

  planning(): void {
    const state = this.current().state;
    if (state === "investigating" || state === "continuing") this.transition("planning", "status_summary", { phase: "planning" });
  }

  planReady(changeId: string, risk: string, applicable: boolean): void {
    this.record("plan_ready", { change_id: changeId, risk, applicable }, "planning");
  }

  requestConfirmation(changeId: string, risk: string, reviewHash: string): void {
    const task = this.current();
    if (!task.change_ids.includes(changeId)) throw new Error("change does not belong to the active Relay task");
    this.transition("waiting_confirmation", "confirmation_required", { change_id: changeId, risk, review_hash: reviewHash });
  }

  confirmationDeclined(): void {
    this.transition("investigating", "status_summary", { phase: "confirmation_declined" });
  }

  applying(changeId: string): void {
    this.transition("applying", "applying", { change_id: changeId });
  }

  applySucceeded(changeId: string, outcome: string): void {
    this.transition("verifying", "tool_result", { name: "relay_change_apply", outcome, change_id: changeId });
  }

  recordAction(name: string, changeId: string, outcome: string): void {
    const state = this.current().state;
    if (name === "discard" && (state === "planning" || state === "continuing")) {
      this.transition("investigating", "tool_result", { name, change_id: changeId, outcome });
    } else {
      this.record("tool_result", { name, change_id: changeId, outcome }, state);
    }
  }

  verification(check: string, passed: boolean, evidence: Record<string, unknown>): void {
    const state = this.current().state;
    if (state === "applying" || state === "investigating") this.transition("verifying", "verification", { check, passed, ...evidence });
    else this.record("verification", { check, passed, ...evidence }, state);
    if (!passed && this.current().state === "verifying") this.continue("goal_verification_failed");
  }

  continue(reason: string): void {
    this.transition("continuing", "task_continuing", { reason });
    this.transition("investigating", "status_summary", { phase: "continued_investigation" });
  }

  complete(evidence: string): void {
    const task = this.current();
    if (!task.verified) throw new Error("task cannot be completed without a passing Relay verification");
    this.transition("completed", "completed", { evidence });
  }

  block(reason: string): void { this.transition("blocked", "blocked", { reason }); }
  fail(code: string): void { this.transition("failed", "failed", { code }); }
  cancel(): void {
    if (this.snapshot && !["completed", "cancelled"].includes(this.snapshot.state)) this.transition("cancelled", "cancelled", {});
  }

  contextSummary(): string {
    const task = this.current();
    const verifiedContext = [...task.events].reverse().find((event) => event.kind === "status_summary" && event.payload.phase === "verified_system_context")?.payload.context;
    const summaries = task.events
      .filter((event) => ["status_summary", "tool_result", "plan_ready", "verification", "applying"].includes(event.kind) && event.payload.phase !== "verified_system_context")
      .slice(-16)
      .map((event) => ({ kind: event.kind, state: event.state, payload: event.payload }));
    while (summaries.length) {
      const context = JSON.stringify({ task_id: task.id, goal: task.goal, state: task.state, change_ids: task.change_ids, verified_system_context: verifiedContext ?? null, observations: summaries });
      if (Buffer.byteLength(context) <= 12 * 1024) return context;
      summaries.shift();
    }
    const minimal = JSON.stringify({ task_id: task.id, goal: task.goal, state: task.state, change_ids: task.change_ids, verified_system_context: verifiedContext ?? null, observations: [] });
    if (Buffer.byteLength(minimal) > 12 * 1024) throw new Error("task resume context exceeds the size limit");
    return minimal;
  }

  private transition(state: TaskState, kind: TaskEventKind, payload: Record<string, unknown>): TaskSnapshot {
    return this.record(kind, payload, state);
  }

  private record(kind: TaskEventKind, payload: Record<string, unknown>, state = this.current().state): TaskSnapshot {
    this.snapshot = this.store.append(this.current().id, kind, state, payload);
    this.emit(this.snapshot.events.at(-1)!);
    return this.snapshot;
  }

  private emit(event: TaskEvent): void {
    if (!eventKinds.has(event.kind)) return;
    const safeEvent: AgentEvent = {
      schema_version: 1,
      task_id: event.task_id,
      sequence: event.sequence,
      at: event.timestamp,
      type: event.kind,
      state: event.state,
      payload: event.payload,
    };
    for (const listener of this.listeners) listener(safeEvent);
  }
}
