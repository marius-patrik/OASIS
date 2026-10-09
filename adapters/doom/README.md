# DOOM original-engine math adapter (partial)

This adapter directly depends on the pinned GPL-2.0 upstream
[room](https://github.com/sunsided/room) Rust engine, running the original
source functions:

- `m_fixed::FixedMul` and `FixedDiv` for DOOM 16.16 arithmetic, saturation,
  and original overflow behavior.
- `p_maputl::P_AproxDistance` for the original map/thing distance estimator.
- `m_bbox::M_ClearBox` and `M_AddToBox` for native bounding-box logic.

The origin module registers these capabilities through standard OASIS
`GameAdapter` and executes them through `NativeModule`. Tests validate
the original numeric and boundary semantics plus independent contexts and
snapshot persistence. The upstream source is unchanged.

**Not a DOOM game integration:** native simulation ticks, controller commands,
weapons, collision traversal, renderer, multiplayer and IWAD loading are NOT
integrated. Those have globally mutable state in the existing port and require
dedicated process isolation or refactoring without replacing game logic.

**Licensing:** This adapter links GPL-2.0 source code and is distributed as a
GPL-scoped adapter, independently of OASIS's core crates. Distributing a
combined executable may subject the whole combined work to GPL obligations.
Source assets / IWADs are not bundled.

Test:
```sh
git submodule update --init --recursive
cargo test --manifest-path adapters/doom/Cargo.toml --lib
```
