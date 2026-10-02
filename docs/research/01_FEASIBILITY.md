# Machbarkeitsprüfung

## Ergebnis

Technisch realistisch, wenn Relay kleiner bleibt als ein allgemeiner NixOS-Agent.

NixOS liefert bereits:

- deklaratives Modulsystem
- Options-Typen und Validierung
- Evaluation
- reproduzierbare Builds
- System-Closures
- Generationen
- build / dry-activate / test / boot / switch
- maschinenlesbare Nix-Ausgaben
- NixOS-Options-Dokumentation

Relay muss selbst liefern:

- Managed-Bereich
- typed intents
- Renderer
- Candidate Engine
- Drift Protection
- Risk Classification
- Health Checks
- Source + Runtime Recovery
- Change Journal
- Privilege Boundary
- Knowledge Cache

## Aufwand

```text
Proof of Concept:           4–5/10
brauchbares MVP:            6/10
persönlicher Daily Driver:  7–8/10
generischer NixOS Manager:  9/10
```
