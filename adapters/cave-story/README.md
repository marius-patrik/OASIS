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
