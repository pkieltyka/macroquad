# Supported fork surface and soundness remediation

This fork prioritizes HTML5 games, including Zubspace, using browser WASM,
WebGL1/2 and `audio`. Chrome/Firefox are the primary browser targets across
modern OS environments with the required browser capabilities, including
Windows, macOS and Linux. Actual browser fixtures and historical application
acceptance ran on Linux hosts; this does not establish Safari, mobile or all
OS/device/driver combinations as validated. See the [support scope](fork.md).

Native Linux is a secondary development/test path. Builds without audio support
the game/server tests; native audio must still compile. The native window
backend is X11 with OpenGL (GLX/EGL). Window initialization rejects Wayland and
other native OS targets before entering Miniquad; these native guards do not
restrict the OS hosting a browser WASM build. Companion crates remain build
targets but are not independently release-supported.

Browser context selection defaults to Auto (WebGL2 first, WebGL1 only on
context-creation failure), with strict explicit1/2 requests. The current Rust
requires `webgl_selection_version` extension v1 before context/GPU setup;
legacy Miniquad ABI2 and audio ABI1 remain unchanged. Missing/unsupported
selection contracts and unavailable contexts fail rather than fabricating
capabilities. The owned GL version and MSAA/format capabilities come from the
actual context, including fallback1. See [selection and pairing](fork.md#webgl-selection).

The tested source revision is
[`ea02ddd91439fb1c5024f0dc062c6a04b1f3d271`](https://github.com/pkieltyka/macroquad/tree/ea02ddd91439fb1c5024f0dc062c6a04b1f3d271),
published on `zubspace-reviewed-ea02ddd`. Its scoped ownership and buffer
remediation passed independent and accumulated review, including the LP64 XDND
writer correction in `4f9022c`. All four jobs in
[the fork CI run](https://github.com/pkieltyka/macroquad/actions/runs/38994560847)
passed: supported targets, Rust 1.88, and Miri on x86_64/i686. Pinned Zubspace acceptance is recorded separately below and in the ledger. This is not advisory suppression or a
patched-version claim for published upstream releases.

| Source | Finding in upstream base `8d602d3` | Fork disposition |
| --- | --- | --- |
| [#333](https://github.com/not-fl3/macroquad/issues/333) | Global mutable context aliases during touch callbacks, rendering, coroutine polling/results, and resource destruction. | Scoped TLS components replace global references. Camera/input/uniform callbacks and future destruction run outside context scopes; owner-associated GPU cleanup follows draw leases. |
| [#634](https://github.com/not-fl3/macroquad/issues/634), [#746](https://github.com/not-fl3/macroquad/issues/746) | Original short-buffer example is asserted against, but public mutable dimensions/storage, 32-bit multiplication, truncated decoded dimensions, and GPU readback remain unsafe boundaries. | Image fields are externally private. Constructors validate complete layouts; pixel slices use safe chunk views. Dimensions, upload buffers, partial rectangles, screenshot dimensions, and RGBA8 readback are checked before backend calls. |
| [#723](https://github.com/not-fl3/macroquad/issues/723) | Lens invokes arbitrary callbacks on uninitialized references; scene has unchecked typed conversions, global mutation and lifetime invalidation. | Scene, lenses, tweens, and their state-machine APIs and frame hooks are removed. |
| Experimental global storage | Replacing the sole stored owner can invalidate an outstanding borrow; global map mutation is unsynchronized. | Removed. Companion profiler now owns TLS state. |
| PR [#956](https://github.com/not-fl3/macroquad/pull/956), uniforms | Safe representation conversion and unconstrained scalar uniform transmutation permit uninitialized bytes. Metadata sizing may overflow. | Unsafe representation contract, checked layouts and aligned initialized uniform storage; custom conversion executes before the graphics scope. |
| PR [#973](https://github.com/not-fl3/macroquad/pull/973) | Audio isolates one component but leaves scheduler aliases, destruction and pending-load lifetimes. | Adapted independent audio ownership with deferred deletion and terminal WASM load states; scheduler polling/results/destruction now run outside its borrow. Graphics and audio ownership are independent. |
| Telemetry, test initialization | Mutable statics, raw zone pointers, global mutex initialization and resetting context with reentrant destructors. | Independent TLS profiler with owned zone trees/token guards; native attributed tests use subprocess isolation without mutable mutexes or Miniquad resets. |

## Miniquad boundary

The dependency remains Miniquad 0.4.11. We expose an allowlisted `miniquad` facade
with configuration, input/render metadata value types, the input event-handler
trait, and `date::now`. The raw
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

## Scoped graphics, UI and resource ownership

`Context`, the rendering backend and UI have separate thread-local owners. The
private `Scoped<T>` helper takes a component out of its cell for the duration of
a closure and restores it on return or unwind. No reference/borrow guard escapes,
and the cell is never borrowed across application code or destruction.
Recursive access to the same active component is rejected; independent components
remain available. Camera trait methods, uniform `ToBytes` methods and input
subscriber callbacks execute before entering the relevant graphics scope.
Input replay takes an owned event batch before invoking subscribers, so their
callbacks can grow the subscriber list, draw, and start/cancel coroutines.

`root_ui(|ui| ...)` grants exclusive UI access within a closure. Its callback can
use graphics/input APIs. UI input processing and draw-list construction run
outside the context/backend scopes; material creation and atlas upload precede
rendering. UI references cannot be returned from the closure. Default clipboard
operations check the window thread before entering the dependency. Arbitrary
values stored in the UI are destroyed outside the context/backend scopes.

GPU guards retain their originating owner and only enqueue primitive deletion
requests on final drop. Textures have non-reused batching keys; weak handles
resolve to the owner's white texture after expiration rather than addressing a
reused slot. Cross-owner/closed-owner access is rejected. Draw calls and camera
state retain texture/pass leases; material selection and queued draws retain
materials. Flush, clear and reset release queued leases without keeping old
arena resources in reusable draw-call capacity. Custom material textures retain
their own guards. Render-pass guards retain every color, resolve and depth
attachment, independently of externally mutable `RenderTarget` fields.

The vendored backend's render-pass deletion releases only FBOs. Attachments are
released by their explicit owners; a texture clone remains valid after its
render target/pass is dropped. This matches the dependency's Metal ownership
convention, although Metal is not a supported native backend of this fork.
Snapshot caches release replaced textures/FBOs and use a resolved color texture
when sampling an MSAA target. Unsupported multisampling is rejected from backend
capabilities before allocation; WebGL1 does not silently enter MSAA calls.

An atomic process-wide entry gate permits one window initialization attempt,
including failed attempts. Other threads cannot initialize a second backend or
reach this thread's context. On normal completion/accepted close, shutdown runs
inside a live backend callback: user futures/captures are dropped outside scopes,
the scheduler closes against destructor-created tasks, UI/profiler owners are
released, renderer resources are destroyed and deferred GPU cleanup is drained.
Audio TLS releases its owner; outstanding sounds retain theirs. Closed handlers
ignore subsequent events. Miniquad X11 drops its handler after destroying its GL
context, so `Stage::drop` is a CPU-only fallback rather than issuing late GL calls.
Late resource drops cannot call a closed backend. Context resets are not exposed.
Resources still retained after logical shutdown are reclaimed with the underlying
OS/browser graphics context; late guards do not invoke foreign GPU deletion.
Graphics context loss/restoration is not implemented: restoring a browser context
does not rebuild existing GPU resources. Reload the application after context loss.

## Telemetry ownership

CPU zones use an owned tree and index/token stack, not parent pointers into a
relocating vector. `Frame`/`Zone` snapshots own their data and can cross threads.
`ZoneGuard` belongs to its thread/token: dropping an ancestor closes descendants,
late descendant drops are harmless, and a frame boundary invalidates old guards.
Zone clocks are context-independent; unwinding does not retain a component borrow.
Capture snapshots retain owner-associated texture/FBO guards until the final
clone drops, including across recapture. Capture pause/resume calls nest.

Miniquad 0.4.11 does not implement valid GPU elapsed-query availability/results.
This fork performs no raw timer-query or timing-only GL synchronization calls.
`gpu_timing_supported()` is false and `gpu_queries()` returns no samples;
unavailable timing is not reported as a measured zero. Begin/end query labels
remain available separately through `gpu_query_labels()`.

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
timers. At window/thread shutdown the store closes before destroying user captures;
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
The included fork runtime implements this extension, and its import gate checks
the pairing. Downstream application provenance/import gates must require the
extension when pinning the fork. Older bundles are not compatible with this WASM.

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
- Replace `root_ui().label(...)` with `root_ui(|ui| ui.label(...))`.
  Wrap widget builders as `root_ui(|ui| builder.ui(ui, |ui| { ... }))`.
  `Ui::new`, the unimplemented hidden `camera_ui`, and unsafe global atlas reset
  are outside the supported API. Use the root UI and owned fonts instead.
- GPU textures, render targets, materials, fonts and capture snapshots are
  thread-affine. Move CPU images, mesh data or owned CPU telemetry snapshots
  between threads instead. Graphics operations require the active owner.
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

Zubspace integration `725dd5d` migrates the LVZ image constructor/accessors and
explicitly cancels retired arena/LVZ sound tasks rather than merely dropping
Copy handles. Its renderer uses `Texture2D::from_rgba8`, and its direct Miniquad
clock calls remain available. Actual game LVZ/bot audio and library sound
play/stop/cancel/failure evidence must be distinguished from legacy received
voice: that module has no current game UI upload caller/server relay handler,
so native protocol checks do not establish human voice end-to-end acceptance.

Application source64c8f58 fixed the independently reproduced respawn handoff,
passed its actual strict renewal and retained17 successful continuation
invocations. A later genuine maxed Ship admission failure (Javelin1200 versus
required2000) required production repair
`fb3fbd3928a20416fdde1234a8a327df9e483381`, now committed after three fresh
reviews and matching source tests. Separately negotiated Ship support, observer
capability discovery and explicit Ship freezing preserve spectator return and
initial playable joins; this is not an admission/policy relaxation.

Actual fb3 objective retries and all targeted Ship renewals passed, including
strict life2 energy2000 admission without RTC rejoin. Current source tuple is
compiled client `c975ae2634573605725107ebfcffc0785d632f3a`, fixture delivery
`6167ac530c2d5dfa963ad12cb5ecb1e9190ad049`, with byte-unchanged native server
`ec9090fdc50734b6edaef930b12d70a684fcaab1`. The new f2f39d8 client/retained
551c68 server image pair passed actual isolated Compose, independent verification
and complete owned0→1→0 cleanup. This is bounded RTC/package evidence.

[The patch ledger](patch-ledger.md#intermediate-application-acceptance-checkpoints)
records separate source/image tuples and original REDs. The c975 forced-reorder
retry passed all12 strict predicates on first poll with a21-file/three-QA audit.
Chrome's hostile two-bot run now supplies actual observed child-explosion proof:
child97/type15 dealt400 damage, energy293→0, to victim768/life2, bound to the
observed parent91 commit5881 and actual uploaded/GET-verified QA attachment.
The generic verifier covers14 files/one QA; `qaTraceChildAudit` remains null.
The earlier unexercised-child limitation is resolved by new live observation,
not source-only recapture, offline lineage or promotion of primary-only history.

Firefox two-bot renewal passed full13-file/one-QA verification, primary-only.
A separate genuine-input QA upload/GET primitive-JSON probe passed14-file/one-QA
verification and exact whole-JSON/raw-byte equality; it makes no child claim.
Earlier nonlethal/exact-upload identity REDs remain preserved. Reviewed fixture
corrections preserve exact checks and all ten compiled artifacts. The original DEBUG high-refresh aggregate remains RED; its
single supported release Firefox Extreme renewal passed full independent
five-file/two-QA verification. Private reviewed user integration2008 now passed
all four bounded browser cells, including one affected mixed renewal with two
strict wire/gameplay cycles and complete RTC drain. Its initial external-adapter
cleanup RED remains archived. The fixed09:49 snapshot does not cover later user
commits. Reviewed fixture-only private branchf1afd7/treecd57 adds no production
delta; accepted application artifacts/browser cells remain compiled2008.
All five remaining focused delivery stages passed full verification, with
headed Firefox20 explicitly renewing the failed headless-GL startup case. Eight
cadence/two startup cells passed independent verification with immutable inputs;
Chrome Extreme measured14.50% lower cadence, a material concern with dynamic
workload equivalence not established (early accepted versus late rejected
respawn). Measurement/reporting is complete, without isolated library attribution.
Final documentary review is a separate handoff step; measurement success is not
a performance-parity or security-wide proof. Macroquad's published source and CI remain at ea02ddd; no
official advisory-wide closure or universal-platform claim follows here.

## Remaining unsafe code and foreign contracts

Every remaining `unsafe` site under `src/` is confined to these boundaries:

| Site | Contract and enforcement |
| --- | --- |
| `tobytes.rs` trait implementations and slice views | Primitive/scalar-math representations are initialized without padding; arrays/slices inherit the explicit unsafe trait contract. Views borrow the complete initialized allocation. Safe material setters copy representations before their graphics scope. |
| `quad_gl.rs` uniform byte views | Private initialized `Vec<u32>` backing guarantees backend alignment. Checked metadata bounds every byte slice/upload to the backend's signed limit. |
| `texture.rs` screen-copy helper | An active OpenGL owner and live RGBA8 single-sample texture precede raw-ID lookup/GL calls. The helper restores the actual texture binding, preserving Miniquad's cache. GPU calls receive valid scalar output pointers, no retained Rust references. |
| `audio.rs` WASM source adapter | Encoded byte length fits u32; paired imports copy input synchronously, own IDs, publish explicit terminal states and ignore late completion after deletion. No private quad-snd field access or representation casts. |
| Test-only backend trait method | `test_backend.rs::texture_raw_id` is required by Miniquad's unsafe trait method signature; its body rejects raw GPU access and never dereferences a pointer. |

There are no mutable statics, raw zone/scene pointers, fabricated lifetimes or
unchecked custom slot-map access in Macroquad's supported source. `exec` uses a
standard `Wake` implementation rather than a custom raw-waker vtable.

Miniquad and quad-snd still contain foreign/platform code. This review covers
the reachable supported paths, not every platform in their source trees.
Miniquad is private behind the facade; initialization and backend access are
confined above. The narrow vendored fixes cover deletion-cache invalidation,
non-owning render passes, checked/null-safe X11 property copies and clipboard
state, native-long XDND format-32 writes, null-safe GL version discovery, WebGL
capability detection and WebGL2 depth formats. [Vendor provenance](../vendor/README.md) records exact patches and
reconstruction checks. Texture transfers/readback and uniform metadata are
validated in Macroquad before their unsafe dependency boundaries.

The browser host must implement the paired import ABI, copy borrowed input
before returning, write only specified output ranges, and dispatch window events
as event-loop callbacks rather than recursively reentering a live handler. The
pinned runtime/import gate and real-browser fixtures validate that supported
host. Arbitrary modified host functions, GL state mutation through another
backend, and direct use of Miniquad's unrestricted APIs are outside this facade.
Native audio retains quad-snd's existing decoder errors/panics; this change does
not claim a new native codec implementation or an audit of all dependencies.

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
source is deleted once and remains deleted after late callbacks. It captures the
actual decoding context, establishes a controlled suspended state, and requires
a trusted WebDriver gesture followed by the runtime's real `resume()` call and
a running `statechange` before reporting completion. This tests resumption from
controlled suspension, not every browser autoplay policy or audio device. Run it with
`tests/ui_material_browser.py --fixture tests/audio_lifecycle_regression.html
--marker "audio lifecycle regression: passed" --runtime js/mq_js_bundle.js
--output target/audio-lifecycle-browser --unlock-audio --timeout 90` after building the example
for WASM with audio (quad-snd requires the existing WASM `--allow-undefined`
linker setting). The same runner preserves the existing UI regression by default.

Graphics ownership CPU/Miri entry points are `ownership::tests`,
`texture::ownership_tests`, `telemetry::tests` and the existing buffer/lifecycle
helpers. The real clock in telemetry requires `-Zmiri-disable-isolation`; no
production helper is substituted for Miri and leak checking remains enabled.
Atlas CPU tests release their ordinary owned values rather than deliberately
leaking them to avoid old global-context destructors.
`examples/graphics_ownership_regression.rs` and
`tests/graphics_ownership_regression.html` cover camera/UI/uniform reentry,
input subscriber growth during touch/mouse replay, target/attachment clones,
queued/discarded draw leases, material/capture lifetimes and complete shutdown.
The browser census checks temporary textures return to baseline, then reach zero
on shutdown. Run the generic browser runner with that fixture, marker
`graphics ownership regression: passed`, the paired `--runtime`, and an isolated
`--output` directory. Native execution also exercises MSAA/depth and rejects
cross-thread access/concurrent window initialization. Browser render tests need
an active visible tab; the runner explicitly selects X11 in the Firefox driver
process environment so a host Wayland session cannot move it off the isolated
test display. It records visibility and actual animation frames without replacing
browser scheduling.


The retained core interpreter runs used nightly-2026-10-16 with57 passing tests
and one existing ignore on each of x86_64/i686; leak checking stayed enabled.
Actual-helper X11/XDND Miri and real native/browser FFI fixtures supplement that
CPU evidence. The native-long writer's old allocation failed the interpreter
before correction; no fabricated replacement helper supplied that regression.
These checks address the supported fork contracts; they do not declare an
official upstream patched version, context restoration, arbitrary foreign hosts
or dormant human-voice relay paths supported.
