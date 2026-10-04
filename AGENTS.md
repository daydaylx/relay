# AGENTS.md – Relay Repository Instructions

## Project identity

Relay is an independent NixOS-first system control tool with an optional embedded Pi agent
runtime for natural-language task investigation and orchestration.

It is **not**:
- a fork of the Pi coding-agent product,
- a generic autonomous root agent,
- a shell wrapper with unrestricted LLM access.

The system core must remain deterministic, testable, recoverable and usable without AI.

## Product goal

Relay translates user intent into safe, structured NixOS changes.

Primary flow:

```text
Intent
→ Resolve
→ Candidate
→ Evaluate
→ Build
→ Diff / Preview
→ Risk
→ Confirm
→ Activate
→ Health
→ Journal
→ Recovery
```

## Hard architecture rules

1. NixOS is the source of truth.
2. NixOS configuration writes remain inside the explicit managed boundary. User-file writes are
   forbidden until the Ownership Map and File Transaction Layer authorize a concrete target.
3. Pi never writes arbitrary Nix code directly. It uses the typed Relay Core for managed system
   changes and the Execution Gateway for other policy-approved operations.
4. AI is optional and never the security boundary.
5. PLAN/read operations must not mutate the system.
6. Every mutation requires:
   - preflight,
   - recovery plan,
   - risk classification,
   - verification.
7. `dry-activate` is preview only.
8. `nixos-rebuild test` is temporary activation, not automatic rollback.
9. Source state and runtime state must both be recoverable.
10. Switch inhibitors must never be bypassed automatically.
11. Protected resources must remain blocked by code, not merely by prompt.
12. Secrets must not enter the Nix store, normal logs, change journal or AI context.
13. No permanent root process.
14. No generic root shell exposed to the model.
15. Do not add subagents, Rabbitmode, remote administration, cloud daemons or multi-user server
    operation to this scope. Relay-owned MCP and web read access are part of the Knowledge Broker
    architecture, subject to Trust Classes and the Execution Gateway; never import personal Pi config.

## Protected resources for MVP

Automatic mutation is forbidden for:

- `system.stateVersion`
- partitioning
- filesystems
- LUKS / disk encryption
- bootloader
- Secure Boot
- Nix daemon trust settings
- `trusted-users`
- fundamental auth/user access configuration
- SSH access foundation
- secret-management foundations
- database major-version migrations
- major NixOS release upgrades

These areas may be inspected and planned, but not automatically applied.

## Managed write boundary

The MVP should assume a structure similar to:

```text
relay-system/
├── flake.nix
├── flake.lock
├── hosts/
├── hardware-configuration.nix
└── relay/
    └── managed.nix
```

Relay owns `relay/managed.nix`.

Other configuration files are read-only unless a later ADR explicitly expands the boundary.

## Coding principles

- Prefer typed domain objects over shell strings.
- Keep Nix/NixOS command construction inside one adapter.
- Prefer machine-readable output (`--json`, store paths, explicit metadata).
- Never parse human terminal output when a structured interface exists.
- Avoid implicit global state.
- Every mutating operation must be idempotent or explicitly guarded.
- Persist enough evidence for crash recovery.
- Refuse to guess when live state disagrees with planned state.
- Keep UI, AI, system adapter and change engine separable.

## Rust recommendation

Rust is the preferred implementation language for the core because Relay is a long-lived local system tool with:

- process orchestration,
- state machines,
- filesystem safety requirements,
- persistence,
- privilege boundaries,
- recovery semantics.

Do not implement Nix itself. Orchestrate official Nix/NixOS tools behind adapters.

## Testing expectations

Tests must focus on real safety properties, not coverage percentage.

Mandatory categories:

- source drift
- candidate isolation
- failed evaluation
- failed build
- switch inhibitor
- failed test activation
- failed health check
- reboot-required change
- protected resource rejection
- source rollback
- runtime rollback
- crash/restart during a change
- AI unavailable
- malformed model intent
- stale options index

## Definition of implementation quality

A feature is not complete merely because the command succeeds.

It is complete only when:
- state before is known,
- intended state is explicit,
- recovery is known,
- action is verifiable,
- result is journaled,
- failure path is tested.

## Scope discipline

If a proposed feature does not improve one of these core capabilities:

```text
Understand system
Plan change
Validate change
Apply safely
Verify result
Recover
Explain
```

it should normally be deferred until after the MVP.
