# Zubspace Macroquad 0.5.1

Prepared GitHub-only release of package `macroquad` 0.5.1, tag
`zubspace-v0.5.1`, source commit `@COMMIT@`. This source template does not publish
or create a tag. Publication requires final local acceptance, green public CI
on the exact reviewed commit and a separate release decision. The immutable
0.5.0 release, notes, assets and historical game acceptance remain unchanged.
Upstream Macroquad stays 0.4.16; vendored Miniquad stays 0.4.11. Rust 1.88+ is the
compiler requirement, separate from the runtime extension versions below.
Original licenses and the complete reconstructible vendor patch are retained.

## Upgrade Rust and JavaScript together

Pin the full commit from the generated `release-manifest.json` and use its
matching `mq_js_bundle.js`; Cargo does not install JavaScript:

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
macroquad = { git = "https://github.com/pkieltyka/macroquad", rev = "@COMMIT@", default-features = false }

[target.'cfg(target_arch = "wasm32")'.dependencies]
macroquad = { git = "https://github.com/pkieltyka/macroquad", rev = "@COMMIT@", default-features = false, features = ["audio"] }
```

Verify asset checksums, copy the paired runtime locally before `load("app.wasm")`,
and upgrade/roll back the Rust pin, resolved lockfile and JS together. The runtime
SHA-256 is `3a262ff4f74cc051b49d4b487ea136154fbd5c8e71a8053ff33785bc7de74d3e`.
Source provenance keeps local package sources null; the generated manifest binds
concrete Git sources and the exact commit without a self-referential source SHA.
The source/runtime archives retain the generator, upstream base and licenses.

Run `node tools/check_browser_runtime.mjs --wasm path/to/app.wasm` against the
resolved lockfile before startup (`--no-audio` only if actual audio imports are
absent). The offline checker validates the exact `env.init_webgl(i32,i32)->void`
initializer signature, implementations and ABI exports. The WebGL selection
extension is now v2; Miniquad ABI2 and plugin/audio ABI1 remain unchanged. Current
Rust paired with a v1 runtime fails explicitly before context/GPU creation.
The reverse direction differs: already compiled 0.5.0 WASM checks for extension 1
before calling its initializer, so pairing it with this v2 runtime traps in that
old guard; this cannot acquire a new diagnostic from the new initializer. The
offline checker rejects that stale initializer with an actionable error. Browser
"old-v1" negatives use current WASM with a v1 runtime guard override and test the
first direction; they do not establish the old-binary diagnostic in reverse.

## Rendering and capacity migration

`sample_count <= 1` now explicitly requests drawing-buffer antialiasing off,
including the default 1; values >1 request it on. This corrects the previously
ignored setting and can change edge appearance. The browser chooses actual
technique/sample count and may decline AA. Auto still tries WebGL2 then WebGL1
only on context-creation failure; explicit versions stay strict. Alpha,
premultiplication and other context defaults are unchanged. Applications that
want the 0.5.0 browser look must request it: `sample_count: 4` reproduced 0.5.0
framebuffers byte for byte in Chrome and Firefox, WebGL1 and WebGL2 (see
[measurements](#measured-browser-cost)).

Public mesh submissions remain atomic. Oversized calls or local indices outside
the vertex slice panic with a semantic message when `debug_assertions` are on.
That includes the default dev profile, so a plain `cargo build` for WASM stops the
module on the first such call. Otherwise the whole call is rejected and a warning
with submitted counts, effective limits and remediation is logged once per
distinct reason. No topology is clipped. Empty indices are a no-op.
Effective vertex ceilings are 65,535 on actual WebGL2 (index 65,535 is fixed
primitive restart), otherwise 65,536; index count remains independently bounded.
Submit smaller valid meshes or increase configured capacities within those limits.

Oversized built-in 2D triangles split privately only with the default pipeline,
preserving every Vertex attribute/order. Arc chunks are generated before u16
source indices wrap. Every custom material, including custom triangle pipelines
using `gl_VertexID` or `_ScreenTexture`, keeps the original direct submission
when it fits and atomic rejection otherwise. Splitting default built-ins can
produce multiple draws/capture snapshots. Public meshes are not automatically
split. Default-material shapes split; textured quads and text do not (they need
4 vertices/6 indices). Nonempty built-in UI requires at least 3 vertices/3 indices,
is rejected under the same debug/release policy below that, and splits its private
triangle lists above it. End-frame UI uses screen projection while retaining the
game's camera.

`gl_set_drawcall_buffer_capacity` validates first, flushes pending geometry using
the current camera/pass/viewport/model/scissor, then resizes. Invalid requests
neither flush nor change capacities. This replaces the earlier queued-work panic;
callers should account for the flush boundary.

## Draw submission

Each flush now uploads its geometry once into shared vertex/index buffers and
draws every call at an index offset, instead of giving every draw-call slot its
own full-capacity buffer pair and re-uploading per call. Calls are grouped so
segment-relative u16 indices stay below the effective vertex limit, and the
shared buffers grow with actual use. Consecutive calls into the same target share
one render pass. Unchanged pipeline, viewport, scissor, bindings and uniform
values are not re-sent, and Miniquad no longer re-selects an already active
texture unit or reassigns sampler units on every bind. Draw order, state and
pixels are unchanged. Screen-texture snapshots, telemetry capture and calls that
sample their own MSAA target's (resolve) texture still see every earlier call.
The unsupported Metal backend keeps one call per upload and pass state.

Previously, a frame with N draw calls kept N full-capacity buffer pairs alive:
about 410 KB each at the default 10,000/5,000 capacity, so roughly 140 MB for a
350-call frame. Memory now tracks the largest flush actually submitted.
Per-sprite work also avoids a backend size query, an owner check and an atlas
hash lookup when no texture atlas was built, and skips `sin`/`cos` for
unrotated sprites.

## Input behavior

Mouse presses originating on the canvas receive one owned release through the
owning window, including outside the canvas. Release coordinates remain relative
to the canvas (and can lie outside it); pointer-lock relative/raw motion is retained.
Blur/hidden cleanup precedes the deduplicated focus notification. Unrelated and
duplicate releases are ignored.

Physical and simulated Left holds combine; an ending source cannot erase the
other hold. Cancelled contacts terminate bookkeeping without adopting a new
primary finger. Legacy first genuine Ended behavior is retained even when other
contacts survive. Disabling simulation clears its hold while preserving a physical hold; blur
terminates both sources. Touch wake works with simulation on or off.

If a terminal touch event is lost, a later authoritative `TouchEvent.touches`
list cancels only missing IDs whose Started this canvas owned, at their last
known positions. Explicit changes are delivered first and new Started owners
are registered before reconciliation; unrelated listed contacts are never
adopted. A list is authoritative only when it agrees with the event's own
changes (started/moved contacts listed, ended ones absent). Synthetic events built
without one, which default to an empty list, therefore no longer cancel their own
contact. No missing-event detection is possible without a later hint. Tests label
synthetic touch sequences explicitly; they are not physical-touch qualification.
A context-menu workaround is deferred: trusted headed Chrome/Firefox right-click/
Escape sequences released Right in the tested matrix; Firefox Shift-right omitted
contextmenu. DOM events do not prove a visible native popup, and no global menu
suppression or speculative Left/Middle release is introduced.

## Measured browser cost

A generated Zubspace-shaped scene was measured on 2026-10-17 in headless
Chrome 155 and Firefox 158 on Linux, pinned to an AMD Radeon RX 9060 XT. The
scene has a two-layer starfield over 7 star textures, about 2,700 map tiles,
sprites, per-label camera switches and a radar/HUD. At 1920×1080 that is 371 draw
calls and about 4,500 sprites per frame. Each cell pooled 2,700 frames over 3
interleaved rounds. The page timed the whole `requestAnimationFrame` callback
(Rust plus WebGL submission on the main thread). GPU time came from
`EXT_disjoint_timer_query`, which Firefox exposes only for WebGL1.

Median CPU milliseconds per frame. 0.5.0 always antialiased; 0.5.1 uses `sample_count: 4`:

| Browser / context | 0.5.0 | 0.5.1 candidate `7f6ec9b` | 0.5.1 final | vs 0.5.0 |
| --- | --- | --- | --- | --- |
| Chrome WebGL2 | 1.72 | 1.35 | 0.68 | −60% |
| Chrome WebGL1 | 1.78 | 1.36 | 0.65 | −63% |
| Firefox WebGL2 | 2.54 | 2.34 | 1.30 | −49% |
| Firefox WebGL1 | 2.58 | 2.36 | 1.22 | −53% |

- **Calls and spikes:** WebGL calls per frame fell from 11,649 to 1,229, with the same 371 draw calls. 95th-percentile frames fell proportionally (Chrome WebGL2: 2.08 → 0.86 ms).
- **Pixels:** every framebuffer was byte-identical to 0.5.0 in each browser.
- **GPU time:** unchanged at about 1.1 ms.
- **Antialiasing cost:** requesting it added about 0.12 ms (Chrome) and 0.15 ms (Firefox) of GPU time per frame. A 2560×1440 check (518 draw calls) showed +0.22 ms in Chrome, and candidate → final CPU time of 1.92 → 0.84 ms (Chrome) and 3.20 → 1.52 ms (Firefox).

These results come from one desktop GPU and CPU (a Ryzen 9 7950X), not other operating systems, mobile devices or integrated graphics. Weaker CPUs should benefit proportionally more from the CPU reductions, but that was not measured.

## Evidence and limits

Boxed scoped ownership removes large Context/UI copies from measured optimized
WASM draw/UI paths while preserving scoped borrowing, unwinding and destruction.
An external generated-scene comparison of `e753c60` (inline) with `cb88c6e` (boxed)
found mean CPU scene-API issuance reductions of 17.91–41.47% across the eight
browser/GL/variant aggregates. Four fixed A-before-B rounds used 216 measured
samples per browser/GL/variant/revision; all 32 round aggregates improved, not
every individual sample. Recorded PNG pixels matched and GL checks were clean.
Only executable ownership source differed (plan prose also differed). This used
Linux Chrome SwiftShader/Firefox software GL and a historical runtime, excluding
deferred end-frame submission and GPU work. It establishes neither whole-frame
FPS, physical-GPU benefit nor application performance. See the qualified
[evidence/checklist](../../plans/0.5.1-browser-efficiency.md).

Final acceptance must use `make release-check` and `make test-ci` on the reviewed
clean candidate, plus exact-commit public supported/MSRV/Miri CI before publication.
The complete local gate includes strict actual audio suspension/trusted gesture/
resume and source cleanup, input/UI/lifecycle/codec/resource/pixel checks and
diagnostic scene trials. CI deliberately runs bounded real graphics smoke and
does not require audio hardware or full timing/lifecycle acceptance. Ten repeated
Firefox attribute matrices are a separate local stability gate; zero observed failures
cannot prove zero flakiness. Final receipts remain separate from these prepared
notes until they have actually run.

Cross-OS/browser-device/physical-GPU/hardware-audio qualification, context
restoration and GPU timing remain unavailable or pending. No game files/assets,
adoption, deployment or renewed game acceptance are part of this release work.
The ttf-parser advisory remains; this is not an official RustSec patched release
or a clean-audit claim. See [fork migration](../fork.md),
[runtime provenance](../browser-runtime.md), [security limits](../security.md) and
[patch attribution](../patch-ledger.md).
