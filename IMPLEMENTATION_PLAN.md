# OASIS — Implementation Plan v0.1

This is an implementation sequence for the PRD and [technical design](TECHNICAL_DESIGN.md), **not** a claim that any game has already been integrated.

## Gate 0: Contract and candidate feasibility (parallel)

- [ ] Pin exact SHAs and inspect source/license/asset packaging for the DOOM `room` and Cave Story `doukutsu-rs` candidates.
- [ ] For each, identify headless game tick, state ownership, per-character controller, physics/geometry queries, camera and render entry points.
- [ ] Verify whether two independent instances can coexist safely (process-isolation acceptable) and whether state can be restored from snapshots.
- [ ] Audit copyleft obligations and distribution of executable code vs. game assets.
- [ ] Produce feasibility scorecards; substitute another Rust implementation *only* if evidence defeats a candidate.

**Exit:** two viable independent ports or documented replacements, and a frozen minimal contract surface for implementation.

## Gate 1: Data substrate

- [ ] Apply `0001_core.sql`, `0002_indexes.sql` to an actual PostgreSQL 15+ instance.
- [ ] Build registry validation (namespace/version/payload schema) and a guarded data-service API; game modules cannot directly write tables.
- [ ] Build identity, players, characters, items, definitions, component/state, relationships, worlds, and native module registration.
- [ ] Implement transactional custody/ownership moves, journal/idempotency, optimistic revision checks, and monotonic lease fencing.
- [ ] Exercise concurrency/race/crash tests using *synthetic* data only.

**Exit:** SQL and application-level invariant tests green; a user, character, world, original item definition, and arbitrary game-specific typed components persist and round-trip.

## Gate 2: Universal runtime and synthetic adapters

- [ ] Turn `contracts` into a compiled, versioned Rust crate; define an ABI/IDL for out-of-process transports.
- [ ] Build `WorldPort` dispatcher, native execution contexts, native handle registry, module clocks, snapshots/restore.
- [ ] Build geometry/spatial query and typed interaction routing/authority verification.
- [ ] Implement generic camera owner selection, frame composition interface, and render contribution registration.
- [ ] Build two artificial fixture modules in isolated crates; different axes, 2D/3D and native frequencies.
- [ ] Implement server authoritative networking, sessions, interest filtering, world lifecycle, and restart recovery.
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