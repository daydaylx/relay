# Risk Register

Jedes Risiko nennt die Kontrolle und – seit der Umsetzung von T2–T4 – wo sie im Code sitzt und
welcher Test sie belegt.

## R1 – Source/runtime mismatch

**Risiko:** Candidate wurde gebaut, Live Source änderte sich danach.

**Kontrolle:** Hash/identity unmittelbar vor Apply erneut prüfen.

**Umsetzung:** `engine.rs::preflight` vergleicht Quellbaum-Hash, laufendes System, Systemprofil,
Kandidatenverzeichnis und Managed-Modul mit dem Plan; nach dem Schreiben wird die Live-Quelle neu
evaluiert und gegen die Kandidaten-Derivation geprüft (`identity-mismatch` ⇒ Rollback vor jeder
Aktivierung). Tests: `source_drift_between_plan_and_apply_aborts_before_any_mutation`,
`runtime_drift_and_pending_reboots_block_apply`, `a_modified_or_missing_candidate_is_never_activated`,
`a_candidate_that_evaluates_differently_after_the_source_write_is_never_activated`.

---

## R2 – `test` wird als Rollback missverstanden

**Risiko:** temporäre Aktivierung verändert Runtime und kann stateful Nebenwirkungen haben.

**Kontrolle:** vorherigen Systempfad speichern, Health Gate, Recovery vorbereiten.

**Umsetzung:** Plan-Record und Journal speichern den vorherigen Systempfad; ein fehlgeschlagener
Health-Check führt zu einer aktiven Rückaktivierung (`switch-to-configuration test <vorher>`), nie
zur Annahme, `test` heile sich selbst. Tests:
`failed_health_check_rolls_back_because_test_is_not_a_rollback`, VM-Test „a unit that crashes after
activation…“.

---

## R3 – Stateful service migration

**Risiko:** alte Generation kann Datenformatänderung nicht rückgängig machen.

**Kontrolle:** `MIGRATION_REQUIRED`, keine automatische Mutation im MVP.

**Umsetzung:** `change.rs` klassifiziert Datenbank-Dienste als `MIGRATION_REQUIRED` bzw. blockiert
`services.<db>.package` als geschützt; `apply` verweigert diese Klasse. Test:
`plans_for_stateful_migrations_are_explained_but_never_applied`.

---

## R4 – Secrets im Nix Store

**Risiko:** sensible Daten werden über Store/Logs sichtbar.

**Kontrolle:** Secrets aus Managed Config und AI Context verbannen.

**Umsetzung:** Optionsnamen mit Geheimnis-Bestandteilen (`password`, `secret`, `token`, `psk`, …)
sind geschützt; das Journal enthält nur Kennungen, Hashes, Store-Pfade und Reason-Codes; Nix-Stderr
(kann ausgewertete Werte enthalten) landet nur in privaten 0600-Dateien, nie in normaler Ausgabe;
Journal und State-Verzeichnis sind 0600/0700. Tests: `secret_bearing_option_names_never_reach_managed_nix`,
`journal_and_state_never_contain_configuration_values`,
`failed_evaluation_fails_the_plan_without_touching_anything_and_keeps_stderr_private`.

---

## R5 – Model hallucinated option/value

**Risiko:** AI erfindet eine Option oder einen ungültigen Wert.

**Kontrolle:** typed schema + Options Index + evaluation; Modelloutput ist nie authoritative.

**Umsetzung:** `intent.rs` akzeptiert nur das strikte Schema; erfundene Optionen scheitern in der
echten Evaluation des Kandidaten (getestet gegen echtes Nix), ohne dass etwas verändert wird.
Tests: `malformed_model_intents_are_rejected_before_rendering`,
`protected_resources_cannot_be_smuggled_in_through_an_intent`.

**Ergänzung (T5):** `relay ask` gleicht Vorschläge zusätzlich mit dem lokalen Options-/Paketindex ab
(erfundene Namen, schreibgeschützte Optionen, unpassende Typen) und verlangt eine getippte
Bestätigung. Tests: `invented_names_and_impossible_values_are_caught_against_the_local_index`,
`hostile_or_confused_model_output_never_reaches_planning`.

---

## R6 – Force bypass of NixOS safety

**Risiko:** Switch Inhibitor oder Validation wird umgangen.

**Kontrolle:** Force-/bypass mechanisms in Policy deny-list.

**Umsetzung:** Es gibt keine Force-Option. `NIXOS_NO_CHECK` wird aus jedem Kindprozess entfernt und
kann nicht gesetzt werden (`exec.rs`); Inhibitoren führen zum Boot-Pfad, nie zu `test`/`switch`
(`host.rs::compare_systems`, `engine.rs`). Tests:
`switch_check_bypass_variables_can_never_be_passed_to_children`,
`a_switch_inhibitor_forces_the_boot_path_and_is_never_bypassed`, VM-Test „a switch inhibitor forces
the boot path…“ (mit `NIXOS_NO_CHECK=1` in der Umgebung).

