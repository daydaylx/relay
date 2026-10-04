import { appendFileSync, closeSync, constants, fchmodSync, fsyncSync, lstatSync, mkdirSync, openSync, readFileSync, readdirSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { containsLikelySecret } from "./security.js";

export type TaskState = "created" | "investigating" | "planning" | "waiting_confirmation" | "applying" | "verifying" | "continuing" | "completed" | "blocked" | "failed" | "cancelled";
export type TaskEventKind = "task_started" | "status_summary" | "tool_started" | "tool_result" | "plan_ready" | "confirmation_required" | "applying" | "verification" | "task_continuing" | "completed" | "blocked" | "failed" | "cancelled";

export interface TaskEvent {
  schema_version: 1;
  sequence: number;
  task_id: string;
  timestamp: string;
  kind: TaskEventKind;
  state: TaskState;
  payload: Record<string, unknown>;
}

export interface TaskSnapshot {
  id: string;
  goal: string;
  created_at: string;
  state: TaskState;
  events: TaskEvent[];
  change_ids: string[];
  verified: boolean;
}

const terminalStates = new Set<TaskState>(["completed", "blocked", "failed", "cancelled"]);
const transitions: Record<TaskState, readonly TaskState[]> = {
  created: ["investigating", "cancelled", "failed"],
  investigating: ["planning", "waiting_confirmation", "verifying", "continuing", "completed", "blocked", "failed", "cancelled"],
  planning: ["investigating", "waiting_confirmation", "blocked", "failed", "cancelled"],
  waiting_confirmation: ["applying", "investigating", "blocked", "cancelled", "failed"],
  applying: ["verifying", "investigating", "blocked", "failed", "cancelled"],
  verifying: ["completed", "continuing", "investigating", "blocked", "failed", "cancelled"],
  continuing: ["investigating", "planning", "blocked", "failed", "cancelled"],
  completed: [], blocked: ["investigating", "cancelled"], failed: ["investigating", "cancelled"], cancelled: [],
};

const MAX_EVENTS = 2_000;
const MAX_EVENT_BYTES = 16 * 1024;
const MAX_GOAL_BYTES = 4 * 1024;
const taskIdPattern = /^[0-9a-f]{8}-[0-9a-f-]{27,36}$/i;

export function defaultTaskDirectory(): string {
  return resolve(process.env.XDG_STATE_HOME || join(homedir(), ".local", "state"), "relay", "tasks");
}

export class TaskStore {
  constructor(private readonly directory = defaultTaskDirectory()) {}

  create(goal: string): TaskSnapshot {
    const normalized = goal.trim();
    if (!normalized || Buffer.byteLength(normalized) > MAX_GOAL_BYTES || containsLikelySecret(normalized)) {
      throw new Error("task goal is empty, too large, or looks like it contains a secret");
    }
    this.ensureDirectory();
    const id = randomUUID();
    const event: TaskEvent = {
      schema_version: 1, sequence: 0, task_id: id, timestamp: new Date().toISOString(),
      kind: "task_started", state: "created", payload: { goal: normalized },
    };
    this.writeEvent(id, event, true);
    return this.read(id);
  }

  read(id: string): TaskSnapshot {
    this.validateId(id);
    const path = this.path(id);
    const stat = lstatSync(path);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > MAX_EVENTS * MAX_EVENT_BYTES) throw new Error("task journal is not a bounded regular file");
    const lines = readFileSync(path, "utf8").split("\n").filter(Boolean);
    if (lines.length === 0 || lines.length > MAX_EVENTS) throw new Error("task journal event count is invalid");
    const events = lines.map((line) => {
      if (Buffer.byteLength(line) > MAX_EVENT_BYTES) throw new Error("task journal event exceeds the size limit");
      const parsed: unknown = JSON.parse(line);
      if (!isTaskEvent(parsed) || parsed.task_id !== id || containsLikelySecret(JSON.stringify(parsed.payload))) {
        throw new Error("task journal contains an invalid or sensitive event");
      }
      return parsed;
    });
    const snapshot = aggregate(events);
    if (!snapshot) throw new Error("task journal does not begin with a valid task event");
    return snapshot;
  }

  append(id: string, kind: TaskEventKind, state: TaskState, payload: Record<string, unknown> = {}): TaskSnapshot {
    this.validateId(id);
    const current = this.read(id);
    if (current.events.length >= MAX_EVENTS) throw new Error("task journal event limit reached");
    if (!transitions[current.state].includes(state) && state !== current.state) {
      throw new Error(`invalid task transition ${current.state} -> ${state}`);
    }
    const clean = boundedPayload(payload);
    if (containsLikelySecret(JSON.stringify(clean))) throw new Error("task event looks like it contains a secret");
    const event: TaskEvent = {
      schema_version: 1, sequence: current.events.length, task_id: id,
      timestamp: new Date().toISOString(), kind, state, payload: clean,
    };
    this.writeEvent(id, event, false);
    return this.read(id);
  }

  list(): TaskSnapshot[] {
    this.ensureDirectory();
    return readdirSync(this.directory)
      .filter((name) => name.endsWith(".jsonl") && taskIdPattern.test(name.slice(0, -6)))
      .sort()
      .map((name) => this.read(name.slice(0, -6)));
  }

  private ensureDirectory(): void {
    mkdirSync(this.directory, { recursive: true, mode: 0o700 });
    const stat = lstatSync(this.directory);
    if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error("task store path must be a real directory");
    const descriptor = openSync(this.directory, "r");
    try { fchmodSync(descriptor, 0o700); } finally { closeSync(descriptor); }
  }

  private validateId(id: string): void {
    if (!taskIdPattern.test(id)) throw new Error("task id is invalid");
  }

  private path(id: string): string { return join(this.directory, `${id}.jsonl`); }

  private writeEvent(id: string, event: TaskEvent, create: boolean): void {
    const line = `${JSON.stringify(event)}\n`;
    if (Buffer.byteLength(line) > MAX_EVENT_BYTES) throw new Error("task event exceeds the size limit");
    const flags = create
      ? constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | (constants.O_NOFOLLOW ?? 0)
      : constants.O_WRONLY | constants.O_APPEND | (constants.O_NOFOLLOW ?? 0);
    const descriptor = openSync(this.path(id), flags, 0o600);
    try {
      fchmodSync(descriptor, 0o600);
      appendFileSync(descriptor, line, { encoding: "utf8" });
      fsyncSync(descriptor);
    } finally { closeSync(descriptor); }
  }
}

