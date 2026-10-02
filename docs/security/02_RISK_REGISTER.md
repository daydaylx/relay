# Risk Register

## R1 – Source/runtime mismatch

**Risiko:** Candidate wurde gebaut, Live Source änderte sich danach.

**Kontrolle:** Hash/identity unmittelbar vor Apply erneut prüfen.

---

## R2 – `test` wird als Rollback missverstanden

**Risiko:** temporäre Aktivierung verändert Runtime und kann stateful Nebenwirkungen haben.

**Kontrolle:** vorherigen Systempfad speichern, Health Gate, Recovery vorbereiten.

---

## R3 – Stateful service migration

**Risiko:** alte Generation kann Datenformatänderung nicht rückgängig machen.

**Kontrolle:** `MIGRATION_REQUIRED`, keine automatische Mutation im MVP.

---

## R4 – Secrets im Nix Store

**Risiko:** sensible Daten werden über Store/Logs sichtbar.

**Kontrolle:** Secrets aus Managed Config und AI Context verbannen.

---

## R5 – Model hallucinated option/value

**Risiko:** AI erfindet eine Option oder einen ungültigen Wert.

**Kontrolle:** typed schema + Options Index + evaluation; Modelloutput ist nie authoritative.

---

## R6 – Force bypass of NixOS safety

**Risiko:** Switch Inhibitor oder Validation wird umgangen.

**Kontrolle:** Force-/bypass mechanisms in Policy deny-list.

---

## R7 – Broken boot

**Risiko:** boot-required Change startet nicht.

**Kontrolle:** vorherige Generation erhalten; boot path und recovery documentation; protected boot scope.

---

## R8 – Over-expansion of scope

**Risiko:** Projekt wird wieder zum universellen Agenten.

**Kontrolle:** Target States; AI erst ab T5; Desktop erst ab T6.

---

## R9 – CLI compatibility drift

**Risiko:** Nix-Version verändert experimentelle CLI.

**Kontrolle:** zentraler NixAdapter + capability/version detection.

---

## R10 – Recovery depends on AI

**Risiko:** Nutzer kann ohne Provider/Internet nicht zurück.

**Kontrolle:** Recovery CLI/Core vollständig AI-unabhängig.
