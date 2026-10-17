# Supported fork surface and soundness remediation

This fork serves Zubspace. Browser WASM with `audio` is primary; Linux native
builds without audio support the game and server tests. Native audio must still
compile. Other upstream targets and companion crates are not release-supported.

RUSTSEC-2025-0035 remains **under remediation**. Removal/buffer validation and
coroutine/audio lifecycle changes do not complete graphics context, UI,
telemetry, or resource ownership. No advisory suppression or upstream
patched-version claim is made.

| Source | Finding in upstream base `8d602d3` | Fork disposition |
| --- | --- | --- |
| [#333](https://github.com/not-fl3/macroquad/issues/333) | Global mutable context aliases during touch callbacks, rendering, coroutine polling/results, and resource destruction. | Graphics context, callbacks and resource ownership work remains required; coroutine/audio stores are independent. |
| [#634](https://github.com/not-fl3/macroquad/issues/634), [#746](https://github.com/not-fl3/macroquad/issues/746) | Original short-buffer example is asserted against, but public mutable dimensions/storage, 32-bit multiplication, truncated decoded dimensions, and GPU readback remain unsafe boundaries. | Image fields are externally private. Constructors validate complete layouts; pixel slices use safe chunk views. Dimensions, upload buffers, partial rectangles, screenshot dimensions, and RGBA8 readback are checked before backend calls. |
| [#723](https://github.com/not-fl3/macroquad/issues/723) | Lens invokes arbitrary callbacks on uninitialized references; scene has unchecked typed conversions, global mutation and lifetime invalidation. | Scene, lenses, tweens, and their state-machine APIs and frame hooks are removed. |
| Experimental global storage | Replacing the sole stored owner can invalidate an outstanding borrow; global map mutation is unsynchronized. | Removed. Companion profiler now owns TLS state. |
| PR [#956](https://github.com/not-fl3/macroquad/pull/956), uniforms | Safe representation conversion and unconstrained scalar uniform transmutation permit uninitialized bytes. Metadata sizing may overflow. | Tracked by the separate representation/uniform patch; independent of context ownership. |
| PR [#973](https://github.com/not-fl3/macroquad/pull/973) | Audio isolates one component but leaves scheduler aliases, destruction and pending-load lifetimes. | Adapted independent audio ownership with deferred deletion and terminal WASM load states; scheduler polling/results/destruction now run outside its borrow. Graphics ownership remains separate work. |
| Telemetry, test initialization | Mutable statics, raw zone pointers, global mutex initialization and resetting context with reentrant destructors. | Native attributed tests now use subprocess isolation without mutable mutexes or Miniquad resets; graphics/telemetry ownership work remains required. |

## Miniquad boundary

The dependency remains Miniquad 0.4.11. We expose an allowlisted `miniquad` facade
with configuration, input/render metadata value types, and `date::now`. The raw
backend crate, constructors, backend references, pointer/buffer sources and GL
bindings are private. Compile-fail documentation tests guard the public boundary.

Relevant inspected dependency boundaries are `graphics/gl.rs`'s texture readback
(no output-length check), resize (no source-length check), partial upload
(unchecked size/rectangle arithmetic), and uniform upload (a safe raw pointer API
with incorrect layout bounds). Macroquad avoids exposing these as safe APIs.
Its internal RGBA transfers validate byte counts before calling Miniquad; the
shared layout check also enforces Miniquad's u32 format-size limit and Rust's
isize slice limit. Readback supports RGBA8 and one sample only. New transfer
paths must use equivalent checks. Raw renderer mutation is outside the supported
API and cannot change pixel formats or GL packing state behind those checks.

`Image` dimensions remain u16. Decoded dimensions outside that range return an
error, and render targets outside that range are rejected before allocation.
Empty CPU images are valid. Buffer-length and invalid-rectangle errors panic in
existing infallible texture APIs and return errors in image constructors.

## Coroutine and audio ownership

The coroutine scheduler has independent thread-local ownership. Polling takes a
future out of the store and leaves a cancellable placeholder; no scheduler
borrow crosses `Future::poll` or future/result destruction. Tasks created during
an update wait until the next one. Cancellation of an active poll takes effect
when it returns. Recursive polling of that same future is rejected before any
future access. Results can be retrieved once; completed/cancelled unit tasks
need no retained allocation. IDs are never reset/reused, including after
`stop_all_coroutines`.

Manual time deltas are finite and nonnegative. Nested polls restore their
parent's timer delta on return and unwind. Async locals may be destroyed while
their own poll is unwinding, with that task's timeline still active; the store
remains unborrowed throughout. Invalid/backwards automatic frame times pause
timers. At thread shutdown the store closes before destroying user captures;
queries/cancellation remain safe and newly started tasks are immediately
cancelled without accessing a destroyed TLS key.

Audio retains the originating owner in every `Sound`, independently of the
graphics context and scheduler. Its source guard is installed before awaiting
readiness. Final-handle drop enqueues deletion and collects when the owner's
context is free; a drop during an audio borrow never recursively borrows it.
Sounds keep their owner alive even after audio TLS has been destroyed. Audio
initialization is single-use per thread, with no public context replacement.
Native/no-audio keeps the dummy backend; native audio retains quad-snd's decoder
behavior.

The audio isolation adapts [#973](https://github.com/not-fl3/macroquad/pull/973),
head `0e97981f8418c08f04f316e2b7e0c7dbe8c7ef42` by kampffrosch94
`<kampffrosch94@github.com>`. It is not imported as an advisory-wide fix.

WASM/audio requires the paired runtime's `audio_source_state` extension v1:
`0` pending, `1` ready, `-1` decode failure and `-2` removed/unknown. The existing
`macroquad_audio` plugin ABI remains version 1. A small WASM-only source adapter
owns the ID directly; it does not read quad-snd private fields. Failed or
cancelled loads release their source. The paired JS runtime must copy encoded
input synchronously, prevent deleted sources from being resurrected by late
callbacks, and handle both callback and Promise rejection from `decodeAudioData`.
Zubspace's runtime provenance/import gate must require this extension when
pinning the fork. Older JavaScript bundles are not compatible with this WASM.

Native `#[macroquad::test]` cases run in fresh exact-selected harness processes.
The parent verifies that the requested child test actually entered, so a missing
filter cannot silently pass. This supports grouped/parallel test runs without
resetting Miniquad's process-wide display. WASM uses finite browser fixtures;
the native process harness is intentionally unavailable there.

## Intentional API changes

- Replace `Image { bytes, width, height }` with
  `Image::from_rgba8(width, height, bytes)?`. Read `width()`, `height()`, `bytes()`
  and mutate pixels through `bytes_mut()` or `get_image_data_mut()`.
- `sub_image` accepts finite nonnegative integer rectangles inside the image.
  It rejects fractional, nonfinite, negative and out-of-bounds rectangles.
- `experimental::{scene, collections, state_machine}` and
  `experimental::coroutines::tweens` are removed. Own application state directly;
  the retained coroutine API does not depend on scene nodes.
- `window::{get_internal_gl, InternalGlContext}`, public `QuadGl`/`GlPipeline`,
  raw texture import/export and raw render-pass export are removed. Use
  `rendering_backend()` for shader-source selection, `load_material` for custom
  shaders, and normal texture/render-target drawing APIs.
- `Sound` and `Coroutine` handles belong to their originating thread and are
  not `Send`/`Sync`. `start_coroutine` accepts thread-local futures without a
  `Send` bound. Copying/dropping a coroutine handle leaves its task running:
  drain retired queues with explicit `stop_coroutine` calls, including completed
  results. `audio::AudioContext` is internal rather than a replaceable public
  engine context.
- `telemetry::scene_allocated_memory` and raw drawcall texture IDs are removed.
  The companion profiler retains frame/coroutine metrics without raw thumbnails.

The upstream particle crate and examples requiring raw renderer access are not
part of this fork's supported build. Their Macroquad development dependency and
raw-renderer/particle/tree examples are removed. Custom material, UI, text,
tiled and platformer examples remain build targets.

Zubspace's test-only LVZ image constructor needs the `Image::from_rgba8`
migration; its renderer uses `Texture2D::from_rgba8`. Its direct Miniquad calls
are `date::now` and remain available. Its WASM voice and LVZ paths require the
retained coroutine/audio API. Its dynamic sound queue replacements must drain
and cancel old Copy handles instead of only clearing their vectors.

## Regression scope

CPU regressions cover exact image construction, pixel/byte mutation, invalid
sub-images, decoded dimension truncation, malformed uploads rejected before
backend access, and checked layout/partial-transfer arithmetic without allocating
large buffers. Compile-fail tests protect removed imports and private fields.
These CPU paths are suitable for Miri. GPU and audio FFI require separate native
and browser validation; passing these tests alone does not close the advisory.

The fork CI runs native examples/checks with and without audio, WASM checks with
audio, CPU and compile-fail tests, and the finite native RGBA/UI lifecycle GPU
regressions. Miri is pinned to nightly-2026-10-16 for x86_64 and i686, covering
32-bit slice/metadata overflow as well as representation and buffer mutation.
The PNG codec integration test runs natively and is skipped only by Miri; its
dimension-check helper still runs under both interpreter targets.

Lifecycle CPU/Miri entry points are `experimental::coroutines::tests` and
`audio::tests` (the instrumented no-audio backend exercises the actual owner,
load future and deletion helpers). They cover nested creation/polling, explicit
cancellation and detached handles, result/future Drop reentry, stale IDs, manual
clocks/unwind, thread shutdown and pending/failed/completed audio ownership.
`examples/audio_lifecycle_regression.rs` plus
`tests/audio_lifecycle_regression.html` exercise the paired WASM imports through
real decoding, failure, pending cancellation, last-clone drop and cancellation
of an unclaimed completed sound. The browser fixture checks each allocated
source is deleted once and remains deleted after late callbacks. Run it with
`tests/ui_material_browser.py --fixture tests/audio_lifecycle_regression.html
--marker "audio lifecycle regression: passed" --runtime /path/to/paired/bundle.js
--output target/audio-lifecycle-browser --unlock-audio --timeout 90` after building the example
for WASM with audio (quad-snd requires the existing WASM `--allow-undefined`
linker setting). The same runner preserves the existing UI regression by default.
