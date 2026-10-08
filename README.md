# OASIS

Universal multiplayer game-runtime infrastructure. Independently implemented
game engines retain their own native simulation, physics, controllers, cameras
and rendering; OASIS defines the storage, module interfaces, composition and
authoritative interaction contracts through which they can coexist.

## Documents
- [Product requirements](PRD.md)
- [Technical design](TECHNICAL_DESIGN.md)
- [Implementation plan](IMPLEMENTATION_PLAN.md)

## Current implementation
- [PostgreSQL schema](migrations/) with dedicated universal tables and extensible record data.
- [Engine-independent Rust contracts](contracts/src/lib.rs).
- [Native-module host](runtime/src/lib.rs) with spatial query dispatch, native
  state snapshots, authority fencing, and typed interaction routing.
- [Universal world coordinator](runtime/src/universe.rs) for authenticated player
  identity, independent native modules, world presence, movement and replication.
- [World tick scheduler](runtime/src/scheduler.rs) for independent native
  simulation clocks without client-driven ticks.
- [Loopback development gateway](runtime/src/gateway.rs) with real TCP clients.
  This is deliberately not a public network interface and has no production
  authentication, transport security or bandwidth-aware state replication.
- [Durable travel migration](migrations/0004_world_travel.sql) with atomic
  presence/authority handoff and idempotent transaction retries.
- [GitHub Actions CI](.github/workflows/ci.yml) compiling the Rust workspace,
  running module-level tests, applying PostgreSQL migrations and checking constraints.

### Test locally

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
python3 tests/verify_structure.py
# With a disposable PostgreSQL 16 instance and PG* environment configured:
psql -v ON_ERROR_STOP=1 -f migrations/0001_core.sql
psql -v ON_ERROR_STOP=1 -f migrations/0002_indexes.sql
psql -v ON_ERROR_STOP=1 -f migrations/0003_transfers.sql
psql -v ON_ERROR_STOP=1 -f migrations/0004_world_travel.sql
psql -v ON_ERROR_STOP=1 -f tests/schema_smoke.sql
```

**Status:** This is a working foundation, **not yet a complete MMO**.
The real DOOM and Cave Story adapters, production networking, physical graphics composition and game adapters
remain unimplemented. Cross-game interactions are final black-box verification
criteria, never hard-coded game-pair features.

No game assets or proprietary source code are distributed in this repository.
