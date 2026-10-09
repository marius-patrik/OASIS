# Native game process boundary

The OASIS-native worker is an **engine-independent final execution boundary**:
an original game process owns its source renderer, scene, camera, mutable globals,
physics, controller and other runtime state. OASIS owns the child process and a
reliable typed transport, not the game's simulation implementation.

- `ProcessModule::spawn(command, module_id, context_id)` creates one separate
  native OS process per execution context and validates its versioned handshake.
- `serve_worker(local_module)` runs inside that game-specific executable.
  `LocalNativeModule` has no `Send` requirement, allowing native engines with
  thread-affine scenes and render resources.
- All `NativeModule` calls (instantiate, step, snapshot, restore, remove)
  cross the length-prefixed protocol. One step can make synchronous calls to
  *all five* universal `WorldPort` operations; the host services them through
  its normal authoritative world context. The game is never given database
  credentials or asked to implement another engine's behavior.
- Original bytes, unsigned values and arbitrary namespaced values are preserved.
  Globally unique `Id` and high precision clock counters serialize as decimal
  strings to avoid JSON floating-point truncation; nonfinite native floats are
  rejected, not silently converted to `null`.
- A 16 MiB limit bounds individual control frames. Large assets and video
  surfaces require separate referenced/shared-memory transport; this is the
  **control and native snapshot channel**, not a pixel-stream renderer.

## Verify

```sh
cargo test -p oasis-native-process --test process --locked
```

The test starts a **real executable child** and verifies handshake rejection,
original entity identity, snapshot/restore, arbitrary binary component
roundtrips, and all five WorldPort callbacks including typed interaction
submission. Tests do not implement game-pair compatibility.

## Remaining work

This establishes process isolation, **not a security sandbox**: restricting
syscalls/filesystem/network/CPU, signed native module distribution, worker
timeouts, IPC reconnection, lossless large binary-state transport, source-native
render surface handoff and native-side checkpoint/restart are pending.
Durable leases and commit fencing must stop a stale child from producing
authoritative side effects after recovery.

To run the complete upstream DOOM and Cave Story simulations, each source
implementation must also expose its **original actual game tick and native
scene lifecycle** inside a game-specific worker. The pinned Cave Story public
API currently hides `GameEntity::tick` and its framework context; an
upstream bridge/fork is required, not a locally copied player physics model.
