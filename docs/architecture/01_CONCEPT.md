# Relay – Produktkonzept

## Zweck

Relay ist eine lokale Kontroll- und Bedieneschicht für NixOS.

```text
User Intent
→ Relay
→ NixOS Options / Packages / Runtime
→ Candidate Config
→ Evaluate
→ Build
→ Preview
→ Risk
→ Test / Boot / Switch
→ Verify
→ Journal / Recovery
```

## Kein Coding-Agent

Relay denkt primär in Host, Option, Paket, Generation, System Closure, Runtime State, Change, Risk und Recovery.

AI darf Nutzerabsicht verstehen und erklären, aber nicht beliebigen Nix-Code direkt schreiben oder Security Policies umgehen.

## Zielbild

```text
                RELAY UI
                   │
         ┌─────────┴─────────┐
         │                   │
      CLI/TUI            Natural Language
                              │
                              ▼
                        Optional AI Layer
                              │
                              ▼
                    Typed Relay Intent
                              │
                              ▼
┌────────────────────────────────────────────┐
│                Relay Core                  │
│ System Model · Change Engine · Policy      │
│ Health · Recovery · Journal · Knowledge    │
└────────────────────┬───────────────────────┘
                     │
                     ▼
┌────────────────────────────────────────────┐
│                 NixOS                      │
│ Modules · Evaluation · Build · Generation │
│ systemd · Nix Store · Runtime · Desktop   │
└────────────────────────────────────────────┘
```
