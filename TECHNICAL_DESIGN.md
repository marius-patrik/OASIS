# OASIS — Technical Design Specification

**Revision:** 0.1 (2026-10-08)  
**Authority:** [`PRD.md`](PRD.md) defines the product. This document specifies the first implementation.  
**Status:** Design baseline and code skeleton; not a completed game platform.

## 1. Architectural invariant

A game integration knows **only the universal platform contract**. It never imports the other game, changes behavior based on a foreign game ID, or converts foreign gameplay into a pre-authored local analogue.

- **World-owning engine modules** implement terrain, environmental simulation, native world objects, and environment rendering.
- **Origin character modules** implement that character's original controller, physics logic, animation, and camera logic. They query foreign-world geometry through the platform port.
- **Origin item/capability modules** implement the original capability's behavior and may issue typed interaction requests against other entities.
- **Receiver-owning modules** apply authorized effects to the state they own and produce acknowledged outcomes.
- **OASIS** owns universal IDs, reference mapping, authorization, transactionality, routing, timing coordination, state transport, frame composition, and MMO infrastructure.

No requirement that a character's origin renderer becomes the world's renderer. The user's selected camera is an independently owned contribution; the world remains visible through its native pipeline.

**Proof obligation:** Two independently integrated engines coexist and interact with zero game-pair-specific code. Their cross-game behavior is a **final verification scenario**, not an implementation workstream.

## 2. Execution and deployment topology

```text
                   Client (launcher, camera, render composer)
                      | authentication / input / snapshots
                  Gateway / Session Router
                      | world instance routing
           +----------+-------------------+
           |                              |
     World Instance A                World Instance B
     Authoritative host             Authoritative host
     World engine modules           World engine modules
     Visiting native modules        Visiting native modules
     Generic runtime ports          Generic runtime ports
           |                              |
           +-----------+------------------+
                       | durable commands
                Universal Data Service
             PostgreSQL + blob artifacts
                       |
             Type Registry / Event Journal
```

- An **engine** is registered metadata and versioned module artifacts; it is not necessarily one process.
- One **world instance** owns a runtime composed of multiple **execution contexts**. A module context can host many entity instances; no complete game client is required per character.
- A native Rust module can be in-process or run in a worker. The contract and IDs are the same; process selection must not change observable gameplay semantics.
- The client and the server may host different halves of an originating engine. Authoritative calculations run on the server. Client rendering is a native module contribution and cannot directly mutate authoritative state.
- Render buffers, collision caches, and per-tick transforms are **not SQL records by default**; they are runtime data conveyed over shared memory, local channels, or network messages.

## 3. Database: physical schema v0.1

The executable PostgreSQL DDL is in:

- [`migrations/0001_core.sql`](migrations/0001_core.sql): 31 universal tables with keys and constraints.
- [`migrations/0002_indexes.sql`](migrations/0002_indexes.sql): indexes and one-active-presence rule.

### 3.1 Stable tables, variable record payloads

| Concern | Stable tables | Extensible data |
|---|---|---|
| Accounts | `users`, `players`, `characters`, `sessions` | Profile, custom attributes, character-specific state |
| Game registry | `types`, `assets`, `engines`, `engine_modules`, `engine_adapters`, `games`, `worlds`, `world_modules` | Native structure definitions, capabilities, binary artifacts |
| World hosting | `world_instances`, `spatial_frames`, `presences`, `execution_contexts` | Local simulation configuration and mappings |
| Entity system | `entity_definitions`, `entities`, `items`, `inventories`, `components`, `capabilities`, `entity_capabilities`, `relations` | Engine-origin definitions and arbitrary typed components |
| Exclusivity | `ownerships`, `locations`, `authority_leases` | Ownership and custody are separate, exclusive facts |
| Persistence | `state_documents`, `transactions`, `events`, `engine_bindings` | Checkpoints, commits, references to native implementations |

The listed physical tables are *not* one table per game. A registered type is addressed as `(namespace, name, version)`; its `schema` specifies the permissible shape of each namespaced JSONB `data` payload. Components let arbitrary game-specific values exist on a platform character without adding SQL columns.

