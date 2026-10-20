# Cave Story native source adapter — partial world integration

**Scope:** OASIS calls the actual upstream `doukutsu-rs` implementation of
`HitExtents::point_in_entity_x` and `point_in_entity_y` in
`engines/cave-story/src/game/physics.rs`. No formulas are copied into OASIS.

The independently registered native module accepts arbitrary source-typed
character state, preserves unknown components through native snapshots,
executes the original hitbox membership algorithm on signed native fixed-point
coordinates, and emits source-typed results. Distinct execution contexts have
independent instances. No copyrighted game assets are required to run tests.

The independent [native weapon module](src/weapon.rs) calls the original
Cave Story `Weapon::consume_ammo` and `Weapon::refill_ammo` methods directly.
It preserves source item identity, snapshot revisions, unknown native metadata,
and original ammunition semantics (including unlimited ammunition and maximum
refill), even when instantiated in a different OASIS host. This isolated ammunition module is distinct from the source-native
player worker below, which *does* execute original inventory/weapon firing,
projectile spawning/movement and track ammunition/XP. Complete source-world
audio and **lossless** cooldown/bullet state persistence remain unimplemented.

This **does not** constitute a full Cave Story game adapter. The original
player/controller, PXM tile collision, inventory/weapon and bullet ticks run in
the separate worker described below, while NPCs, scripts, all complete scene
state, native camera/rendering and multiplayer playable worlds remain missing.
This isolated hitbox capability is only one part of the native simulation.

The nested Cargo workspace intentionally compiles the pinned upstream crate
as a dependency; root OASIS CI is independent of the upstream game's large
dependency graph.

```sh
git submodule update --init --recursive
cargo test --manifest-path adapters/cave-story/Cargo.toml --lib
```

## Source-native stage maps

The [stage module](src/stage.rs) uses the pinned original
`doukutsu_rs::game::map::Map::load_pxm` PXM reader and
`Map::get_attribute` tile-attribute resolution. It accepts original PXM and
256-byte source-native attribute data, retains arbitrary additional native
state, and runs independently through the universal OASIS module registry,
`Host`, and snapshot/restore lifecycle. Asset-free tests generate valid PXM
source-format fixtures and exercise the original loader in CI.

It does **not** infer physics from arbitrary tile codes, replace Cave Story's
slope/tile collision response, provide a renderer or full stage lifecycle, or
make the game playable. Those require integrating the game's actual scene,
player, physics, NPC, and rendering contexts.

## Native OS process mode

The same original Cave Story hitbox, ammunition and PXM-stage modules now
have a [standalone native worker](src/bin/oasis-worker.rs), launched by the
generic `ProcessModule`. In the child, each module invokes the exact same
pinned `doukutsu-rs` functions; OASIS only forwards commands, native
snapshots and authoritative WorldPort calls. The [process tests](tests/process.rs)
prove all three modules execute in separate OS processes, and that a weapon
retains its globally stable ID, ammunition, arbitrary native metadata and
source-specific refill semantics after restarting into another native worker.

```sh
cargo test --manifest-path adapters/cave-story/Cargo.toml --test process
```

This standalone hitbox/ammunition/stage worker does not itself expose
the player/scene renderer. The separate source-native player worker below
**does** invoke original `Player::tick` through a reviewed source patch;
a complete `GameScene` and native renderer are still absent.

## Original player tick bridge (source patch)

The [reviewable source patch](source-bridge.patch) exposes a dedicated
`doukutsu_rs::oasis_bridge::Simulation` API from the **pinned original
game source**. Its `tick()` directly invokes upstream
`GameEntity::tick(&mut Player, &mut SharedGameState, &NPCList)`; all
acceleration, drag, movement, animation and player rules remain in the
original engine. The bridge owns the game's native headless
`Context`, `SharedGameState`, built-in filesystem, player, NPC list,
and a source-native `PlayerController` implementation driven by the
provided directional/jump/shoot buttons.

The pinned submodule is verified by commit hash *before* applying this
patch at native CI build time. The patch is tracked in OASIS; it does
not introduce a fake movement solver or fork unrelated game behavior.

