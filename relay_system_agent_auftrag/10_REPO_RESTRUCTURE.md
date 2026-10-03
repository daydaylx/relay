# 10 – Vorgeschlagene Repo-Struktur

Keine sofortige Komplettmigration erzwingen. Zielstruktur:

```text
relay/
├── AGENTS.md
├── README.md
├── Cargo.toml
├── package.json
├── flake.nix
│
├── crates/
│   ├── relay-core/          # bisherige deterministische Rust-Engine
│   └── relay-protocol/      # optional: Schema/Protocol helpers
│
├── agent/
│   ├── src/
│   │   ├── main/
│   │   ├── context/
│   │   ├── inventory/
│   │   ├── router/
│   │   ├── tools/
│   │   ├── relay-bridge/
│   │   └── ui/
│   ├── prompts/
│   └── tests/
│
├── docs/
│   ├── architecture/
│   ├── security/
│   ├── planning/
│   └── testing/
│
├── relay-managed/
└── tests/
```

## Namensentscheidung

Das bestehende Binary `relay` darf vorerst bleiben.

Später mögliche Trennung:

```text
relay           # Systemagent / Nutzeroberfläche
relay-core      # interne Core-Schnittstelle
```

oder:

```text
relay-agent
relay-core
```

Vor Umbenennung ADR erstellen.

## Rust nicht wegwerfen

Die vorhandene Rust-Engine ist wertvoll und darf nicht einfach in TypeScript neu
implementiert werden.

Agent Runtime und Security/Transaction Core dürfen unterschiedliche Sprachen verwenden.
