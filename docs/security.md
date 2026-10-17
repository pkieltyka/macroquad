# Supported fork surface and soundness remediation

This fork serves Zubspace. Browser WASM with `audio` is primary; Linux native
builds without audio support the game and server tests. Native audio must still
compile. Other upstream targets and companion crates are not release-supported.

RUSTSEC-2025-0035 remains **under remediation**. The removal and buffer-validation
batch below does not repair global context, coroutine, UI, telemetry, or resource
ownership. No advisory suppression or upstream patched-version claim is made.

| Source | Finding in upstream base `8d602d3` | Fork disposition |
| --- | --- | --- |
| [#333](https://github.com/not-fl3/macroquad/issues/333) | Global mutable context aliases during touch callbacks, rendering, coroutine polling/results, and resource destruction. | Ownership work remains required. |
| [#634](https://github.com/not-fl3/macroquad/issues/634), [#746](https://github.com/not-fl3/macroquad/issues/746) | Original short-buffer example is asserted against, but public mutable dimensions/storage, 32-bit multiplication, truncated decoded dimensions, and GPU readback remain unsafe boundaries. | Image fields are externally private. Constructors validate complete layouts; pixel slices use safe chunk views. Dimensions, upload buffers, partial rectangles, screenshot dimensions, and RGBA8 readback are checked before backend calls. |
| [#723](https://github.com/not-fl3/macroquad/issues/723) | Lens invokes arbitrary callbacks on uninitialized references; scene has unchecked typed conversions, global mutation and lifetime invalidation. | Scene, lenses, tweens, and their state-machine APIs and frame hooks are removed. |
| Experimental global storage | Replacing the sole stored owner can invalidate an outstanding borrow; global map mutation is unsynchronized. | Removed. Companion profiler now owns TLS state. |
| PR [#956](https://github.com/not-fl3/macroquad/pull/956), uniforms | Safe representation conversion and unconstrained scalar uniform transmutation permit uninitialized bytes. Metadata sizing may overflow. | Tracked by the separate representation/uniform patch; independent of context ownership. |
| PR [#973](https://github.com/not-fl3/macroquad/pull/973) | Audio isolates one component but leaves scheduler aliases, destruction and pending-load lifetimes. | Audio and coroutine ownership work remains required. |
| Telemetry, test initialization | Mutable statics, raw zone pointers, global mutex initialization and resetting context with reentrant destructors. | Ownership work remains required. |

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
- `telemetry::scene_allocated_memory` and raw drawcall texture IDs are removed.
  The companion profiler retains frame/coroutine metrics without raw thumbnails.

The upstream particle crate and examples requiring raw renderer access are not
part of this fork's supported build. Their Macroquad development dependency and
raw-renderer/particle/tree examples are removed. Custom material, UI, text,
tiled and platformer examples remain build targets.

Zubspace's test-only LVZ image constructor needs the `Image::from_rgba8`
migration; its renderer uses `Texture2D::from_rgba8`. Its direct Miniquad calls
are `date::now` and remain available. Its WASM voice and LVZ paths require the
retained coroutine/audio API; subsequent ownership changes must preserve them.

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
