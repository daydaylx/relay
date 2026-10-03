export type MutationAction = "apply" | "undo" | "recover";
type Plan = { id: string; applicable: boolean; risk: string };
type Confirmation = { action: MutationAction; phrase: string; params: Record<string, unknown> };

export class MutationGate {
  private plan: Plan | undefined;
  private confirmation: Confirmation | undefined;

  recordPlan(plan: Plan): void {
    if (/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(plan.id)) this.plan = plan;
  }

  requestApply(id: string): string | undefined {
    if (!this.plan || this.plan.id !== id || !this.plan.applicable || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(id)) return undefined;
    const phrase = `APPLY ${id}`;
    this.confirmation = { action: "apply", phrase, params: { change_id: id, confirmed: true } };
    return phrase;
  }

  requestUndo(preview: unknown): string | undefined {
    if (!preview || typeof preview !== "object") return undefined;
    const value = preview as { change_id?: unknown; review?: unknown };
    if (typeof value.change_id !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(value.change_id) || typeof value.review !== "string") return undefined;
    const phrase = `UNDO ${value.change_id}`;
    this.confirmation = { action: "undo", phrase, params: { change_id: value.change_id, confirmed: true } };
    return phrase;
  }

  requestRecover(preview: unknown): { phrase: string; ids: string[] } | undefined {
    if (!Array.isArray(preview) || preview.length === 0 || preview.length > 32) return undefined;
    const ids = preview.map((entry) => (entry as { id?: unknown })?.id);
    if (ids.some((id) => typeof id !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(id))) return undefined;
    const targets = ids as string[];
    if (new Set(targets).size !== targets.length) return undefined;
    const phrase = `RECOVER ${targets.join(" ")}`;
    this.confirmation = { action: "recover", phrase, params: { expected_ids: targets, confirmed: true } };
    return { phrase, ids: targets };
  }

  consume(input: string): { kind: "none" } | { kind: "cancelled" } | { kind: "authorized"; action: MutationAction; params: Record<string, unknown> } {
    if (!this.confirmation) return { kind: "none" };
    const pending = this.confirmation;
    this.confirmation = undefined;
    if (input !== pending.phrase) return { kind: "cancelled" };
    return { kind: "authorized", action: pending.action, params: pending.params };
  }

  clearPlan(): void {
    this.plan = undefined;
  }

  planRisk(id: string): string | undefined {
    return this.plan?.id === id ? this.plan.risk : undefined;
  }

  hasConfirmation(): boolean {
    return this.confirmation !== undefined;
  }
}
