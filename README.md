# OASIS

Universal multiplayer game-runtime infrastructure. Independently implemented
game engines retain their own native simulation, physics, controllers, cameras
and rendering; OASIS defines the storage, module interfaces, composition and
authoritative interaction contracts through which they can coexist.

## Documents
- [Product requirements](PRD.md)
- [Technical design](TECHNICAL_DESIGN.md)
- [Implementation plan](IMPLEMENTATION_PLAN.md)
- [G0 full-game contract v1](contracts/G0_SPEC.md)
- [G0 requirements/test evidence matrix](contracts/G0_REQUIREMENTS_MATRIX.md)
- [G0 upstream feasibility and license blockers](contracts/G0_FEASIBILITY_AND_LICENSES.md)

## Current implementation
- [PostgreSQL schema](migrations/) with dedicated universal tables and extensible record data.
- [Engine-independent Rust contracts](contracts/src/lib.rs).
- [Native-module host](runtime/src/lib.rs) with spatial query dispatch, native
  state snapshots, authority fencing, and typed interaction routing.
- [Pinned upstream native game sources](engines/README.md) for DOOM and Cave Story,
  both compiled independently in the [native-source CI](.github/workflows/native-engines.yml).
- [Cave Story source-backed native modules](adapters/cave-story/) execute
  original `doukutsu-rs` hitbox checks, weapon-ammunition operations, and
  native PXM stage loading/tile-attribute lookup. All register through the
  same generic contracts, preserve original module state and arbitrary
  snapshot components, and have independent source-backed CI tests.
- [DOOM source-backed native module](adapters/doom/) executes original
  `room` 16.16 fixed-point operations, approximate distance, and bbox
  routines (including exact upstream edge cases). Kept in an independent
  GPL-scoped adapter rather than linking GPL game code into the core.
- [Source-native Cave Story gameplay worker](adapters/cave-story/src/gameplay.rs)
  now executes pinned original player controller/movement, PXM stage collision,
  original inventory/weapon firing, bullet movement and projectile–tile collision
  inside an isolated non-`Send` worker. Source tests validate original input,
  ammo and movement checkpoint restart. Its native snapshot omits active bullets,
  original cooldowns, NPCs/scripts, camera and full scene state.
- **Neither original game is fully playable through OASIS.** DOOM currently
  executes only original fixed-point/distance/bbox functions; its world/thinker,
  player, WAD, weapons and renderer are not hosted. Cave Story's partial
  original source worker is not a complete scene/game server or native renderer.
- [Independent source engine worker processes](adapters/) run reviewed native
  functions and expose original partial state via the generic worker protocol.
  Process isolation alone does not provide OS sandboxing.
- [Source-neutral v1 full-game roles](contracts/src/universal.rs) specify
  world simulations, native controllers, geometry, item effects, opaque full
  native checkpoints, actual render buffers/cameras, isolation, clocks and
  lifecycle. [Neutral 2D/3D vectors](contracts/src/neutral.rs) and
  [envelope tests](contracts/tests/g0_contract.rs) exercise the wire shapes;
  they do **not** certify source gameplay or complete conformance.
- [Game-independent native subprocess boundary](native-process/) supports
  existing partial source workers, snapshot/restore and all synchronous
  `WorldPort` callbacks. A fully enforced v1 worker ABI, original scenes,
  real source render pixels and end-to-end production security are pending.
- [Adapter catalog](runtime/src/catalog.rs) for transactional, collision-free
  native module registration and independently activated execution contexts.
- [Universal world coordinator](runtime/src/universe.rs) for authenticated player
  identity, independent native modules, world presence, movement and replication.
