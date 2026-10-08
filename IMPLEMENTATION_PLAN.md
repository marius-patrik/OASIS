# OASIS — Implementation Plan v0.1

This is an implementation sequence for the PRD and [technical design](TECHNICAL_DESIGN.md), **not** a claim that any game has already been integrated.

## Gate 0: Contract and candidate feasibility (parallel)

- [x] Pin exact SHAs and inspect source entry points for the DOOM `room` and Cave Story `doukutsu-rs` candidates; license/asset packaging review is still required.
- [ ] For each, identify headless game tick, state ownership, per-character controller, physics/geometry queries, camera and render entry points.
- [ ] Verify whether two independent instances can coexist safely (process-isolation acceptable) and whether state can be restored from snapshots.
- [ ] Audit copyleft obligations and distribution of executable code vs. game assets.
- [ ] Produce feasibility scorecards; substitute another Rust implementation *only* if evidence defeats a candidate.

**Exit:** two viable independent ports or documented replacements, and a frozen minimal contract surface for implementation.

## Gate 1: Data substrate

- [x] Apply core, indexes, custody and travel migrations to PostgreSQL 16 through CI.
- [ ] Build registry validation (namespace/version/payload schema) and a guarded data-service API; game modules cannot directly write tables.
- [ ] Implemented read APIs for persisted identity/session, entity definition, components and native snapshots; complete guarded create/update data-service APIs remain open.
- [x] Implement SQL transactional custody/ownership moves, journal/idempotency, optimistic revision checks, and monotonic lease fencing; Rust-to-SQL integration is still open.
- [ ] PostgreSQL smoke tests and Rust integration tests now cover two-way world travel, native snapshots, replay and stale-revision rollback; real concurrent process-crash recovery and data-service authorization remain open.

**Exit:** SQL and application-level invariant tests green; a user, character, world, original item definition, and arbitrary game-specific typed components persist and round-trip.

## Gate 2: Universal runtime and synthetic adapters

- [ ] `contracts` compiles in CI; the stable out-of-process ABI/IDL remains open.
- [x] Build in-process `WorldPort` dispatcher, native execution contexts, native handle registry, module clocks, and snapshots/restore.
- [ ] Build geometry/spatial query and typed interaction routing/authority verification.
- [x] Implement camera owner selection, frame composition plans and render contribution registration; GPU composition remains open.
- [ ] Build two artificial fixture modules in isolated crates; different axes, 2D/3D and native frequencies.
- [ ] Implemented an in-process multi-world coordinator, session fencing, fixed-rate native-module ticking and loopback TCP test gateway; production networking, persistence bindings, interest filtering and restart recovery remain open.
- [ ] Run the conformance kit against both fixtures independently and together; disallow fixture-pair conditionals.

**Exit:** runtime demonstrates generic composition and multiplayer without either source game.

## Gate 3A: DOOM adapter — independent lane

- [ ] Extract/host original DOOM controller, simulation, weapon capability, world geometry and native render output via the universal contract.
- [ ] Isolate original static globals or host separate contexts; retain native behavior.
- [ ] Register all definitions, native bindings and game-world records through the universal API.
- [ ] Run DOOM-only multiplayer, persistence, restart, and generic fixture conformance.

**Exit:** DOOM-only world fully passes published adapter conformance.

## Gate 3B: Cave Story adapter — independent lane

- [ ] Extract/host Cave Story simulation, controller, physics/geometry, item behaviors, camera/render pipeline through the **same** contract.
- [ ] Keep its original 2D frame and native mechanics; do not change DOOM adapter or platform for its private types.
- [ ] Register records, assets and native bindings through the universal API.
- [ ] Run Cave-Story-only multiplayer, persistence, restart, and generic fixture conformance.

**Exit:** Cave Story-only world fully passes published adapter conformance.

## Gate 4: Operational integration, no game-pair code

- [ ] Deploy both independent worlds on the same shared platform.
- [ ] Audit source dependency graphs: `adapter-doom` must not import or reference `adapter-cave-story` and vice versa.
- [ ] Confirm neither adapter contains foreign-game names, translation tables or pair exceptions.
- [ ] Check authoritative replication, storage and asset provisioning, cross-world transaction recovery, version mismatch handling, and basic performance.

**Exit:** complete platform and two independent adapter releases frozen for verification.

## Gate 5: Final black-box verification only

- [ ] Travel the same persistent character between both worlds.
- [ ] Observe original-native camera/controller/render contribution inside the other environment.
- [ ] Transfer an actual instance of an original item to the second world and exercise its originating module.
- [ ] Apply at least one authorized effect to a foreign entity via generic interfaces.
- [ ] Drop/trade the same item to another player; carry it in the reverse direction; verify ID remains unchanged and survives restart.
- [ ] Confirm zero compatibility code was written for either game pair.

**On failure:** return to generic runtime contract / individual adapter defect, add a synthetic reproducer, rerun all adapter conformance tests, then restart final verification. Never make a partner-specific exception.

## Verified implementation baseline (2026-10-08)

- Universal Rust workspace, adapter catalog, native-module runtime, authority fencing, clock scheduling, render composition planning, and synthetic conformance tests compile and pass GitHub Actions.
- PostgreSQL schema, custody transfer, authoritative presence-travel, and **atomic native simulation checkpoints** execute and pass CI. The Rust `oasis-store` client round-trips real native state through PostgreSQL; cold-start state restoration is now available through `oasis-service`, while live travel coordination, expired-lease reclamation and crash-safe handoff remain open.
- The DOOM and Cave Story Rust sources are pinned as Git submodules and compile independently in native source CI. Their OASIS adapters do not yet exist.
- The development TCP gateway is loopback-only and does not provide a production player API or full gameplay state replication.
- Final black-box interoperability has **not** been attempted; cross-game code must remain absent.

## Workstream separation

- **Platform core:** schema, data service, universal runtime, authoritative MMO hosting, generic tests.
- **Adapter A:** DOOM source extraction, DOOM-only tests, no edits to Adapter B.
- **Adapter B:** Cave Story source extraction, Cave-Story-only tests, no edits to Adapter A.
- **Validation:** contract suite and independent black-box verification report; no features in the validation lane.

## Do not begin with

- AI-powered ingestion, agent modding, arbitrary proprietary-game compatibility.
- A new renderer or physics solver that replaces native engine behavior.
- Pairwise integration demonstrations before reusable contracts exist.
- An inventory SQL JSON blob that cannot enforce exclusive custody.
- Generic JSON records without dedicated user/player/character/item/world tables.

## Operational acceptance definition

All generic contracts pass with synthetic fixtures. Two independently implemented, complete game integrations pass unchanged conformance tests, provide actual multiplayer worlds, and pass final cross-game black-box scenarios without pair-specific code or duplicated persistent entities.