**Native component wire representation:** `components.data` and
`state_documents` use the same recursively tagged `Value` codec
(`k` discriminant with `v` payload). This preserves original
`Bytes`, signed/unsigned integers, 128-bit references, nested
sequences/maps and opaque game extensions across first-time entity
loads *and* subsequent native snapshots. The canonical empty
component is `{"k":"map","v":{}}`; untagged JSON objects are not
valid native component values. General-purpose record `data`
fields elsewhere remain freely extensible JSONB. Unknown value tags
fail explicitly rather than falling back to lossy JSON coercion.

**Invariants enforced in SQL:** foreign keys, identity uniqueness, casefolded player handles, one active authoritative presence per entity, at most one exclusive location, compatible session-user-player-character associations, component presence ownership, nonnegative revisions, and format/shape checks on JSONB objects.

**Invariants enforced by the universal data service:** type-schema validation; immutable published definition/module versions; entity subtype registration (`items`, `characters`); containment-cycle prevention; logical consistency between registered world frames and presences; authorization; atomic transition protocols; event append-only rules and monotonic authority epochs. Triggers or stored procedures can harden these after the service semantics are tested.

`characters` intentionally **does not duplicate** `entities.definition_id` as suggested by the PRD's conceptual table catalog; entity definition has one physical source of truth. This is a normalization resolution, not a change to player/character semantics. `state_documents` references the checkpoint owner; its binary blob (if any) is an `assets` reference.

### 3.2 Ownership, containment, and presence

These must never be conflated:

- **Ownership:** `ownerships(entity_id, owner_entity_id)` — durable entitlement/control. An entity may be owned while physically elsewhere.
- **Location:** `locations(entity_id, container_entity_id | presence_id)` — exactly one exclusive physical/container location. A dropped item has a presence in its world; a stored item is located in a globally identified inventory entity.
- **Presence:** `presences(entity_id, world_instance_id, frame_id)` — authoritative participation in an active world. A visiting character can leave World A and join World B without changing entity ID, definition, or origin module.
- **Native handle:** runtime-only `(execution_context_id, native_slot)` — mapping that can change after restart or travel. It is never used as the stable public identity.
- **Proxy:** non-authoritative adapter-local representation for collision/render/targeting. A proxy is **not** a second item, character, or authoritative presence.

### 3.3 Definition vs. instance vs. module

A definition in `entity_definitions` points to registered type and original module. A particular sword is one row in `entities` plus `items`, with its mutable state in `components`. Its actual behavior is bound via `engine_bindings`, `capabilities`, and `entity_capabilities` to a versioned `engine_modules` artifact. Its owner, current container, and world presence remain independent facts.

Published definitions, code hashes, and interface versions must be treated as immutable. A new version is a *new definition/module registration*; existing items retain the original binding until explicitly migrated by authorized rules.

### 3.4 Transaction boundaries

A **drop** is a transaction changing custody from inventory to a world presence while leaving ownership unchanged unless a separate ownership operation is requested. A **pickup** changes location and, when game rules demand, may change ownership. A **trade** may change both.

- Lock affected `entities`, `ownerships`, `locations`, relevant `presences`, and `authority_leases` in consistent ID order (or use serializable transactions with retries).
- Validate authenticated actor permissions, expected `revision`, and current lease `epoch`.
- Enforce exclusivity in one SQL transaction; persist the new state and an idempotent `transactions` / `events` record before acknowledging success.
- Replay a duplicate `idempotency_key` by returning the committed outcome; never repeat the mutation.
- Never regard `expires_at` as sufficient fencing: every authoritative command carries the current epoch and a stale holder must be rejected.

A cross-world transfer is a **state-machine command**: quiesce origin → snapshot native modules → stage destination context → atomically revoke source authority, relocate presence/custody, advance epoch → resume destination. If commit fails, discard staged destination and resume origin. If destination resume fails after commit, retain a recoverable transition state, do not silently restore source authority. A crash-recovery worker reconciles from the last durable decision.

