# Cave Story native hitbox adapter (partial)

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
refill), even when instantiated in a different OASIS host. Projectile
simulation, firing cooldown, sound effects, player inventories, and weapon XP
are **not** implemented by this initial source-backed item capability.

This **does not** constitute a full Cave Story adapter: its player controller,
tile/slope collision response, NPC interactions, weapons, camera, original
rendering pipeline, and playable worlds are not yet wired into OASIS. In
particular, point-in-hitbox membership is only a small part of its physics.

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

This **does not** expose the full upstream player tick or scene renderer.
Those upstream modules are private and require a source-native bridge, not
a replica Cave Story physics/controller model in OASIS.

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
Native scene NPCs, projectiles, cameras/renderers, original world script
lifecycle and complete opaque player-state serialization remain open. The existing worker protocol can host this non-`Send`
engine context once its lifecycle and complete native snapshot rules
are integrated.