- [Lease-gated world scheduler](runtime/src/scheduler.rs) for independent
  native simulation clocks without client-driven ticks; every persistent
  character must have its original-module context and current PostgreSQL
  authority epoch validated before simulation. Storage failure or an expired
  lease halts that shard instead of running unauthorized game logic.
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
- [Live native world travel](service/src/lib.rs) snapshots original source
  game logic under the authoritative Universe mutex, commits native state,
  world presence and fencing epoch through PostgreSQL, and activates that
  same origin module in the destination world. Failed or ambiguous commits
  quarantine the affected native state instead of executing a duplicate.
  [Real database tests](service/tests/live_travel.rs) inject a destination
  engine failure, recover from the committed checkpoint in a new runtime,
  and verify successful return travel.
- [Trusted runtime recovery service](service/src/lib.rs) verifies a persisted
  session, reads its current native state and authoritative world placement,
  and restores the original engine module through the generic runtime.
  Expired/missing authority is rejected rather than assuming an empty world.
  A restarted process can reclaim an **expired** lease using the authenticated
  session, preserving original module/world identity while advancing fencing
  epochs. CI restores a character from PostgreSQL into fresh runtimes,
  including after simulating a dead server.
- [GitHub Actions CI](.github/workflows/ci.yml) compiling the Rust workspace,
  running module-level tests, applying PostgreSQL migrations and checking constraints.

### Test locally

```sh
cargo test --workspace --all-targets
cargo test -p oasis-native-process --test process --locked
cargo test -p oasis-contracts --test g0_contract --locked
cargo clippy --workspace --all-targets -- -D warnings
python3 tests/verify_structure.py
git submodule update --init --recursive
# The upstream source bridge is applied only once to a clean pinned checkout:
git -C engines/cave-story apply --check ../../adapters/cave-story/source-bridge.patch
git -C engines/cave-story apply ../../adapters/cave-story/source-bridge.patch
cargo test --manifest-path adapters/cave-story/Cargo.toml --lib
cargo test --manifest-path adapters/cave-story/Cargo.toml --test player_process
cargo test --manifest-path adapters/doom/Cargo.toml --lib
# With a disposable PostgreSQL 16 instance and PG* environment configured:
psql -v ON_ERROR_STOP=1 -f migrations/0001_core.sql
psql -v ON_ERROR_STOP=1 -f migrations/0002_indexes.sql
psql -v ON_ERROR_STOP=1 -f migrations/0003_transfers.sql
psql -v ON_ERROR_STOP=1 -f migrations/0004_world_travel.sql
psql -v ON_ERROR_STOP=1 -f migrations/0005_native_checkpoints.sql
psql -v ON_ERROR_STOP=1 -f migrations/0006_authority_recovery.sql
psql -v ON_ERROR_STOP=1 -f tests/schema_smoke.sql
psql -v ON_ERROR_STOP=1 -f tests/store_fixture.sql
DATABASE_URL="host=localhost user=oasis password=oasis_ci dbname=oasis" cargo test -p oasis-store --test postgres
DATABASE_URL="host=localhost user=oasis password=oasis_ci dbname=oasis" cargo test -p oasis-service --test postgres
```

**Status:** Schema/runtime foundations and independently tested original-source
functions are CI-verified. Cave Story's real source player, collision,
inventory and bullet execution remains a partial scene; DOOM has only source
math/geometry helpers. The full playable, cross-world authenticated MMO is
**not yet implemented**. See [G0 matrix](contracts/G0_REQUIREMENTS_MATRIX.md)
for requirement-by-requirement evidence and unknowns.

Live travel is atomic at the SQL layer and coordinated with in-process
native transitions; a failed destination is quarantined and recoverable.
Cross-process travel, cancellation of in-flight native steps, fencing of
external effects, production networking/authentication, spatially filtered
replication, GPU composition and full original game adapters remain open.
The loopback development gateway is not production client networking.

Cross-game behavior is **final black-box verification only**. No game-pair
compatibility branches or transformations are implemented.

The top-level OASIS source does not bundle game art or audio. **Caution:** the
pinned DOOM upstream submodule tracks `doom1.wad`; its presence in a recursive
checkout is not permission to include it in an OASIS binary, test artifact or
release. See [licensing audit](contracts/G0_FEASIBILITY_AND_LICENSES.md).
