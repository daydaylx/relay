# Contributing to Relay

## Before making a change

Read:

1. `README.md`
2. `AGENTS.md`
3. relevant ADRs
4. `docs/planning/04_TARGET_STATES.md`
5. `docs/planning/03_ACCEPTANCE.md`

## Change categories

### Core-safe
Changes that do not mutate a live NixOS system.

Examples:
- parsing
- indexing
- UI
- read-only discovery
- test fixtures

### System-mutating
Any change that can alter:
- Nix source state,
- generations,
- activation,
- services,
- boot behavior,
- privileged state.

These changes require:
- explicit recovery design,
- failure-path tests,
- policy review,
- journal behavior.

## Pull requests

Each PR should state:

- problem,
- scope,
- architectural impact,
- affected target state,
- safety impact,
- tests,
- recovery implications.

Do not combine unrelated architectural changes.

## New dependencies

New runtime dependencies need a short justification:
- why needed,
- why standard library/current dependencies are insufficient,
- security/maintenance impact.

## Protected scope

Do not implement automatic mutation of protected resources unless a new ADR explicitly changes the project boundary.