---

## R7 – Broken boot

**Risiko:** boot-required Change startet nicht.

**Kontrolle:** vorherige Generation erhalten; boot path und recovery documentation; protected boot scope.

**Umsetzung:** Bootloader-, initrd-, LUKS- und Dateisystem-Optionen sind geschützt. Reboot-Änderungen
setzen nur das Systemprofil und `boot`; nach dem Reboot verifiziert `relay recover` (Health) und
rollt andernfalls über die Boot-Konfiguration auf die vorherige Generation zurück, nie per Live-Switch.
Tests: `an_unhealthy_system_after_the_reboot_is_rolled_back_to_the_previous_generation`,
`a_pending_reboot_can_be_aborted_explicitly`.

---

## R8 – Over-expansion of scope

**Risiko:** Projekt wird wieder zum universellen Agenten.

**Kontrolle:** Target States; AI erst ab T5; Desktop erst ab T6.

**Umsetzung:** Keine Abhängigkeiten, kein Netzwerk, keine Subagenten/MCP/Plugins; die einzigen
privilegierten Aktionen sind zwei typisierte Adapter-Aufrufe (ADR 0006).

---

## R9 – CLI compatibility drift

**Risiko:** Nix-Version verändert experimentelle CLI.

**Kontrolle:** zentraler NixAdapter + capability/version detection.

**Umsetzung:** Alle Nix-/NixOS-Kommandos entstehen in `nix.rs`. Entscheidungen stützen sich auf
Dateien und Links (`switch-inhibitors`, Kernel-Links, Systemprofil) und strukturierte Ausgaben
(`--json`, `systemctl --output=json`, `nixos-version --json`), nicht auf Terminaltext. Nur die
Kombination Nix 2.34.8 / NixOS 26.05 ist validiert; es wird keine breitere Kompatibilität behauptet.

---

## R10 – Recovery depends on AI

**Risiko:** Nutzer kann ohne Provider/Internet nicht zurück.

**Kontrolle:** Recovery CLI/Core vollständig AI-unabhängig.

**Umsetzung:** `undo`, `recover` und alle Rollbacks sind rein lokal; es gibt keinen Provider-Code.
Test: `intents_from_any_frontend_run_the_same_pipeline_without_any_ai_provider`.

---

## R11 – Zwei Relay-Prozesse / halb abgeschlossene Änderung

**Risiko:** Zwei gleichzeitige Änderungen oder ein Absturz mitten in der Aktivierung hinterlassen
einen Zustand, den niemand mehr versteht.

**Kontrolle:** Single-Writer-Lock, Write-ahead-Journal, evidenzbasierte Recovery, Blockade neuer
Änderungen bis zur Auflösung.

**Umsetzung:** `state.rs::lock`, `journal.rs` (nur erlaubte Übergänge, nur ein abgeschnittener
letzter Eintrag wird toleriert), `engine.rs::recover`. Tests: die `a_crash_…`-Tests,
`recovery_refuses_to_guess_when_the_runtime_is_neither_previous_nor_candidate`,
`a_second_relay_process_is_excluded_by_the_lock`.

---

## R12 – Datenabfluss an einen KI-Provider

**Risiko:** Anfragen, Index-Auszüge oder Reviews verlassen den Rechner; ein Schlüssel könnte in Logs oder
der Prozessliste landen.

**Kontrolle:** minimaler, deterministischer Prompt ohne Werte/Pfade/Secrets (`--show-prompt` zeigt ihn);
Schlüssel nur aus Umgebung/Datei, per stdin an `curl`, nie in Argumenten, Journal oder Debug-Ausgabe;
HTTP nur für Loopback; Provider-Fehler geben keinen Response-Body weiter; Zeitlimit.

**Umsetzung:** `ai.rs`. Tests: `the_prompt_states_the_schema_and_rules_and_carries_no_secrets_or_paths`,
`the_openai_compatible_provider_keeps_the_key_out_of_arguments_and_parses_the_answer`,
`local_servers_may_use_plain_http_but_nothing_else_may`, `http_failures_are_reported_without_leaking_the_response_body`.

---

## R13 – Eine Systemänderung beschädigt die grafische Sitzung

**Risiko:** Ein `switch` lässt Monitore verschwinden, beendet den Kompositor oder bricht seine
Konfiguration.

**Kontrolle:** Desktop-Baseline vor und Desktop-Health nach `test`; Rollback bei Schaden; Session-
Fundamente (Display-Manager) nur über den Boot-Pfad; der Kompositor wird nie gesteuert.

**Umsetzung:** `hypr.rs`, `engine.rs`. Tests: die Desktop-Tests im Simulator,
`only_the_allow_listed_read_only_requests_exist`,
`hyprland_options_are_live_but_session_foundations_need_the_boot_path`.
