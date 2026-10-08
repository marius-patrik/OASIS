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
- [Pinned upstream native game sources](engines/README.md) for DOOM and Cave Story,
  both compiled independently in the [native-source CI](.github/workflows/native-engines.yml).
  These sources are **not yet OASIS adapters**.
- [Adapter catalog](runtime/src/catalog.rs) for transactional, collision-free
  native module registration and independently activated execution contexts.
- [Universal world coordinator](runtime/src/universe.rs) for authenticated player
  identity, independent native modules, world presence, movement and replication.
- [World tick scheduler](runtime/src/scheduler.rs) for independent native
  simulation clocks without client-driven ticks.
- [Loopback development gateway](runtime/src/gateway.rs) with real TCP clients.
  This is deliberately not a public network interface and has no production
  authentication, transport security or bandwidth-aware state replication.
- [Durable travel migrations](migrations/) with atomic presence/authority
  handoff, transactional native checkpoints, and idempotent retries.
- [PostgreSQL runtime store](store/src/lib.rs) for loading accounts, players'
  sessions, arbitrary entity components, active world presence, and native
  engine snapshots; native state is persisted using a lossless tagged codec.
  [Database integration tests](store/tests/postgres.rs) exercise two-way
  travel, recovery and rejected stale updates against real PostgreSQL.
- [Trusted runtime recovery service](service/src/lib.rs) verifies a persisted
  session, reads its current native state and authoritative world placement,
  and restores the original engine module through the generic runtime.
  Expired/missing authority is rejected rather than assuming an empty world.
  CI restores a character from PostgreSQL into a fresh runtime.
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
psql -v ON_ERROR_STOP=1 -f migrations/0005_native_checkpoints.sql
psql -v ON_ERROR_STOP=1 -f tests/schema_smoke.sql
psql -v ON_ERROR_STOP=1 -f tests/store_fixture.sql
DATABASE_URL="host=localhost user=oasis password=oasis_ci dbname=oasis" cargo test -p oasis-store --test postgres
DATABASE_URL="host=localhost user=oasis password=oasis_ci dbname=oasis" cargo test -p oasis-service --test postgres
```

**Status:** This is a working foundation, **not yet a complete MMO**.
The **actual DOOM and Cave Story adapters**, crash-safe **live** world-travel orchestration and lease reclamation,
production networking/authentication, scalable
interest-managed replication, and GPU compositing remain unimplemented.
The PostgreSQL store and recovery service provide real persistent reads,
transactional writes, and cold-start native-state recovery.
However, **live travel is not yet orchestrated atomically with native module
lifecycle**, and expired leases cannot yet be reclaimed. The TCP gateway uses injected authentication and returns snapshot
identifiers/revisions, not complete game-ready replicated state.
Cross-game interactions are final black-box verification criteria,
never hard-coded game-pair features.

No game assets or proprietary source code are distributed in this repository.