function aggregate(events: TaskEvent[]): TaskSnapshot | undefined {
  const first = events[0];
  if (!first || first.kind !== "task_started" || first.state !== "created" || typeof first.payload.goal !== "string") return undefined;
  let state: TaskState = "created";
  let verified = false;
  const changeIds = new Set<string>();
  events.forEach((event, index) => {
    if (event.sequence !== index || (index > 0 && !transitions[state].includes(event.state) && event.state !== state)) {
      throw new Error("task journal sequence or state transition is invalid");
    }
    if (terminalStates.has(state) && event.state !== state) throw new Error("task journal continues after a terminal state");
    state = event.state;
    if (event.kind === "plan_ready" && typeof event.payload.change_id === "string") changeIds.add(event.payload.change_id);
    if (event.kind === "verification") verified = event.payload.passed === true;
    if (event.kind === "applying" || event.kind === "task_continuing") verified = false;
    if (event.kind === "completed" && !verified) throw new Error("task cannot be completed without a passing verification event");
  });
  return { id: first.task_id, goal: first.payload.goal, created_at: first.timestamp, state, events, change_ids: [...changeIds], verified };
}

function boundedPayload(value: Record<string, unknown>): Record<string, unknown> {
  const encoded = JSON.stringify(value);
  if (Buffer.byteLength(encoded) > MAX_EVENT_BYTES - 512) throw new Error("task event payload exceeds the size limit");
  return JSON.parse(encoded) as Record<string, unknown>;
}

function isTaskEvent(value: unknown): value is TaskEvent {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const event = value as Partial<TaskEvent>;
  return event.schema_version === 1 && Number.isSafeInteger(event.sequence) && (event.sequence ?? -1) >= 0 &&
    typeof event.task_id === "string" && taskIdPattern.test(event.task_id) && typeof event.timestamp === "string" &&
    typeof event.kind === "string" && ["task_started", "status_summary", "tool_started", "tool_result", "plan_ready", "confirmation_required", "applying", "verification", "task_continuing", "completed", "blocked", "failed", "cancelled"].includes(event.kind) &&
    typeof event.state === "string" && Object.hasOwn(transitions, event.state) && !!event.payload && typeof event.payload === "object" && !Array.isArray(event.payload);
}