**Important:** `transactions` and `events` give durable auditability; they are not meant to write all game ticks.

## 4. Universal Rust interface v0.1

The reference crate [`contracts/src/lib.rs`](contracts/src/lib.rs) defines:

| Primitive / trait | Purpose |
|---|---|
| `Id`, `TypeRef`, `Value`, `EntityView`, `Snapshot` | Globally addressable and extensible typed state |
| `NativeHandle`, `ModuleDescriptor` | Engine-local object identity and published module requirements |
| `NativeModule` | Instantiation, native simulation stepping, checkpoint/restore, teardown |
| `WorldPort` | Universal geometry, spatial queries, frame transforms, entity views, effect requests |
| `InteractionReceiver` | Target-owned validation and state application |
| `RenderProvider` | Native camera state and render geometry/surfaces/overlays |
| `GameAdapter`, `AdapterRegistry` | Independent package registration and execution context creation |
| `ClockStep`, `InputIntent` | Native cadence and platform-routed input |
| `AuthorityStamp`, `InteractionRequest/Result` | Typed cross-owner effects with fencing and acknowledgements |

The crate uses only `std` types, exposing a minimal source-level shape. It does not yet prescribe a transport serialization/IDL, ABI stable plugin boundary, or GPU handle format. These are separate implementation decisions. The reference source is intended to be compiled and evolved, but it has **not** been build-tested in this environment.

### 4.1 Native ownership and effects

- A native character physics module consumes `WorldPort::geometry` or `query`; **that module** calculates movement and emits its own state changes.
- A native weapon module computes attack behavior and issues `InteractionRequest` with an operation identified by a registered `TypeRef`.
- The platform validates sender authority, routes by global target ID to the owner module, and returns an `InteractionResult`. The receiving module decides its own state update and sends a committed outcome.
- A module **must not** directly modify another module's exclusive state. If the target does not support the requested semantic interface, the receiver reports `Rejected` / `Unsupported` instead of inventing an equivalent mechanic.
- The runtime must order and deduplicate interaction requests, reconcile prediction, and separate logical outcome from renderer-only effects.

### 4.2 Space, dimensions, and time

- `spatial_frames` describe basis, unit scale, and reference hierarchy. Frame conversions are explicit and testable.
- Native 2D simulation uses its own 2D frame and constraints; a render contributor can map its geometry or sprite into a 3D displayed scene without rewriting the entity.
- Native 3D characters in 2D worlds retain their models and native controller, while environmental boundaries are expressed through world geometry and constraints. Extra movement dimensions are not magically invented.
- Different engines can tick at different rates. Each context owns its own clock; event ordering uses timestamps and platform sequence/order rules. Clients interpolate visuals without gaining simulation authority.

### 4.3 Rendering

An engine can submit **native geometry**, a **color/depth surface**, or **overlay** output using one render-contribution contract. Each contribution carries frame ID, camera reference, and synchronization metadata. The compositor decides depth ordering and camera transforms; it does not reimplement mesh shading, animation, or the source physics.

The concrete GPU backend is deliberately postponed; rendering across different graphics APIs may require a copy path. Claims of zero-copy, identical lighting, or universal render compatibility must be measured, not presumed.

## 5. Platform services and API operations

The Rust traits cover native execution. Persistent services need a separate transport-neutral IDL with at least these operations:

| Domain | Core commands / queries |
|---|---|
| Identity | `CreateUser`, `CreatePlayer`, `CreateCharacter`, `OpenSession`, `SelectCharacter` |
| Registry | `RegisterType`, `RegisterAsset`, `RegisterEngine`, `RegisterModule`, `RegisterAdapter`, `PublishDefinition`, `BindCapability` |
| Data | `GetEntity`, `QueryEntities`, `GetComponents`, `PatchComponent(expected_revision)`, `GetRelations` |
| World | `CreateWorld`, `StartInstance`, `JoinWorld`, `LeaveWorld`, `TravelWorld`, `ListWorlds` |
| Ownership | `PutInInventory`, `Drop`, `Pickup`, `TransferOwnership` (all idempotent transactions) |
| Authority | `AcquireLease`, `RenewLease`, `FenceLease`, `RevokeLease` |
| Live runtime | `SubmitInput`, `QueryGeometry`, `SubmitInteraction`, `PublishSnapshot`, `SubscribeArea`, `AcknowledgeOutcome` |

