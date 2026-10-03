# Examples

All examples assume the one-time setup from the README: `relay init --flake /etc/nixos`, the module
imported by the host configuration (`./relay/managed.nix`), tracked by Git, and one manual rebuild
so the running system publishes `/etc/relay/managed.nix`.

Progress goes to stderr, results to stdout as JSON. `HOST` is the flake's `nixosConfigurations`
name.

## Bluetooth aktivieren

```sh
relay search-option bluetooth --index ~/.cache/relay/options.json --flake /etc/nixos --host HOST
relay plan --flake /etc/nixos --host HOST set-option hardware.bluetooth.enable bool true
```

```json
{"id":"chg-1790984727264-19831-000","risk":"LIVE_SWITCHABLE","applicable":true,
 "base_system":"/nix/store/…-nixos-system-HOST-…","candidate_system":"/nix/store/…-nixos-system-HOST-…",
 "reboot_components":[],"inhibitors":[],
 "managed_diff":["+   hardware.bluetooth.enable = true;"],
 "closure_diff":"bluez: ∅ → 5.…","next":"relay apply chg-1790984727264-19831-000"}
```

Nothing on the system changed yet: the candidate was evaluated and built from an isolated copy.

```sh
relay show chg-1790984727264-19831-000      # explanation, diff, recovery plan
relay apply chg-1790984727264-19831-000 --expect-active bluetooth.service
```

`apply` shows the review, asks for `yes`, then: checks source/runtime/candidate against the plan →
`dry-activate` preview → writes `relay/managed.nix` → re-evaluates the live source and compares the
candidate identity → `test` (temporary activation) → health check → profile + `switch`.
If anything fails, the source file and the runtime are restored and the exit code is `2`.

## VLC hinzufügen, entfernen und die letzte Änderung rückgängig machen

```sh
relay plan  --flake /etc/nixos --host HOST add-package vlc    && relay apply <id>
relay plan  --flake /etc/nixos --host HOST remove-package vlc && relay apply <id>
relay undo        # the removal is reverted: vlc is back, in source and in the running system
relay undo        # the addition is reverted as well
```

`remove-package` only works for packages Relay itself added (it knows the managed baseline).

## Typed intent (script or optional AI provider)

```sh
echo '{"schema":1,"changes":[
  {"op":"set_option","option":"hardware.bluetooth.enable","value":true},
  {"op":"add_package","package":"vlc"}]}' |
  relay plan --flake /etc/nixos --host HOST --intent -
```

Anything that does not match the schema — unknown operations, extra fields, wrong types,
protected options, Nix-looking option or package names — is rejected before any file is written.

## Refusals you should expect

```text
relay: change targets a protected resource (system.stateVersion) and cannot be applied automatically
relay: no effect: the managed configuration already contains these changes
relay: the configuration source changed since this plan was made; plan again
relay: change chg-… is test-activated and unresolved; run `relay recover` first
```

## Reboot-required change

When the candidate changes the kernel, initrd, kernel modules, systemd, or a NixOS *switch
inhibitor*, `plan` reports `REBOOT_REQUIRED`. `apply` then only prepares the next boot generation
(`boot`), never `test`/`switch`, and reports `reboot-pending`. After rebooting:

```sh
relay recover            # verifies the booted candidate (health) and records it as applied
relay recover --abort-pending   # or: cancel before rebooting
```

## After a crash or power loss

```sh
relay status --flake /etc/nixos | jq .unresolved_change
relay recover            # rolls back (or completes a verified change), from live evidence
```

## Natürliche Sprache (optional)

Das Modell schlägt nur einen typisierten Intent vor. Relay prüft ihn, plant ihn in einer isolierten
Kopie und fragt, bevor etwas angewendet wird.

```sh
# Was würde ein Provider erhalten? (sendet nichts)
relay ask "Aktiviere Bluetooth" --host HOST --options-index ~/.cache/relay/options.json --show-prompt

# Lokales Modell über Ollama (kein Schlüssel, kein Internet)
relay ask "Installiere vlc" --host HOST --flake /etc/nixos \
  --provider openai --base-url http://localhost:11434/v1 --model llama3.1:8b \
  --options-index ~/.cache/relay/options.json --packages-index ~/.cache/relay/packages.json

# Gehostetes Modell; der Schlüssel nur über die Umgebung oder eine Datei
export RELAY_AI_API_KEY_FILE=~/.config/relay/key
relay ask "Mach die letzte Änderung rückgängig" --host HOST --provider anthropic --model <modell>

# Eigener Adapter: ein Programm, das {"system":…,"user":…} von stdin liest und die Antwort druckt
relay ask "Entferne vlc" --host HOST --flake /etc/nixos --provider command --provider-command ./my-model.sh
```

Typische Ausgaben:

```text
relay: model output rejected: change 0: change targets a protected resource (system.stateVersion) …
relay: model output rejected: option 'hardware.bluetoth.enable' is not in the local options index (invented or stale?)
relay: the model declined: "I cannot change disks."
relay: nothing was applied. Review with `relay show <id>`, then `relay apply <id>`
```

`--explain` lässt den Provider das (deterministische) Review in Alltagssprache erklären;
`--apply` fragt nach dem Planen interaktiv. `--yes` wird von `ask` bewusst abgelehnt.

## Hyprland (nur lesend)

```sh
relay desktop status     # Version, Monitore, Workspaces, Fensteranzahl, Konfigurationsfehler (keine Titel)
relay desktop health     # {"responsive":true,"monitors":1,"config_errors":[]}
```

Mit laufender Sitzung vergleicht `apply` Monitore und Konfigurationsfehler vor und nach `test` und
rollt zurück, wenn ein Monitor verschwindet, der Kompositor nicht mehr antwortet oder neue Fehler
auftauchen (`--no-desktop-check` schaltet das ab).
