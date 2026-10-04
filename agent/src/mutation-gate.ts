export type MutationAction = "apply" | "undo" | "recover";
type Plan = { id: string; applicable: boolean; risk: string; reviewHash?: string };
type Confirmation = { action: MutationAction; phrase: string; params: Record<string, unknown> };

export class MutationGate {
  private readonly plans = new Map<string, Plan>();
  private confirmation: Confirmation | undefined;

  recordPlan(plan: Plan): void {
    if (/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(plan.id)) this.plans.set(plan.id, plan);
  }

  requestApply(id: string): string | undefined {
    const plan = this.plans.get(id);
    if (!plan || !plan.applicable || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(id)) return undefined;
    const phrase = plan.reviewHash ? `APPLY ${id} ${plan.reviewHash.slice(0, 12)}` : `APPLY ${id}`;
    this.confirmation = { action: "apply", phrase, params: { change_id: id, confirmed: true } };
    return phrase;
  }

  requestUndo(preview: unknown): string | undefined {
    if (!preview || typeof preview !== "object") return undefined;
    const value = preview as { change_id?: unknown; review?: unknown };
    if (typeof value.change_id !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(value.change_id) || typeof value.review !== "string") return undefined;
    const reviewHash = typeof (value as { reviewHash?: unknown }).reviewHash === "string" ? (value as { reviewHash: string }).reviewHash : undefined;
    const phrase = reviewHash ? `UNDO ${value.change_id} ${reviewHash.slice(0, 12)}` : `UNDO ${value.change_id}`;
    this.confirmation = { action: "undo", phrase, params: { change_id: value.change_id, confirmed: true } };
    return phrase;
  }

  requestRecover(preview: unknown, reviewHash?: string): { phrase: string; ids: string[] } | undefined {
    if (!Array.isArray(preview) || preview.length === 0 || preview.length > 32) return undefined;
    const ids = preview.map((entry) => (entry as { id?: unknown })?.id);
    if (ids.some((id) => typeof id !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(id))) return undefined;
    const targets = ids as string[];
    if (new Set(targets).size !== targets.length) return undefined;
    const phrase = reviewHash ? `RECOVER ${targets.join(" ")} ${reviewHash.slice(0, 12)}` : `RECOVER ${targets.join(" ")}`;
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
    this.plans.clear();
  }

  clearPlanById(id: string): void {
    this.plans.delete(id);
  }

  planRisk(id: string): string | undefined {
    return this.plans.get(id)?.risk;
  }

  hasConfirmation(): boolean {
    return this.confirmation !== undefined;
  }
}
