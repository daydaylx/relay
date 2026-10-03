# Empfohlene Implementierungsreihenfolge

## Schritt 1

Repository bootstrap.

Noch keine NixOS-Mutation.

## Schritt 2

Read-only System Adapter.

Relay muss den Rechner verstehen.

## Schritt 3

Options- und Package-Knowledge.

Relay muss wissen, welche strukturierten Änderungen überhaupt existieren.

## Schritt 4

Typed Change Object + Renderer.

Noch keine Aktivierung.

## Schritt 5

Candidate Engine.

Build und Diff isoliert.

## Schritt 6

Risk + Policy.

Protected Scope technisch blockieren.

## Schritt 7

Activation zunächst nur in NixOS-VM/Testumgebung.

## Schritt 8

Recovery in Simulator und NixOS-VM belegen. Aktivierung und Recovery nicht auf dem Daily Driver
testen. Ein späterer Live-Einsatz ist eine separate, bewusst geprüfte Bereitstellung und kein
Abnahmetest.

## Schritt 9

AI.

Erst nachdem dieselben Operationen ohne AI funktionieren.

## Schritt 10

Hyprland/Desktop.

Nicht vorher.

## Anti-Reihenfolge

Nicht so beginnen:

```text
ChatGPT
→ UI
→ Hyprland
→ Root Permissions
→ irgendwann Systemmodell
```

Das würde erneut einen Agenten statt eines Systemwerkzeugs erzeugen.
