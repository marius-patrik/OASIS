# OASIS — Architecture-to-Implementation Pack

This pack continues the finalized product PRD into concrete implementation design. It is a **specification and reference skeleton**, not a functioning MMO.

| File | Contents |
|---|---|
| [`PRD.md`](PRD.md) | Product authority, decisions, acceptance requirements |
| [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) | Universal schema semantics, execution composition, state/authority, API and verification model |
| [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) | Implementation gates and parallel independent game-adapter lanes |
| [`migrations/0001_core.sql`](migrations/0001_core.sql) | PostgreSQL v0.1 universal table structure |
| [`migrations/0002_indexes.sql`](migrations/0002_indexes.sql) | Query indexes, one active authoritative presence, normalized handle uniqueness |
| [`contracts/src/lib.rs`](contracts/src/lib.rs) | Rust module/adapter contracts and reference protocol types |
| [`tests/verify_structure.py`](tests/verify_structure.py) | Offline specification structure smoke test (not a SQL execution test) |

## Baseline decisions

- One relational schema with first-class players, characters, items, worlds, engines, and universal typed components.
- Original engine modules perform original logic, physics and rendering; OASIS mediates their interfaces.
- Only universal interfaces; adapters never depend on each other.
- Cross-game interaction is the **final acceptance verification**, not an implementation workstream.
- DOOM and Cave Story Rust rewrites are candidates pending source inspection and feasibility gates.

## Local commands when toolchains are available

```bash
# Apply to disposable PostgreSQL 15+ database; database provisioned externally.
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0001_core.sql
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0002_indexes.sql

# Compile reference contracts and run unit tests.
cargo test --manifest-path contracts/Cargo.toml

# Only checks file presence and basic static structural invariants.
python tests/verify_structure.py
```

## Verification status

No PostgreSQL server, Rust toolchain, running games, or dynamic cross-game integration is bundled with this artifact. SQL execution, Rust compilation, and adapters must be tested in a properly provisioned build environment before describing this as runnable infrastructure.