Every mutating API call identifies the principal and requested resource, carries a request/idempotency ID, passes expected revision where relevant, and produces a typed status/outcome. Version negotiation occurs before native context instantiation. Unauthorized/unknown behaviors are failures, not implicit conversion or untyped scripting.

## 6. Testing strategy: prove *contracts* before games

First implement two **synthetic** test modules in separate packages, with different native clocks, different dimensions, and no imports from each other. They independently pass the same adapter compliance kit:

1. Register type/definition/capability with a custom namespaced payload.
2. Create several entity instances and independent execution contexts; confirm no global-state leakage.
3. Persist/restore state with stable universal IDs and valid native-handle regeneration.
4. Export world geometry to a generic visiting physics module.
5. Emit a generic typed interaction to an unrelated target module, receive authorized outcome, and reject stale authority.
6. Supply geometry and raster outputs to the compositor, with coherent frames and distinct cameras.
7. Enforce one exclusive item location and no duplicate result for retried transfer.
8. Run across 30/60/variable-Hz module contexts without interpreting another module's native game ID.

Then integrate DOOM and Cave Story independently. Each must run a stand-alone multiplayer world and pass exactly the same compliance suite. **Do not integrate one against the other during adapter implementation.**

The final black-box interoperability report is run only after both independent integration gates pass. If a test fails, fix a universal defect, or an adapter's failure to satisfy its universal contract. No partner-game name may appear in an adapter's implementation code as a compatibility branch.

## 7. Practical boundaries and unknowns

- A typed universal protocol does not guarantee that every conceivable original mechanic has a meaningful effect on every possible target. Standard interactions need explicitly defined, versioned semantics; genuinely unsupported interactions must be reported.
- A Rust rewrite being feature-complete **does not** guarantee headless execution, thread safety, independently instantiable player physics, multiplayer support, or full reusable rendering.
- The two games remain candidates until independent feasibility spikes measure extraction difficulty and licensing/distribution constraints.
- Untrusted native module execution is unsafe in-process. MVP initially assumes audited/trusted modules; distribution and sandboxing of arbitrary third-party code are not solved here.
- Render composition and physics coupling can be expensive. Performance expectations need baselines from actual engine runs before budgets can be fixed.
- The initial schema has generic type definitions in SQL, but full registered JSON schema validation and authorization belong in the yet-to-be-implemented Data Service. Do not expose the migration as an unsafe direct-write public API.

## 8. Decisions and non-decisions

**Committed:** universal relational tables, typed record extensibility, portable identity, native code ownership, independent game adapters, composable module ports, authoritative multiplayer, verifiable final-only cross-game compatibility.

**Provisional:** PostgreSQL as storage engine, Rust as implementation language, in-process vs worker module layout, DOOM/Cave Story as candidates, native GPU composition implementation, wire IDL, and physical execution packaging.

**Excluded from MVP:** AI ingestion, automated reverse engineering, marketplace, game-pair compatibility patches, VR-first UI, full global-scale deployment.

## 9. Research basis

- [SkyCraft design](https://github.com/chasmlol/SkyCraft/blob/main/docs/DESIGN.md) — native logic and geometry/proxy/render bridges; its design docs can precede working implementation.
- [libsm64](https://github.com/libsm64/libsm64) — imported movement/renderer exposed by a library.
- [room](https://github.com/sunsided/room) — Rust Doomgeneric port candidate.
- [doukutsu-rs](https://github.com/doukutsu-rs/doukutsu-rs) — Rust Cave Story rewrite candidate.
- [PostgreSQL 18 CREATE TABLE](https://www.postgresql.org/docs/18/sql-createtable.html) — constraints and deferrability; DDL intentionally uses PG15-compatible features.