Reproduce the source-native player tests locally:

```sh
git submodule update --init --recursive
git -C engines/cave-story apply --check ../../adapters/cave-story/source-bridge.patch
git -C engines/cave-story apply ../../adapters/cave-story/source-bridge.patch
cargo test --manifest-path adapters/cave-story/Cargo.toml --lib player
```

The same original source bridge now loads authentic PXM maps through
`Map::load_pxm` and advances the player's original
`PhysicalEntity::tick_map_collisions` immediately after the original
`Player::tick`, as `GameScene::tick_world` does. The CI fixture constructs
a native-format solid ground and tests that the actual source game's
collision flags and player displacement resolve contact. No tile collision
formulas are copied into OASIS.

This is **headless original player movement and tile physics**, not a complete
playable OASIS world or checkpoint-compatible original game scene.
Native scene NPCs/scripts, complete original renderer/camera and fully
opaque source-owned serialization remain open. Native projectiles **do**
currently tick in the player worker, but their in-flight state is not yet
checkpointed. The source worker already runs in a non-`Send` isolated child,
not merely a planned process boundary.

## Original player simulation in a game worker (movement-only checkpoint)

The [original player module](src/gameplay.rs) hosts the pinned
`doukutsu_rs::oasis_bridge::Simulation` inside its own **non-Send** native
game worker. The universal [process-boundary ABI](../../native-process/)
exposes it as an ordinary `GameAdapter` / `NativeModule` to OASIS. It
loads a source-native PXM stage, applies typed controller input, calls the
actual upstream `Player::tick` and `PhysicalEntity::tick_map_collisions`,
and emits persistent native movement state on each authoritative tick.

The [process integration tests](tests/player_process.rs) verify this through
the actual OASIS catalog and `Host`, including a real child process,
movement and collision, exact persistence of unknown attached game metadata,
restart into another source game process, and fail-closed invalid input.

**Important limit:** the current `player.movement-state` checkpoint stores
only the source player's fixed-point position, velocity, life, collision
flags and native tick, plus the unmodified PXM stage attachment and arbitrary
source components. It does **not** preserve the full original Player object,
NPC state, animation/booster timers, source RNG, inventory, scripts or native
render scene. Therefore it cannot yet be called full lossless native
portability. Completing the source-owned full checkpoint remains mandatory.

Run after applying the pinned upstream patch:

```sh
cargo test --manifest-path adapters/cave-story/Cargo.toml --test player_process
```

## Original weapons and projectiles in the player worker

The pinned Cave Story source bridge also executes original
`Inventory::tick_weapons`, `Weapon::tick`, `BulletManager::tick_bullets`
and `BulletManager::tick_map_collisions` in upstream scene update order.
Weapons are selected through their original `WeaponType` and
`WeaponLevel` enums, not translated into platform combat rules.
The `player.native-weapon` component contains original weapon type, level,
experience, ammunition, and maximum ammunition. OASIS snapshots reflect
the original game inventory after each tick and emit the count of live
original-engine projectiles. Native CI exercises actual Polar Star projectile
spawning, native ammunition consumption and recovery of weapon inventory
state in another game-worker process.

**Important:** Active projectile state and internal weapon cooldown timers
are *not* yet serialized. Restarting the player preserves source weapon ammo
but does not restore in-flight projectiles or exact firing timing; this
remains a partial, not fully lossless, native game checkpoint.

## G0 full-game interface

The [G0 contract-major 1 source-neutral role specification](../../contracts/G0_SPEC.md)
is the implementation target for this source adapter. Existing partial
`NativeModule` functions have not passed whole-game conformance. Adopt the
same [2D and 3D neutral vectors](../../contracts/src/neutral.rs) as other
engines, implement complete `GameScene` native lifecycle and render pixels,
then prove exact fresh-worker resimulation including bullets, timers, scripts
and native camera. Review the [pinned-source feasibility / asset audit](../../contracts/G0_FEASIBILITY_AND_LICENSES.md).
