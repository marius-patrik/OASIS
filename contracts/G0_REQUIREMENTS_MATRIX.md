# G0 — PRD requirements-to-tests and code evidence matrix

**Audited baseline:** `main@001692dc6310111f51ff5b4858c2ead66d78bcb1` (2026-10-11). [Main universal CI green](https://github.com/marius-patrik/OASIS/actions/runs/38151221379). This is a **code/source inspection**, not a claim that complete original games have been run by this G0 agent. Contract test run evidence must refer to the PR head, not the base SHA.

**Status definitions:** **VERIFIED** = explicitly scoped assertion exercised by a cited passing baseline test; **UNVERIFIED** = missing definitive executable evidence; **BLOCKED** = a specifically identified missing implementation, asset/license clearance, or operational prerequisite. A partial primitive is not a verified full PRD clause. Labels apply at the *full clause* level, with partial evidence called out explicitly.

## PRD §11 acceptance clauses — all 17 individually accounted for

| ID / PRD line | Full acceptance requirement | Status | Existing code/test evidence; missing final proof |
|---|---|---|---|
| P-01 / [L335](../PRD.md#schema-and-platform) | Universal constrained users, players, characters, items, worlds and supporting tables | **VERIFIED (physical schema only)** | [migrations/0001_core.sql](../migrations/0001_core.sql), [0002](../migrations/0002_indexes.sql), [schema_smoke.sql](../tests/schema_smoke.sql), [baseline CI](https://github.com/marius-patrik/OASIS/actions/runs/38151221379). Service auth not implied. |
| P-02 / L336 | Both games and arbitrary native records share validated type/registry without game migrations | **UNVERIFIED** | Fixed schema, tagged codec [store/src/codec.rs](../store/src/codec.rs); complete immutable version registration and schema/payload authorization missing. Test both registered game definitions on real SQL. |
| P-03 / L337 | Identity, ownership, custody, state, definitions separate/queryable | **VERIFIED (SQL paths)** | [schema](../migrations/), [store PostgreSQL tests](../store/tests/postgres.rs). End-user authorized API remains missing. |
| P-04 / L338 | Multiple simultaneous connections share persistent authoritative servers | **BLOCKED** | [runtime/src/gateway.rs](../runtime/src/gateway.rs) TCP is loopback development only. No production auth, multi-client replication/reconnect/interest. |
| P-05 / L339 | Crashes/cross-server handoff cannot duplicate items or authority | **UNVERIFIED** | [store tests](../store/tests/postgres.rs), [service live travel](../service/tests/live_travel.rs) cover targeted SQL and in-process recovery; no multi-node stale-worker kill or SQL ambiguous-commit chaos. |
| P-06 / L343 | Both original source games correctly run as independent hosted worlds | **BLOCKED** | [DOOM adapter](../adapters/doom/src/lib.rs) only native math; [Cave Story worker](../adapters/cave-story/src/gameplay.rs) partial native player/weapon tick. Neither full world/renderer. |
| P-07 / L344 | Native logic fidelity, no invented foreign mechanics | **UNVERIFIED** | Pinned original [modules](../engines/README.md), [Cave patch](../adapters/cave-story/source-bridge.patch), [source CI](../.github/workflows/native-engines.yml) establish **specific functions only**; all behaviors need source-vs-host gameplay regressions. |
| P-08 / L345 | Both game adapters pass unchanged neutral complete conformance | **BLOCKED** | [neutral vectors](src/neutral.rs) are structural-only; [G0 tests](tests/g0_contract.rs) do not certify complete gameplay; no independent full conformance runner or adapters implementing all v1 roles. |
| P-09 / L346 | Multiple character/world instances avoid source-global leakage | **UNVERIFIED** | [native-process](../native-process/src/lib.rs) isolates partial modules in OS children, but not two complete simultaneously ticking original-game contexts with deterministic recovered state. |
| P-10 / L347 | Render, native camera, geometry, controls and effects work with synthetic partners | **BLOCKED** | [render planner](../runtime/src/render.rs) only composition metadata; no real surface import/display or source renderer. [G0 v1 packet](src/universal.rs) enforces shape, not semantic execution. |
| P-11 / L351 | Persistent same character enters both worlds, preserves ID/attributes | **BLOCKED** | Generic in-process travel [service](../service/src/lib.rs) works with partial/synthetic state; original engines unhosted end-to-end. **Final issue #7 only.** |
| P-12 / L352 | Origin controller/camera/rendering operate in foreign world | **BLOCKED** | Missing complete per-origin controller and native rendered output; final verification only. |
| P-13 / L353 | Original item definition/implementation survives cross-world journey | **BLOCKED** | Native ammo isolated capability exists, but actual playable world-to-world custody not yet exercised; final verification only. |
| P-14 / L354 | Source-native item ability applies receiver-authorized observable effect | **BLOCKED** | [InteractionRequest](src/lib.rs) typed authority metadata exists; trusted fenced/idempotent receiver commit and full native item implementation missing. Final verification only. |
| P-15 / L355 | Drop/pickup/trade/restart preserve exact item ID and custody | **BLOCKED** | SQL custody primitives tested but no two authenticated users + original live item + crash/restart; final verification only. |
| P-16 / L356 | Reverse-direction original-game flow works | **BLOCKED** | Synthetic/partial SQL round trips do not prove original-game full native state; final verification only. |
| P-17 / L357 | Zero source-pair-specific branches/mappings/compatibility code | **UNVERIFIED** | [tests/verify_structure.py](../tests/verify_structure.py) checks source boundaries; final scan dependency graph + compiled artifacts after A–D and black-box verification required. |

## PRD goals §2, invariants §4, API §§5–8 and nonfunctional §11

| ID | Normative requirement | Status | Evidence / falsifying test needed |
|---|---|---|---|
| G1 / §2 | Universal persistent identity, worlds, typed components, ownership | **UNVERIFIED** | SQL structures and codec verified in P-01/03, but end-to-end authenticated API and registry missing (P-02/04). |
| G2 / §2 | Add independent game without foreign imports | **UNVERIFIED** | Separate adapter crates, structural check; full v1 adapter proof not yet run (P-08/17). |
| G3 / §2 | Original native world/controller/physics/item behaviors | **BLOCKED** | Partial Cave controller, DOOM numeric primitives; complete original loops not hosted. |
| G4 / §2 | Mixed original camera/render/geometry pipelines | **BLOCKED** | Render plan only, no composited pixels or live native geometry bridge. |
| G5 / §2 | Persistent authoritative MMO, players, transferable items | **BLOCKED** | No production authenticated multiplayer/recovery. |
| G6 / §2 | Universal schema/host unaffected by new genres | **UNVERIFIED** | No independent synthetic 2D/3D full module implementations, only neutral vectors. |
| G7 / §2 | Falsifiable generic conformance and final black-box verification | **BLOCKED** | Conformance runner missing; #7 must wait for fully functional A–E. |
| I1 / §4 | Stable global ID is distinct from ephemeral native slot | **VERIFIED (contract representation)** | [contracts/src/lib.rs](src/lib.rs) `Id` /`NativeHandle`, test `native_handles_are_scoped_to_execution_contexts`, SQL identities. Need actual multi-context runtime proof. |
| I2 / §4 | Origin module retains execution, not destination imitation | **UNVERIFIED** | Architectural port exists; foreign-world native execution untested. |
| I3 / §4 | Single authoritative location/ownership and legal transfers | **UNVERIFIED** | SQL schema + simulated traver/retry tests; distributed failure races untested. |
| I4 / §4 | Only source owner can interpret native state and update entity | **UNVERIFIED** | v1 native bytes and provenance added; no trusted worker authentication/data-service gates. |
| I5 / §4 | No per-game persistent schema/table changes | **VERIFIED (current schema inventory)** | [migrations](../migrations/) use universal tables; future additions need schema graph audit. |
| A1 / §5 | Versioned type and capability registry, schema validation | **BLOCKED** | Stored type rows yes; immutable registry and validation service absent. |
| A2 / §5 | Immutable definitions, native artifact hashes and bindings | **UNVERIFIED** | Some schema refs; no pinned artifact validation during full worker startup. |
| A3 / §5 | Idempotent lease-fenced transfer and recovery under SQL ambiguity | **UNVERIFIED** | [service tests](../service/tests/live_travel.rs) target local failures, not multi-node. |
| API-1 / §6 | Authenticated account/player/session/character creation | **BLOCKED** | Internal read methods exist, public hardened create/update API absent. |
| API-2 / §6 | Game/module/world/type/asset registry and launches | **BLOCKED** | Catalog memory registration, not operational artifact registry/launch. |
| API-3 / §6 | Inventory, custody, interactions, travel authorized API | **BLOCKED** | Trusted internal store and scheduler, no production client authorization. |
| API-4 / §6 | Real-time world routing, input, replication, snapshot/deltas | **BLOCKED** | Only loopback TCP and development runtime; no full client/protocol. |
| N-01 / §7 | Lifecycle load/init/step/suspend/checkpoint/restore/destroy | **UNVERIFIED** | [v1 lifecycle](src/universal.rs) contract + tests; old worker supports subset but not full engine. |
| N-02 / §7 | Origin input/controller native processing | **UNVERIFIED** | Cave player tick partial; DOOM complete controller absent. |
| N-03 / §7 | Geometry native colliders/materials/frame transforms | **UNVERIFIED** | [WorldPort](src/lib.rs) basic dispatch, [v1 NativeContact](src/universal.rs); production resolver not implemented. |
| N-04 / §7 | Item source effect request, receiver fenced apply/ack | **BLOCKED** | [v1 EffectReceipt](src/universal.rs) structural checks; no store-backed receiver commit. |
| N-05 / §7 | Source native camera and rendered geometry/pixels/depth | **BLOCKED** | [v1 NativeSurface](src/universal.rs) validates buffers; no original-game pixels or display. |
| N-06 / §7 | Native clocks, multiple frequencies, authority | **UNVERIFIED** | [scheduler](../runtime/src/scheduler.rs), rational v1 manifest; simulation quality and multi-worker phase alignment untested. |
| N-07 / §7 | Full source-owned portable snapshots and cold resume | **BLOCKED** | No complete DOOM thinker/RNG snapshot; Cave bullet/timer/script/NPC state omitted. |
| N-08 / §7 | Process-level isolation without global leakage | **UNVERIFIED** | Child process tests verify primitive isolation; no complete source engine contextual replay. |
| M-01 / §8 | Remote hosting, auth, encrypted clients, reconnect/replication | **BLOCKED** | Current loopback-only gateway; missing. |
| M-02 / §8 | Durable custody, leases, collision-safe effects, cross-world travel | **UNVERIFIED** | SQL primitives exist; concurrent authenticated real-world tests absent. |
| NF-01 / §11 | Benchmark frame pacing, CPU/GPU memory, latency, reconciliation | **BLOCKED** | No end-to-end renderer/client or capacity measurements. |
| NF-02 / §11 | Disconnect, malformed payload, stale leases, crashed worker, contention | **UNVERIFIED** | Isolated negative tests exist; no full multi-process chaos matrix. |
| L-01 / §12 | Distributable rights/third-party assets and GPL compliance | **BLOCKED** | See [source feasibility and licensing](G0_FEASIBILITY_AND_LICENSES.md); no release/commercial asset clearance. |

## Evidence discipline

1. GitHub Actions [CI](../.github/workflows/ci.yml) runs Rust workspace unit tests, source-agnostic worker process tests and PostgreSQL migration/service tests. It **does not** presently demonstrate A–D full-game conformance.
2. [Native Engine Sources](../.github/workflows/native-engines.yml) checks exact upstream commits and runs per-adapter tests. It **does not** execute two complete original games/worlds, full snapshot-replay, or renderer pixels.
3. [`contracts/tests/g0_contract.rs`](tests/g0_contract.rs) is a neutral **envelope** suite: distinguish evidence for serialization, context scope, malformed buffer rejection and effect receipt identity from gameplay.
4. Status **VERIFIED** never covers anything beyond the parenthetical scope. G0 cannot close child issues #4, #5, #6, #7. Final black-box game interaction is intentionally reserved to #7 once independent source proofs are green.
