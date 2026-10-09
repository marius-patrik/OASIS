# Native game implementations

These are **independent upstream Rust game implementations**, not OASIS adapters.

| Native engine | Repository | Pinned upstream commit | Notes |
| --- | --- | --- | --- |
| DOOM | [sunsided/room](https://github.com/sunsided/room) | `904acac5fb7d89d7545082c3fab71408176d2404` | GPL-2.0; headless test utilities, native DOOM symbols; globally held game state needs isolation |
| Cave Story | [doukutsu-rs/doukutsu-rs](https://github.com/doukutsu-rs/doukutsu-rs) | `7e6870e55f072e3f056dc1eef3974a1af26c7aae` | MIT engine; player and physics are tied to shared game context and assets |

Clone with `git clone --recurse-submodules` or initialize with `git submodule update --init --recursive`.

## Contract and integration rules

- These source trees are pinned, untouched upstream repositories. DOOM/Cave Story adaptations must expose the **same** OASIS native-module and render contracts through **separate** adapter packages.
- Native game logic remains native. Do not substitute approximated gameplay mechanisms or inter-game conversions.
- Each game must run its own tests and its own headless OASIS conformance independently.
- The universal runtime must never import or name either game. Only the final integration verification uses both adapters in one process/world.
- The upstream code and game assets have separate licensing obligations. Do not redistribute copyrighted game data without appropriate rights. Linking to GPL code may affect licensing of distributed combined works.

The pinned sources now support individually runnable **partial** source-backed
OASIS native modules under [`adapters/cave-story`](../adapters/cave-story/) and
[`adapters/doom`](../adapters/doom/). The *complete original game
engines* are still not runnable as OASIS worlds. The Cave Story adapter
currently covers hitbox membership, native ammo operations and stage
PXM/attribute loading; the DOOM adapter covers fixed-point and bbox
primitives. Original controllers, world simulation, renderers and native
gameplay loops remain unfinished.
