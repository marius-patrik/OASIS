# OASIS — Product Requirements Document

**Status:** Architecture baseline for implementation  
**Version:** 1.0  
**Date:** 2026-10-08  
**Name:** OASIS is a working codename, not a final brand or trademark decision.

## 1. Product summary

OASIS is a persistent, multiplayer game platform in which **independently implemented games are interoperable by design**. The experience resembles a conventional MMO or creation platform (e.g., Roblox or Fortnite's creator ecosystem), except that a character, item, or executable mechanic is not confined to the game in which it originated.

OASIS supplies a **universal relational database with extensible records**, a **universal API**, an **authoritative MMO runtime**, and a **standard engine-adapter/module contract**. A game supplies its world and native gameplay implementations. A player can keep one persistent identity and character across independently hosted worlds.

**Foundational invariant:** Two games must become compatible by *independently* implementing the same platform contract. Neither game may contain knowledge of the other. Cross-game interaction is an **end-to-end verification outcome**, not a game-pair-specific implementation task.

### What is distinctive

- Fixed, meaningful universal tables (`users`, `players`, `characters`, `items`, `worlds`, etc.), whose records may carry arbitrary, typed, versioned data.
- Native game logic remains executable as reusable modules: character controller, camera, physics, weapons, simulation, rendering, and other subsystems.
- A world continues to use its own environment, renderer, simulation, and game rules; visiting characters, items, and capabilities retain the implementations that define them.
- Generic composition contracts exchange geometry, interactions, rendered contributions, input, clocks, and state. These contracts replace pairwise game-to-game glue.
- Universally identified objects, ownership, inventory, persistence, and sessions span all worlds.

## 2. Product goals and non-goals

### Goals

**G1 — Universal persistence.** One schema and API represent user accounts, player personas, characters, items, worlds, game definitions, assets, capabilities, and arbitrary state from any integrated game.

**G2 — Independent game integration.** One adapter per game/engine integration, without explicit imports, conditionals, converters, or special cases for other games.

**G3 — Native behavior fidelity.** Origin implementations remain authoritative for the logic they own, rather than recreating foreign mechanics in a destination game.

**G4 — Native scene composition.** Multiple rendering and physics implementations can contribute to one experience through reusable platform contracts.

**G5 — MMO semantics.** Shared identities, multiplayer sessions, authoritative world servers, persistent ownership, transferable items, and recoverable state.

**G6 — Genre independence.** Adding a new genre introduces definitions/modules/adapter implementations, not game-specific database tables or platform-core branches.

**G7 — Testable universality.** Automated contract tests and final cross-game verification can falsify the claim of interoperability.

### Not goals in v1

- Automatic game ingestion, AI reverse engineering, or adapter generation.
- Supporting arbitrary closed-source commercial games, bypassing DRM/anti-cheat, or distributing proprietary assets.
- Replacing the source games' renderers, controllers, physics, or gameplay with newly invented equivalents.
- Authoring a DOOM↔Cave Story compatibility patch, shared weapon conversion, or any bespoke cross-game feature.
- A marketplace, creator economy, VR hardware support, polished social UI, user scripting editor, or global production-scale sharding.
- A guarantee that all conceivable interactions retain exactly the same outcome across incompatible simulation models. Unsupported behavior must be reported rather than silently forged.

## 3. Users and core experience

### Participants

- **User:** authenticated account and authorization principal.
- **Player:** one durable persona controlled by a user (a user may have multiple personas).
- **Character:** portable in-universe entity controlled by a player; a player may own multiple characters.
- **World owner/developer:** publishes a world definition and provides compatible native modules/adapter.
- **Server operator:** hosts authoritative world instances and associated execution contexts.

### Primary journey

1. Create an account, player persona, and character.
2. Select and join a hosted world running a registered game.
3. Play the actual game with other connected players; character state, possessions, and game-specific attributes persist.
4. Leave and enter another world without replacing the character's durable identity or its items.
5. Continue playing using the independently registered engine modules. Any cross-game effects arise solely from universal interfaces.

Players should not have to install or understand custom pairwise integrations.

## 4. Conceptual model and terminology

| Concept | Definition |
|---|---|
| **Engine** | A family of executable implementations supplying game/world/character systems. |
| **Engine module** | Versioned, independently addressable execution unit (world simulation, controller, weapon logic, renderer, camera, etc.). This is a logical interface: it need not be a separate process or library. |
| **Adapter** | Implements the universal contract for a given engine/game implementation; exposes native operations and consumes foreign entities/interaction inputs. |
| **Game** | A registered package of definitions, assets, worlds, and engine modules constituting a playable experience. |
| **World** | Durable definition of an environment, its native game modules, rules, and content. |
| **World instance** | A running, server-authoritative execution of a world; many instances may implement one world definition. |
| **Entity** | Globally identified, durable or transient in-universe object; a common identity for characters, items, constructions, NPCs, and other objects. |
| **Definition** | Versioned specification/archetype referenced by entity instances. |
| **Component** | Typed, extensible piece of data attached to an entity or presence. |
| **Capability** | Callable behavior exposed by an engine module and attached to entities/definitions where appropriate. |
| **Presence** | An entity's participation in a particular active world instance, distinct from its durable identity. |
| **Execution context** | Runtime instantiation of an engine module for a world instance and/or a set of entities. |
| **Projection/proxy** | Non-authoritative native representation used by another engine to perceive/interact with an entity. Never a second durable entity or duplicate item. |
| **Authority** | Exclusive right to commit a particular mutation or simulation outcome, fenced and transferable. |

### Required invariants

1. `user ≠ player ≠ character ≠ presence`; no one-to-one assumption is embedded in the model.
2. `engine ≠ game ≠ world ≠ world_instance`.
3. Every entity has one stable universal ID; presence and native handles are replaceable references.
4. Origin module, currently executing module, world-owning module, and authoritative server may differ.
5. Behavior and assets are identified by immutable or versioned definitions; individual mutable state belongs to instances.
6. No adapter knows another adapter's private object model.
7. Both 2D and 3D entities remain native; projection into a common displayed scene does not transform their underlying identity or code.

## 5. Universal database requirements

### 5.1 Storage principle

Use a **stable relational core plus typed extensible record payloads**, not a single opaque `records` table and not one schema per game.

- Stable columns are reserved for identity, foreign keys, versions, lifecycle, ownership, authority, and other enforceable invariants.
- Every applicable record may include `data JSONB NOT NULL DEFAULT '{}'` and a `type_id` or `definition_id` referring to a versioned type definition.
- Type definitions describe data shape and constraints; support both strict fields and namespaced extensions. Arbitrary data is allowed **within declared extension rules**, not silently unvalidated.
- Components hold larger or independently updated dynamic state; do not duplicate authoritative values in both JSONB and relations.
- Live high-frequency physics/animation data resides in world/engine memory and network snapshots; the database stores checkpoints, portable state, durable mutations, and transaction history.
- SQL is an initial physical choice (PostgreSQL); the universal API and logical schema remain independent of database vendor.

### 5.2 Concrete logical table catalog

**Identity**

| Table | Key fixed fields (besides `data`, timestamps, and revision where relevant) |
|---|---|
| `users` | `id PK`, `auth_subject UNIQUE`, `status` |
| `players` | `id PK`, `user_id FK users`, `entity_id UNIQUE FK entities`, `handle UNIQUE` |
| `characters` | `entity_id PK FK entities`, `player_id FK players`, `definition_id FK entity_definitions` |
| `sessions` | `id PK`, `user_id FK users`, `player_id FK players`, `active_character_id FK characters NULL`, `expires_at`, `status` |

**Registry and creation**

| Table | Key fixed fields |
|---|---|
| `types` | `id PK`, `namespace`, `name`, `version`, `category`, `schema JSONB`, `supersedes_id FK types NULL`; unique `(namespace,name,version)` |
| `assets` | `id PK`, `content_hash UNIQUE`, `media_type`, `byte_length`, `storage_uri`, `data JSONB` |
| `engines` | `id PK`, `namespace`, `name`, `version`, `data JSONB` |
| `engine_modules` | `id PK`, `engine_id FK engines`, `module_kind`, `version`, `artifact_id FK assets`, `interface_manifest JSONB` |
| `engine_adapters` | `id PK`, `engine_id FK engines`, `version`, `artifact_id FK assets`, `contract_version`, `declared_interfaces JSONB` |
| `games` | `id PK`, `name`, `default_engine_id FK engines NULL`, `data JSONB` |
| `worlds` | `id PK`, `game_id FK games`, `definition_id FK entity_definitions NULL`, `data JSONB` |
| `world_modules` | `world_id FK worlds`, `module_id FK engine_modules`, `role`, `settings JSONB`; PK `(world_id,module_id,role)` |
| `world_instances` | `id PK`, `world_id FK worlds`, `status`, `authority_node`, `created_at`, `checkpoint_id FK state_documents NULL` |

**Entities and content**

| Table | Key fixed fields |
|---|---|
| `entity_definitions` | `id PK`, `type_id FK types`, `origin_module_id FK engine_modules NULL`, `version`, `data JSONB` |
| `entities` | `id PK`, `definition_id FK entity_definitions`, `origin_module_id FK engine_modules NULL`, `revision`, `data JSONB` |
| `items` | `entity_id PK FK entities`, `data JSONB`; do not duplicate definition/ownership columns already elsewhere |
| `inventories` | `entity_id PK FK entities`, `owner_entity_id FK entities`, `data JSONB` |
| `components` | `id PK`, `entity_id FK entities`, `type_id FK types`, `presence_id FK presences NULL`, `data JSONB`, `revision` |
| `capabilities` | `id PK`, `name`, `type_id FK types`, `module_id FK engine_modules`, `entrypoint`, `interface_schema JSONB`, `data JSONB` |
| `entity_capabilities` | `entity_id FK entities`, `capability_id FK capabilities`, `binding JSONB`; PK `(entity_id, capability_id)` |
| `relations` | `id PK`, `type_id FK types`, `source_entity_id FK entities`, `target_entity_id FK entities`, `data JSONB` |
| `ownerships` | `entity_id PK FK entities`, `owner_entity_id FK entities`, `revision`; unique effective owner per entity |
| `locations` | `entity_id PK FK entities`, `container_entity_id FK entities NULL`, `presence_id FK presences NULL`, `revision`; exactly one location target for exclusively located entities |

**Execution, persistence, and authority**

| Table | Key fixed fields |
|---|---|
| `spatial_frames` | `id PK`, `world_id FK worlds NULL`, `parent_id FK spatial_frames NULL`, `dimensions`, `basis JSONB`, `unit_scale`, `data JSONB` |
| `presences` | `id PK`, `entity_id FK entities`, `world_instance_id FK world_instances`, `frame_id FK spatial_frames`, `active`, `transform JSONB`, `data JSONB` |
| `execution_contexts` | `id PK`, `world_instance_id FK world_instances`, `module_id FK engine_modules`, `status`, `authority_epoch`, `data JSONB` |
| `engine_bindings` | `id PK`, `definition_id FK entity_definitions`, `module_id FK engine_modules`, `native_type_key`, `data JSONB` |
| `state_documents` | `id PK`, `type_id FK types`, `entity_id FK entities NULL`, `world_instance_id FK world_instances NULL`, `data JSONB`, `revision`, `checkpoint_at` |
| `authority_leases` | `resource_key PK`, `holder_context_id FK execution_contexts`, `epoch`, `expires_at` |
| `transactions` | `id PK`, `actor_entity_id FK entities NULL`, `idempotency_key UNIQUE`, `status`, `committed_at` |
| `events` | `id PK`, `transaction_id FK transactions`, `sequence UNIQUE`, `subject_entity_id FK entities NULL`, `type_id FK types`, `payload JSONB`, `created_at` |

### 5.3 Constraint details

- `players.entity_id` ties a player persona to the shared entity identity model. Creating a persona must atomically create both records.
- `characters.entity_id` and `items.entity_id` provide *real, queryable tables* without copying or changing global identity.
- `types` is a versioned typed registry; `components` can host `minecraft.hunger`, `doom.ammunition`, or wholly new game-defined structures on the same character.
- `presences` has at most one active **authoritative** presence per portable entity; non-authoritative projections are ephemeral adapter state, not additional `presences`.
- `locations` and `ownerships` are distinct. A dropped item can still be owned, and a borrowed item can remain in another player's inventory. An item cannot simultaneously have multiple exclusive locations.
- `locations` enforces exactly one of `container_entity_id` or `presence_id` when a location row exists. Nonspatial definitions need no location row.
- `authority_leases.epoch` fences stale writers; cross-world moves and item transfers update exclusive custody atomically with an idempotency key.
- `state_documents` are scoped snapshots/checkpoints, not a mandatory SQL write each simulation tick.
- `relations` represents open-ended typed edges; exclusive invariants such as custody or authority **must not** rely solely on unconstrained edge rows.
- Immutable artifacts are content-addressed; source game assets require lawful provisioning/licensing.
- Add indexes for `(type_id)`, `(definition_id)`, per-world active presences, both relation directions, component owners/types, event sequence, leases, and commonly queried JSONB fields declared by types.

### 5.4 Standard type library (data, not hardcoded game tables)

Bootstrap definitions for transform/spatial body, geometry/collider, camera, render contribution, controller, damage/impact, inventory, equipment, build/place, animation, ownership, input, timers, environmental interactions, and generic capability invocation. These are *interfaces and schemas*, not mandatory implementations or the only possible game mechanics.

For example, the same character entity can hold `core.transform`, `core.controller`, `cave_story.health`, and `doom.weapon_selection` components without adding DB columns.

## 6. Universal API

The universal API provides typed operations over universal resources and is used by both platform services and engine adapters.

### 6.1 Stable API domains

- **Identity:** account/session validation, player persona and character selection, authorization.
- **Registry:** engines, modules, adapters, game packages, type/definition versions, assets, interface discovery.
- **Data:** read/query/create/update records and components with optimistic versioning; typed relation operations.
- **Worlds:** create/start/stop/list instances; join/leave/travel; world discovery and presence.
- **Ownership:** inventories, custody changes, drop/pickup/trade, atomic cross-server transfer.
- **Execution:** instantiate/tick/suspend/resume/destroy engine contexts, attach capabilities, invoke authorized operations.
- **Spatial:** frame transforms, region geometry, shape queries, body/proxy data, contact/impulse exchange.
- **Composition:** camera/input ownership, render-source registration, render targets/surfaces, depth and pose synchronization.
- **Events:** subscribe, publish, replicate, snapshot, acknowledge, replay durable outcomes.

### 6.2 Transport model

- Stable schema/IDL governs calls and events. Rust APIs are first-class, with language-neutral transport possible later.
- Durable state commands use request/response and transactional semantics.
- Tick/state/collision data use local high-throughput channels or shared memory when possible; network transports are used across processes/servers.
- Render surfaces use native shared GPU resources or geometry passes where available; transport is an implementation choice behind a shared interface.
- Version negotiation is explicit; incompatible major versions are rejected, not silently emulated.

## 7. Native engine composition contract

The platform standardizes **how** native execution modules communicate, not **what** their mechanics must be.

### 7.1 Required module contracts

| Contract | Requirement |
|---|---|
| `Lifecycle` | Load, initialize, instantiate, tick, suspend, snapshot, restore, destroy. |
| `Identity` | Map universal IDs to native handles and back without duplicating persistent identities. |
| `Input` | Route intent and device input to the controlling native module. |
| `Spatial` | Declare axes, dimensions, units, transforms, and reference frames. |
| `GeometryQuery` | Supply native colliders/meshes/queries to other physics implementations. |
| `Interaction` | Exchange target references, hits, forces, collision contacts, and arbitrary typed effect requests. |
| `Camera` | Expose native view/projection and camera state; select a per-client camera owner. |
| `Rendering` | Submit native geometry/passes or composed color/depth outputs with synchronization metadata. |
| `State` | Import/export portable typed state and world-presence state. |
| `Authority` | Report authoritative decisions; reject unauthorized/stale writes. |
| `Time` | Declare native tick frequency and interpolate/synchronize across differing clocks. |

Adapters may declare optional advanced interfaces, but all adapters must pass the same mandatory contract conformance suite.

### 7.2 Ownership rules

- **World owner module:** environment geometry, environmental rules, native world logic, world-owned entities, and default environment rendering.
- **Origin character module:** movement/controller, original character-specific physics and animation logic, and native camera behavior when chosen by the player.
- **Origin item/capability module:** original item behavior, effects, animation/render assets, and capability calculations.
- **Platform runtime:** authoritative identities, resource leases, input/interaction routing, common references, persistence, per-view composition, and synchronization.
- **Target/receiver module:** resolves its own native state changes after receiving an authorized interaction request, with explicit acknowledgement and outcomes.

These are **defaults, not universal assumptions**; authority must be declared per system/resource. Multiple simulators may inspect the same scene, but exactly one authority commits each exclusive outcome.

### 7.3 2D and 3D

- Dimensionality is declared by spatial frames, native simulation modules, and render contributors—not by global entity categories.
- A 2D character entering a 3D world retains its sprite/native character rendering. The common camera/spatial interfaces place it in the 3D scene; original character code remains the controller.
- A 3D character entering a 2D world retains its model and character logic, which are rendered into the world's 2D presentation through native geometry/pass or color/depth composition. The world may constrain allowed placement/travel to a plane.
- No AI asset conversion, sprite generation, or reimplementation of the original controller is required.
- A 2D physics controller does not magically acquire fully 3D locomotion. If a game needs new movement degrees of freedom, the appropriate capability/module must explicitly provide them; no fake universal conversion is implied.

### 7.4 Geometry, collisions, and effects

An adapter can export spatial geometry or provide queries. A visiting entity's original physics module consumes that world geometry and resolves its own movement. Host entities may be exposed through proxy data to the visiting module for target acquisition; outcomes return as typed interactions to their authoritative owners. Rendering may compose passes or surfaces, with synchronization of depth, camera, clipping, and timing. These are general mechanisms demonstrated in existing passthrough mods; exact engine hooks remain adapter-specific.

No custom action translates “DOOM shotgun hits Cave Story enemy.” Instead, the original shotgun module generates a platform-defined interaction with a universal target ID, and the target-owning module processes that event according to its own mechanics. There may be explicit, reusable semantic contracts for damage/impulse, but never a rule conditioned on those two game names.

### 7.5 Native execution packaging

Rust is the initial implementation language. An engine module may be an embedded library or a sandboxed local/server worker. Interface identity does not depend on process topology. Reconstructed game code that assumes global state must be isolated or refactored so simultaneous instances cannot overwrite one another.

An entire game should not need to launch per visiting character; modules must support reusable execution contexts and scoped state wherever possible.

## 8. MMO runtime and persistence

- World instances are server authoritative. Clients submit intentions/input; they cannot authoritatively mint items, finalize damage, or approve transfers.
- Servers host world modules and any visiting native modules needed by their active entities; clients host rendering/input parts as applicable.
- Persistent character identity, inventory, and ownership are shared; the current world presence is transient and can be reconstituted.
- Cross-world travel consists of quiesce/snapshot, leased authority handoff, commit of new presence/custody, resume, and reconciliation on failures.
- Runtime maintains interest management and scoped replication: nearby state and authorized durable events rather than full-database broadcasts.
- Each module can retain its own tick rate, with stamped events and controlled synchronization. No requirement to force every game into a single fixed frame rate.
- Checkpoints and event history support restart/recovery; transaction IDs and idempotency protect against duplicate item transfers.
- Multi-server, multi-instance architecture is required from the design, but automated global-scale orchestration/sharding is out of MVP scope.

## 9. Initial games and integration plan

**Candidate A — DOOM:** [`sunsided/room`](https://github.com/sunsided/room). Native Rust port of Doomgeneric's complete engine modules. Known drawbacks include single-player operation, legacy global-state assumptions, and current platform-loop coupling. GPL-2.0; audit linking/distribution implications before committing.

**Candidate B — Cave Story:** [`doukutsu-rs/doukutsu-rs`](https://github.com/doukutsu-rs/doukutsu-rs). Mature Rust reimplementation of Cave Story's engine. MIT-licensed engine; game data/assets must be sourced lawfully. Offers a contrasting 2D platformer world, controller, combat and rendering pipeline.

Both are **candidates, not guaranteed drop-in engines**. Conduct short feasibility spikes first: can each run headlessly, support multiple independent logical instances, expose native tick/state/geometry/render operations, and serialize relevant state without destroying game fidelity? A failure may justify selecting another complete Rust game; it must *not* justify pairwise compatibility shortcuts.

Reference technique: [`SkyCraft`](https://github.com/chasmlol/SkyCraft/blob/main/docs/DESIGN.md) retains Minecraft's logic in Skyrim through world geometry and actor proxy exchange, native input/camera ownership, and composited rendering. [`libsm64`](https://github.com/libsm64/libsm64) illustrates a game controller/renderer extracted as a reusable library. These inspire the contract; neither becomes a required project dependency.

## 10. Implementation scope and phases

Implementation is organized around **independent modules and platform contracts**. Cross-game interactions are explicitly **not** a bespoke implementation lane.

### Phase 0 — Contract feasibility

- Inspect and pin the two source versions, licenses, assets, and executable boundaries.
- Identify native world/controller/physics/rendering ownership, headless execution paths, and shared-state hazards.
- Define a synthetic mock engine for contract tests so the platform can be built without referencing either candidate game.
- Exit gate: credible independent integration paths for both candidates.

### Phase 1 — Universal schema and API

- Implement registry/types, identity/player/character/item tables, entity definitions, components, assets, relations, authority, persistence, and world tables.
- Ship schema migrations, validation, optimistic revisions, atomic ownership/custody moves, event journal, and tests.
- Exit gate: representative data from both games loads into the same schema without new game-specific tables.

### Phase 2 — Runtime and reusable composition interfaces

- Build Rust module lifecycle, execution contexts, native handle mapping, geometry queries, effect routing, input/camera contracts, frame composition, and synchronization.
- Build generic synthetic fixture engines with deliberately different spatial/temporal models.
- Build world authority, snapshots, server-client replication, session routing, and durable handoff infrastructure.
- Exit gate: synthetic fixture modules independently pass mandatory contracts and compose without identifying one another by name.

### Phase 3A — DOOM adapter (independent)

- Extract/host DOOM world, character, item, and rendering behavior as module interfaces, retaining original logic.
- Implement the generic platform adapter; publish types, definitions, and assets in the registry.
- Run it as a server-authoritative multiplayer world, with separate per-player contexts where required.
- Exit gate: DOOM-only world works with universal sessions, persistence, and contract suite.

### Phase 3B — Cave Story adapter (independent; parallel to 3A)

- Extract/host Cave Story systems through the same interfaces, retaining original logic.
- Publish its module/type/asset definitions and run its own server-authoritative multiplayer world.
- Exit gate: Cave Story-only world works with universal sessions, persistence, and the identical contract suite.

### Phase 4 — Neutral integration and operational testing

- Run both adapters against the platform with the same code paths and no cross-imports.
- Test independent world joining, multiple players, crash recovery, persistence, authority, rendering/timing, and general synthetic-module composition.
- Audit the code for special cases referencing both games or translating one game's objects into the other's.
- Exit gate: independent adapters pass with zero game-pair-specific code.

### Phase 5 — **Final verification only: cross-game interoperability**

Perform black-box end-to-end acceptance scenarios using unchanged platform and unchanged adapters. This is not a phase for implementing a game-specific compatibility feature. If the scenario fails, repair the *general contract or the independently nonconforming adapter*, rerun generic conformance tests, then repeat final verification.

## 11. Acceptance criteria

### Schema and platform

- [ ] Real universal `users`, `players`, `characters`, `items`, `worlds`, and corresponding supporting tables exist with enforceable keys/constraints.
- [ ] Both game definitions and arbitrary native data can be stored through the same type/component registry; no per-game migrations.
- [ ] Identity, ownership, custody, state, and definitions remain distinct and queryable.
- [ ] Multiple simultaneous connections share one persistent universe and authoritative servers.
- [ ] Crash/restart and cross-server handoff do not duplicate globally unique items or create conflicting authority.

### Independent game adapters

- [ ] Both source implementations run correctly in independently hosted worlds.
- [ ] Native logic is reused, not approximated via newly rewritten cross-game systems.
- [ ] Each adapter passes the same contract test suite with no knowledge of the other game.
- [ ] Client/server native modules can run for more than one character/instance without leaking global state.
- [ ] Compatible native render contributions, camera ownership, geometry querying, inputs, and effects work with synthetic partner modules.

### Final independent interoperability verification

- [ ] One persistent character can enter both worlds, retaining its identity and arbitrary persistent attributes.
- [ ] The character's native controller/camera/rendering operate in a foreign world using only universal contracts.
- [ ] A native item can travel across the boundary without replacing its definition or implementation.
- [ ] The original item's capability executes and generates observable authorized effects on foreign world/entities.
- [ ] The item can be dropped, acquired by another player, carried elsewhere, and restored after restart with unchanged global ID.
- [ ] At least one opposing-direction scenario runs (not merely A→B).
- [ ] No special-case code, adapter-specific reference, game-pair configuration, converted gameplay mechanic, or handwritten bridging logic was added to pass these tests.

### Nonfunctional verification

Capture frame pacing, simulation timing, inter-module latency, CPU/GPU memory, state reconciliation rates, and authoritative event correctness. Establish performance budgets from measured baselines of the first two games rather than claiming arbitrary FPS/player-count guarantees. Include disconnected clients, crashed modules, stale authority holders, malformed payloads, and conflicting transfers.

## 12. Risks and decisions

| Risk / decision | Direction |
|---|---|
| Original game globally coupled to one player/world | Extract scoped modules or isolate separate execution contexts; do not leak shared process globals. |
| Source physics and world physics disagree | Reuse native owner physics, shared geometry/spatial queries, typed interaction outcomes, and explicit authority. Preserve unsupported status for unrepresentable behavior. |
| Multiple renderers have different depth/lighting APIs | Standardize render contribution contracts; support geometry import and color/depth composition, not one compulsory technique. |
| Cross-engine authoritative combat diverges | Separate initiating native calculation and receiver-owned state application, with event IDs and declared interaction semantics. |
| Original source license restricts distribution | Conduct legal/OSS audit early; keep copyrighted assets external or properly licensed. GPL and MIT obligations differ. |
| Extensible JSONB becomes unvalidated | Versioned type schema and validation on mutations; fixed FK columns for enforceable invariants. |
| Binary/native module execution security | Trusted modules in MVP, process isolation where required, sandbox/permission boundary before accepting arbitrary user code. |
| Native camera versus host world presentation | Camera choice is per viewer; compose world and visiting outputs with declared camera matrices and render metadata. |
| Different clocks, units, or dimensions | Explicit spatial frames, timestamps and clock negotiation; never assume equal tick rates or axis conventions. |
| Sharding complexity | Multi-instance design now; distributed production orchestration later. |

## 13. Decisions fixed by this PRD

1. The platform is an MMO game platform, not an AI modder or a reverse-engineering product.
2. The novelty is universal compatibility of independently integrated games, not new standalone game mechanics.
3. Use universal **relational tables** with typed arbitrary record data, not only a generic key-value/record graph.
4. Users, players, characters, items, games, worlds, and engine modules are first-class concepts.
5. Engine logic, physics, camera, and rendering remain natively implemented and reusable; the platform mediates interactions and compositing.
6. Two Rust game rewrites provide the initial integrations; they are integrated separately.
7. **No game-pair-specific interoperability work is in implementation scope.** Cross-game behavior is a final, falsifiable verification requirement.
8. AI game ingestion, automatic adapter generation, and arbitrary commercial-game support are future work.

## 14. Delivery artifacts

A completed MVP must contain:

- Versioned database migrations and typed registry definitions.
- Universal API contracts/IDL, SDK, and reference server implementation.
- Generic runtime, composition protocol, and conformance fixtures.
- World-hosting, session, replication, persistence, and authority services.
- Two independent Rust game integrations, with their source licenses and asset instructions.
- Automated unit, contract, multiplayer, recovery, and security tests.
- A separate final end-to-end interoperability verification report, identifying precisely which behaviors were observed and any unsupported interactions.

**Definition of success:** After two unrelated games independently implement the platform interface, their characters and items coexist and interact across each other's worlds through that interface—without writing code for that particular pair.

## 15. Research references

- [SkyCraft — design and cross-engine protocol](https://github.com/chasmlol/SkyCraft/blob/main/docs/DESIGN.md): shared geometry, proxies, native control/camera, rendering, inter-process bridge. The document contains design intentions; treat unverified claims separately from functioning code.
- [libsm64](https://github.com/libsm64/libsm64): reusable original movement/render code exposed as a library; original game assets require legitimate user-provided ROM.
- [room (Rust DOOM)](https://github.com/sunsided/room): complete Rust Doomgeneric port; current multiplayer and global-state limitations.
- [doukutsu-rs](https://github.com/doukutsu-rs/doukutsu-rs): Rust Cave Story remake; engine license and separate content files.
- [universal-modder](https://github.com/rehan-remade/universal-modder): integration routes, test oracles, and engineering practices, not a runtime dependency.

---

*The PRD fixes product behavior, boundaries, data concepts, and acceptance criteria. SQL migration details, ABI layout, message serialization, and GPU/physics backends belong in implementation specifications as long as they meet this document's contracts.*